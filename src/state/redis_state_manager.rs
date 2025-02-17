use crate::models::context::Context;
use crate::models::dag::{DAGInstance, DAGTemplate};
use crate::models::parameters::ParameterSet;
use crate::models::task::{TaskInstance, TaskTemplate};
use crate::orchestrator::orchestrator::evaluate_graph;
use crate::state::state_manager::{AgentStatus, StateManager};
use async_trait::async_trait;
use futures_util::stream::StreamExt;
use deadpool_redis::{Config as RedisConfig, Pool, Runtime};
use deadpool_redis::redis::{self, AsyncCommands};
use redis::aio::PubSub;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::log::warn;
use tracing::{debug, error, info};
use crate::utils::constants::PENDING_STATUS;

pub struct RedisStateManager {
    client: redis::Client,
    pool: Pool,
    cluster_id: String,
}

impl RedisStateManager {
    pub async fn new(redis_url: &str, cluster_id: &String) -> Self {
        let mut cfg = RedisConfig::from_url(redis_url);
        // Set the maximum pool size if desired:
        let pool = cfg
            .create_pool(Some(Runtime::Tokio1))
            .expect("Failed to create Redis pool");
        pool.resize(16);
        RedisStateManager {
            client: redis::Client::open(redis_url).expect("Failed to create Redis client"),
            pool,
            cluster_id: cluster_id.clone(),
        }
    }

    async fn get_connection(&self) -> deadpool_redis::Connection {
        self.pool.get().await.expect("Failed to get Redis connection")
    }

    async fn get_pubsub_connection(&self) -> PubSub {
        // Use a direct async connection from the underlying client.
        self.client
            .get_async_pubsub()
            .await
            .expect("Failed to get Redis connection")
    }

    // Key builders
    fn build_task_key(&self, task_id: &str) -> String {
        format!("Cyclonetix:{}:task:{}", self.cluster_id, task_id)
    }
    fn build_context_key(&self, context_id: &str) -> String {
        format!("Cyclonetix:{}:context:{}", self.cluster_id, context_id)
    }
    fn build_parameter_set_key(&self, parameter_set_id: &str) -> String {
        format!(
            "Cyclonetix:{}:parameter_set:{}",
            self.cluster_id, parameter_set_id
        )
    }
    fn task_instance_key(&self, run_id: &str) -> String {
        format!("Cyclonetix:{}:task_instance:{}", self.cluster_id, run_id)
    }
    fn dag_status_key(&self, dag_run_id: &str) -> String {
        format!("Cyclonetix:{}:dag_status:{}", self.cluster_id, dag_run_id)
    }
    fn dag_execution_key(&self, dag_run_id: &str) -> String {
        format!("Cyclonetix:{}:dag_execution:{}", self.cluster_id, dag_run_id)
    }
    fn dag_execution_set_key(&self) -> String {
        format!("Cyclonetix:{}:dag_execution", self.cluster_id)
    }
    fn agent_key(&self, agent_id: &str) -> String {
        format!("Cyclonetix:{}:agent:{}", self.cluster_id, agent_id)
    }
    fn agent_tasks_key(&self, agent_id: &str) -> String {
        format!("Cyclonetix:{}:agent:{}:tasks", self.cluster_id, agent_id)
    }
    fn agents_key(&self, agent_id: &str) -> String {
        format!("Cyclonetix:{}:agents:{}", self.cluster_id, agent_id)
    }
    fn agents_set_key(&self) -> String {
        format!("Cyclonetix:{}:agents", self.cluster_id)
    }
}

#[async_trait]
impl StateManager for RedisStateManager {
    async fn get_work_from_queue(&self, queue: &str) -> Option<(String, String)> {
        let mut conn = self.get_connection().await;

        if let Ok(queue_item) = conn.rpop::<_, String>(queue, None).await {
            let parts: Vec<&str> = queue_item.split('|').collect();
            if parts.len() == 2 {
                return Some((parts[0].to_string(), parts[1].to_string()));
            } else {
                error!(
                    "Malformed queue item: {} (Expected task_run_id|dag_run_id)",
                    queue_item
                );
            }
        }
        None
    }

    async fn store_task(&self, task: &TaskTemplate) {
        let mut conn = self.get_connection().await;
        let key = Self::build_task_key(self, &task.id);
        let _: () = conn
            .set(key, serde_json::to_string(task).unwrap())
            .await
            .unwrap();
    }

    async fn get_task_definition(&self, task_id: &str) -> Option<TaskTemplate> {
        let mut conn = self.get_connection().await;
        let key = Self::build_task_key(self, task_id);

        if let Ok(task_json) = conn.get::<_, String>(key).await {
            serde_json::from_str(&task_json).ok()
        } else {
            None
        }
    }

    async fn get_all_tasks(&self) -> Vec<TaskTemplate> {
        let mut conn = self.get_connection().await;
        let keys: Vec<String> = conn.keys("task:*").await.unwrap_or_else(|_| vec![]);
        let mut tasks = Vec::new();
        for key in keys {
            if let Ok(task_json) = conn.get::<_, String>(key).await {
                if let Ok(task) = serde_json::from_str::<TaskTemplate>(&task_json) {
                    tasks.push(task);
                }
            }
        }
        tasks
    }

    async fn enqueue_task_instance(&self, task_instance: &TaskInstance, dag_run_id: &str) {
        let mut conn = self.get_connection().await;

        let queue_item = format!("{}|{}", task_instance.run_id, dag_run_id); // Store both task & dag run IDs

        conn.lpush::<String, String, ()>(task_instance.queue.clone(), queue_item.clone())
            .await
            .unwrap();

        debug!(
            "Task Instance {} added to queue {} for DAG {}",
            task_instance.run_id, task_instance.queue, dag_run_id
        );
    }

    async fn get_task_status(&self, task_instance_run_id: &str) -> Option<String> {
        let mut conn = self.get_connection().await;
        let key = Self::task_instance_key(self, task_instance_run_id);
        conn.get(key).await.unwrap_or(None)
    }

    async fn update_task_status(&self, task_instance_run_id: &str, status: &str) {
        let mut conn = self.get_connection().await;
        let key = Self::task_instance_key(self, task_instance_run_id);
        conn.set::<String, String, ()>(key, status.to_string())
            .await
            .unwrap();
    }

    async fn is_task_scheduled(&self, task_instance_run_id: &str) -> bool {
        let mut conn = self.get_connection().await;
        let queue_key = "work_queue".to_string();
        let instance_id = task_instance_run_id.to_string();

        let queue_items: Vec<String> = conn.lrange(queue_key, 0, -1).await.unwrap_or_default();
        queue_items.contains(&instance_id)
    }

    async fn store_context(&self, context: &Context) {
        let mut conn = self.get_connection().await;
        let key = Self::build_context_key(self, &context.id);
        let _: () = conn
            .set(key, serde_json::to_string(context).unwrap())
            .await
            .unwrap();
    }

    async fn get_context(&self, context_id: &str) -> Option<Context> {
        let mut conn = self.get_connection().await;
        if let Ok(context_json) = conn.get::<_, String>(context_id).await {
            serde_json::from_str(&context_json).ok()
        } else {
            None
        }
    }

    async fn store_parameter_set(&self, parameter_set: &ParameterSet) {
        let mut conn = self.get_connection().await;
        let key = Self::build_parameter_set_key(self, &parameter_set.id);
        let _: () = conn
            .set(key, serde_json::to_string(parameter_set).unwrap())
            .await
            .unwrap();
    }

    // Remove a completed DAG execution
    async fn remove_scheduled_dag(&self, run_id: &str) {
        let mut conn = self.get_connection().await;

        // Try to remove by run_id directly
        let removed: i32 = conn.hdel(self.dag_execution_set_key(), run_id).await.unwrap_or(0);

        if removed > 0 {
            info!("Successfully removed scheduled DAG: {}", run_id);
        } else {
            tracing::warn!("Scheduled DAG {} not found in Redis.", run_id);
        }
    }

    async fn get_dag_status(&self, run_id: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut conn = self.get_connection().await;
        let key = Self::dag_status_key(self, run_id);
        // Map any error to a boxed error.
        let status: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(status.unwrap_or_else(|| PENDING_STATUS.to_string()))
    }

    async fn get_scheduled_dags(&self) -> Vec<DAGInstance> {
        let mut conn = self.get_connection().await;
        let dag_map: HashMap<String, String> =
            conn.hgetall(self.dag_execution_set_key()).await.unwrap_or_default();

        let parsed_dags: Vec<DAGInstance> = dag_map
            .values()
            .filter_map(|dag| match serde_json::from_str::<DAGInstance>(dag) {
                Ok(d) => Some(d),
                Err(e) => {
                    error!(
                        "Failed to deserialize DAG execution: {} | Error: {:?}",
                        dag, e
                    );
                    None
                }
            })
            .collect();

        tracing::debug!("Parsed DAG executions: {:?}", parsed_dags);
        parsed_dags
    }

    async fn store_dag_execution(&self, dag_execution: &DAGInstance) {
        let mut conn = self.get_connection().await;

        let dag_json = serde_json::to_string(dag_execution).unwrap();
        conn.hset::<&str, &str, String, ()>(self.dag_execution_set_key().as_str(), &dag_execution.run_id, dag_json)
            .await
            .unwrap();

        // Set initial DAG execution status
        let status_key = self.dag_status_key(&dag_execution.run_id);
        conn.set::<String, String, ()>(status_key, PENDING_STATUS.to_string())
            .await
            .unwrap();

        debug!("Stored DAG execution: {}", dag_execution.run_id);
    }

    async fn get_dag_execution(&self, run_id: &str) -> Option<DAGInstance> {
        let mut conn = self.get_connection().await;
        let key = self.dag_execution_set_key();

        match conn.hget::<_, _, String>(key, run_id).await {
            Ok(dag_json) => match serde_json::from_str::<DAGInstance>(&dag_json) {
                Ok(dag) => {
                    tracing::debug!("Successfully retrieved DAG Execution: {:?}", dag);
                    Some(dag)
                }
                Err(e) => {
                    error!("Failed to deserialize DAGExecution for {}: {:?}", run_id, e);
                    None
                }
            },
            Err(e) => {
                error!("Failed to retrieve DAGExecution from Redis: {:?}", e);
                None
            }
        }
    }

    async fn store_dag_definition(&self, dag: &DAGTemplate) {
        let mut conn = self.get_connection().await;
        let dag_json = serde_json::to_string(dag).unwrap();
        conn.set::<String, String, ()>(format!("dag_definitions:{}", dag.id), dag_json)
            .await
            .unwrap();
    }

    async fn get_dag(&self, dag_id: &str) -> Option<DAGTemplate> {
        let mut conn = self.get_connection().await;
        let dag: Option<String> = conn
            .get(format!("dag_definitions:{}", dag_id))
            .await
            .unwrap_or(None);
        dag.and_then(|d| serde_json::from_str(&d).ok())
    }

    async fn cleanup_dag_execution(
        &self,
        dag_run_id: &str,
        task_run_ids: &[String],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.remove_scheduled_dag(dag_run_id).await;

        let mut conn = self.get_connection().await;
        let mut pipe = redis::pipe();

        // Delete the dag_status key.
        let dag_status_key = self.dag_status_key(dag_run_id);
        pipe.del(dag_status_key);

        // Delete each task_instance key.
        for task_run_id in task_run_ids {
            let task_instance_key = Self::task_instance_key(self, task_run_id);
            pipe.del(task_instance_key);
        }

        // Execute the pipeline and convert any error to a boxed error.
        pipe.query_async::<()>(&mut conn)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        info!("Cleaned up execution keys for DAG: {}", dag_run_id);
        Ok(())
    }

    async fn notify_graph_update(&self, dag_run_id: &str) {
        let mut conn = self.get_connection().await;
        conn.publish::<String, String, ()>("graph_update".to_string(), dag_run_id.to_string())
            .await
            .unwrap();
        debug!("Published graph update for DAG: {}", dag_run_id);
    }

    async fn listen_for_graph_updates(self: Arc<Self>) {
        let mut pubsub_conn = self.get_pubsub_connection().await;
        if let Err(e) = pubsub_conn.subscribe("graph_update").await {
            error!("Failed to subscribe to graph updates: {}", e);
            return;
        }
        info!("Subscribed to graph updates.");
        let mut message_stream = pubsub_conn.into_on_message();
        while let Some(msg) = message_stream.next().await {
            let dag_run_id: String = match msg.get_payload() {
                Ok(id) => id,
                Err(e) => {
                    error!("Failed to get graph update payload: {}", e);
                    continue;
                }
            };
            info!("Graph update received for DAG: {}", dag_run_id);
            if let Some(dag_execution) = self.get_dag_execution(&dag_run_id).await {
                evaluate_graph(self.clone(), &dag_execution).await;
            } else {
                error!("No DAG execution found for run_id: {}", dag_run_id);
            }
        }
    }

    async fn get_orchestrator_count(&self) -> Result<u32, Box<dyn Error + Send + Sync>> {
        let mut conn = self.get_connection().await;
        let count: Option<u32> = conn
            .get("orchestrator_count")
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(count.unwrap_or(1))
    }

    async fn get_orchestrator_id(&self) -> Result<u32, Box<dyn Error + Send + Sync>> {
        let mut conn = self.get_connection().await;
        let id: Option<u32> = conn
            .get("orchestrator_id")
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(id.unwrap_or(0))
    }

    async fn register_agent(&self, agent_id: &str) {
        let mut conn = self.get_connection().await;
        let key = self.agent_key(agent_id);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let _: () = conn.hset(key.clone(), "last_heartbeat", timestamp).await.unwrap();
        // Add the agent to the global set of agents.
        let agents_key = self.agents_set_key();
        let _: () = conn.sadd(agents_key, agent_id).await.unwrap();
        info!("Registered agent {}", agent_id);
    }

    async fn update_agent_heartbeat(&self, agent_id: &str) {
        let mut conn = self.get_connection().await;
        let key = Self::agent_key(self, agent_id);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let _: () = conn.hset(key, "last_heartbeat", timestamp).await.unwrap();
        debug!("Updated heartbeat for agent {}", agent_id);
    }

    async fn assign_task_to_agent(&self, agent_id: &str, assignment: &str) {
        let mut conn = self.get_connection().await;
        let key = Self::agent_tasks_key(self, agent_id);
        let _: () = conn.sadd(key, assignment).await.unwrap();
        debug!("Assigned task {} to agent {}", assignment, agent_id);
    }

    async fn remove_task_from_agent(&self, agent_id: &str, assignment: &str) {
        let mut conn = self.get_connection().await;
        let key = Self::agent_tasks_key(self, agent_id);
        let _: () = conn.srem(key, assignment).await.unwrap();
        debug!("Removed task {} from agent {}", assignment, agent_id);
    }

    async fn get_all_agents(&self) -> Vec<AgentStatus> {
        let mut conn = self.get_connection().await;
        let agent_ids: Vec<String> = conn.smembers(self.agents_set_key()).await.unwrap_or_default();
        let mut agents = Vec::new();
        for agent_id in agent_ids {
            let key = Self::agent_key(self, &agent_id);
            let heartbeat: Option<i64> = conn.hget(&key, "last_heartbeat").await.unwrap_or(None);
            let tasks: Vec<String> = conn
                .smembers(Self::agent_tasks_key(self, &agent_id))
                .await
                .unwrap_or_default();
            if let Some(ts) = heartbeat {
                agents.push(AgentStatus {
                    agent_id: agent_id.clone(),
                    last_heartbeat: ts,
                    tasks,
                });
            }
        }
        agents
    }

    async fn get_queue_tasks(&self, queue: &str) -> Vec<String> {
        let mut conn = self.get_connection().await;
        conn.lrange(queue, 0, -1).await.unwrap_or_default()
    }

    // --- New Method for DAG Visualization ---
    async fn get_dag_visualization(&self, dag_run_id: &str) -> Option<String> {
        // Retrieve the DAG execution.
        if let Some(dag_execution) = self.get_dag_execution(dag_run_id).await {
            // Build a JSON structure representing nodes and edges.
            // For simplicity, nodes include: run_id, task_id, and current status.
            // Edges are determined from each task's dependencies.
            let mut nodes = Vec::new();
            let mut edges = Vec::new();
            for task in &dag_execution.tasks {
                let status = self
                    .get_task_status(&task.run_id)
                    .await
                    .unwrap_or_else(|| "unknown".to_string());
                nodes.push(serde_json::json!({
                    "run_id": task.run_id,
                    "task_id": task.task_id,
                    "status": status,
                }));
                for dep in &task.dependencies {
                    edges.push(serde_json::json!({
                        "from": dep,
                        "to": task.run_id,
                    }));
                }
            }
            let graph = serde_json::json!({
                "nodes": nodes,
                "edges": edges,
            });
            return Some(graph.to_string());
        }
        None
    }

    async fn reset_tasks_from_downed_agents(
        &self,
        heartbeat_threshold: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let current_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        // Get all agents with their statuses.
        let agents = self.get_all_agents().await;
        for agent in agents {
            if current_time - agent.last_heartbeat > heartbeat_threshold as i64 {
                info!("Agent {} is down. Resetting its tasks.", agent.agent_id);
                let mut conn = self.get_connection().await;
                let key = Self::agent_tasks_key(self, &agent.agent_id);
                // Get the composite assignment strings.
                let assignments: Vec<String> = conn.smembers(&key).await.unwrap_or_default();
                for assignment in assignments {
                    // Each assignment is in the form "task_run_id|dag_run_id"
                    let parts: Vec<&str> = assignment.split('|').collect();
                    if parts.len() != 2 {
                        warn!(
                            "Malformed assignment {} for agent {}",
                            assignment, agent.agent_id
                        );
                        continue;
                    }
                    let task_run_id = parts[0];
                    let dag_run_id = parts[1];
                    // Retrieve the DAG execution.
                    if let Some(dag_execution) = self.get_dag_execution(dag_run_id).await {
                        if let Some(task_instance) =
                            dag_execution.tasks.iter().find(|t| t.run_id == task_run_id)
                        {
                            info!(
                                "Resetting task {} from downed agent {}",
                                task_run_id, agent.agent_id
                            );
                            // Update task status to "pending".
                            self.update_task_status(task_run_id, PENDING_STATUS).await;
                            // Re-enqueue the task.
                            self.enqueue_task_instance(task_instance, dag_run_id).await;
                        } else {
                            warn!(
                                "Task instance {} not found in DAG {} for downed agent {}",
                                task_run_id, dag_run_id, agent.agent_id
                            );
                        }
                    } else {
                        warn!(
                            "DAG execution {} not found while resetting task {} for downed agent {}",
                            dag_run_id, task_run_id, agent.agent_id
                        );
                    }
                    // Remove the assignment from the agent.
                    let _: () = conn.srem(&key, assignment).await?;
                }
                // Optionally remove the dead agent from the global set.
                let _: () = conn.srem(self.agents_set_key(), &agent.agent_id).await?;
                let agent_key = Self::agent_key(self, &agent.agent_id);
                let _: () = conn.del(&agent_key).await?;
                debug!("Agent {} removed from active agents.", agent.agent_id);
            }
        }
        Ok(())
    }
}
