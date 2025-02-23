use crate::models::context::Context;
use crate::models::dag::{DagInstance, DagTemplate, GraphInstance};
use crate::models::task::{TaskInstance, TaskTemplate};
use crate::state::state_manager::{AgentStatus, StateManager, TaskPayload};
use crate::utils::constants::{DEFAULT_QUEUE, PENDING_STATUS};
use async_trait::async_trait;
use deadpool_redis::redis::{self, AsyncCommands};
use deadpool_redis::{Config as RedisConfig, Pool, Runtime};
use futures_util::stream::StreamExt;
use redis::aio::PubSub;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

pub struct RedisStateManager {
    client: redis::Client,
    pool: Pool,
    cluster_id: String,
}

impl RedisStateManager {
    pub async fn new(redis_url: &str, cluster_id: &str) -> Self {
        let cfg = RedisConfig::from_url(redis_url);
        let pool = cfg
            .create_pool(Some(Runtime::Tokio1))
            .expect("Failed to create Redis pool");
        pool.resize(16);
        RedisStateManager {
            client: redis::Client::open(redis_url).expect("Failed to create Redis client"),
            pool,
            cluster_id: String::from(cluster_id),
        }
    }

    async fn get_connection(&self) -> deadpool_redis::Connection {
        self.pool
            .get()
            .await
            .expect("Failed to get Redis connection")
    }

    async fn get_pubsub_connection(&self) -> PubSub {
        self.client
            .get_async_pubsub()
            .await
            .expect("Failed to get Redis pubsub connection")
    }

    // Key builders for immutable GraphInstance
    fn graph_status_key(&self, graph_run_id: &str) -> String {
        format!(
            "Cyclonetix:{}:graph_status:{}",
            self.cluster_id, graph_run_id
        )
    }
    fn graph_instance_set_key(&self) -> String {
        format!("Cyclonetix:{}:graph_instance", self.cluster_id)
    }
    // Key builders for mutable DagInstance
    fn dag_instance_set_key(&self) -> String {
        format!("Cyclonetix:{}:dag_instance", self.cluster_id)
    }
    fn dag_status_key(&self, dag_run_id: &str) -> String {
        format!("Cyclonetix:{}:dag_status:{}", self.cluster_id, dag_run_id)
    }
    fn task_instance_key(&self, run_id: &str) -> String {
        format!("Cyclonetix:{}:task_instance:{}", self.cluster_id, run_id)
    }
    fn build_task_key(&self, task_id: &str) -> String {
        format!("Cyclonetix:{}:task:{}", self.cluster_id, task_id)
    }
    fn build_context_key(&self, context_id: &str) -> String {
        format!("Cyclonetix:{}:context:{}", self.cluster_id, context_id)
    }
    fn agent_key(&self, agent_id: &str) -> String {
        format!("Cyclonetix:{}:agent:{}", self.cluster_id, agent_id)
    }
    fn agent_tasks_key(&self, agent_id: &str) -> String {
        format!("Cyclonetix:{}:agent:{}:tasks", self.cluster_id, agent_id)
    }
    fn agents_set_key(&self) -> String {
        format!("Cyclonetix:{}:agents", self.cluster_id)
    }
    fn queue_key(&self, agent_id: &str) -> String {
        format!("Cyclonetix:{}:queue:{}", self.cluster_id, agent_id)
    }
}

#[async_trait]
impl StateManager for RedisStateManager {
    async fn get_work_from_queue(&self, queue: &str) -> Option<TaskPayload> {
        let mut conn = self.get_connection().await;
        let queue_key = self.queue_key(queue);
        if let Ok(queue_item) = conn.rpop::<_, String>(queue_key, None).await {
            match serde_json::from_str::<TaskPayload>(&queue_item) {
                Ok(payload) => Some(payload),
                Err(e) => {
                    error!(
                        "Failed to parse task payload from queue: {}. Error: {:?}",
                        queue_item, e
                    );
                    None
                }
            }
        } else {
            None
        }
    }

    async fn put_work_on_queue(&self, task_payload: &TaskPayload, queue: &str) {
        let mut conn = self.get_connection().await;
        let payload_json = serde_json::to_string(task_payload).unwrap();
        let queue_key = self.queue_key(queue);
        let _: () = conn.lpush(queue_key, payload_json).await.unwrap();
        debug!(
            "Enqueued task payload {} to queue {}",
            task_payload.task_run_id, queue
        );
    }

    async fn save_task(&self, task: &TaskTemplate) {
        let mut conn = self.get_connection().await;
        let key = Self::build_task_key(self, &task.id);
        let _: () = conn
            .set(key, serde_json::to_string(task).unwrap())
            .await
            .unwrap();
    }

    async fn load_task(&self, task_id: &str) -> Option<TaskTemplate> {
        let mut conn = self.get_connection().await;
        let key = Self::build_task_key(self, task_id);
        if let Ok(task_json) = conn.get::<_, String>(key).await {
            serde_json::from_str(&task_json).ok()
        } else {
            None
        }
    }

    async fn load_all_tasks(&self) -> Vec<TaskTemplate> {
        let mut conn = self.get_connection().await;
        let keys: Vec<String> = conn.keys("Cyclonetix:*:task:*").await.unwrap_or_default();
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

    async fn load_task_status(&self, task_instance_run_id: &str) -> Option<String> {
        let mut conn = self.get_connection().await;
        let key = self.task_instance_key(task_instance_run_id);
        if let Ok(task_json) = conn.get::<_, String>(key).await {
            let t:TaskInstance = serde_json::from_str(&task_json).unwrap();
            Some(t.status)
        } else {
            None
        }
    }

    async fn save_task_status(&self, task_instance_run_id: &str, status: &str) {
        let mut task_instance = self.load_task_instance(task_instance_run_id).await.unwrap();
        task_instance.status = status.to_string();
        self.save_task_instance(&task_instance).await;
    }

    async fn is_task_scheduled(&self, task_instance_run_id: &str) -> bool {
        let mut conn = self.get_connection().await;
        let queue_key = "work_queue".to_string();
        let instance_id = task_instance_run_id.to_string();
        let queue_items: Vec<String> = conn.lrange(queue_key, 0, -1).await.unwrap_or_default();
        queue_items.contains(&instance_id)
    }

    async fn load_task_instance(&self, task_run_id: &str) -> Option<TaskInstance> {
        let mut conn = self.get_connection().await;
        let key = self.task_instance_key(task_run_id);
        if let Ok(task_json) = conn.get::<_, String>(key).await {
            Some(serde_json::from_str(&task_json).unwrap())
        } else {
            None
        }
    }

    async fn save_task_instance(&self, task: &TaskInstance) {
        let mut conn = self.get_connection().await;
        let key = Self::task_instance_key(self, &task.run_id);
        let _: () = conn
            .set(key, serde_json::to_string(task).unwrap())
            .await
            .unwrap();
    }

    async fn save_context(&self, context: &Context) {
        let mut conn = self.get_connection().await;
        let key = Self::build_context_key(self, &context.dag_instance_id);
        let _: () = conn
            .set(key, serde_json::to_string(context).unwrap())
            .await
            .unwrap();
    }

    async fn load_context(&self, context_id: &str) -> Option<Context> {
        let mut conn = self.get_connection().await;
        if let Ok(context_json) = conn.get::<_, String>(context_id).await {
            serde_json::from_str(&context_json).ok()
        } else {
            None
        }
    }

    // Mutable DagInstance functions
    async fn save_dag_instance(&self, dag_instance: &DagInstance) {
        let mut conn = self.get_connection().await;
        let dag_json = serde_json::to_string(dag_instance).unwrap();
        let _: i32 = conn
            .hset(self.dag_instance_set_key(), &dag_instance.run_id, dag_json)
            .await
            .unwrap();
        let status_key = self.dag_status_key(&dag_instance.run_id);
        let _: () = conn
            .set(status_key, dag_instance.status.clone())
            .await
            .unwrap();

        debug!("Stored DagInstance: {}", dag_instance.run_id);
    }

    async fn load_dag_instance(&self, run_id: &str) -> Option<DagInstance> {
        let mut conn = self.get_connection().await;
        let key = self.dag_instance_set_key();
        match conn.hget::<_, _, String>(key, run_id).await {
            Ok(dag_json) => serde_json::from_str(&dag_json).ok(),
            Err(e) => {
                error!("Failed to retrieve DagInstance: {:?}", e);
                None
            }
        }
    }

    async fn load_scheduled_dag_instances(&self) -> Vec<DagInstance> {
        let mut conn = self.get_connection().await;
        let dag_map: HashMap<String, String> = conn
            .hgetall(self.dag_instance_set_key())
            .await
            .unwrap_or_default();
        let parsed: Vec<DagInstance> = dag_map
            .values()
            .filter_map(|json| serde_json::from_str(json).ok())
            .collect();
        debug!("Retrieved {} DagInstances", parsed.len());
        parsed
    }

    async fn delete_dag_instance(
        &self,
        run_id: &str,
        task_run_ids: &[String],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.get_connection().await;
        let mut pipe = redis::pipe();
        let status_key = self.dag_status_key(run_id);
        pipe.del(status_key);
        for task_run_id in task_run_ids {
            let key = Self::task_instance_key(self, task_run_id);
            pipe.del(key);
        }
        pipe.query_async::<()>(&mut conn)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        info!("Cleaned up DagInstance: {}", run_id);
        Ok(())
    }

    // Immutable GraphInstance functions
    async fn save_graph_instance(&self, graph_instance: &GraphInstance) {
        let mut conn = self.get_connection().await;
        let graph_json = serde_json::to_string(graph_instance).unwrap();
        let _: i32 = conn
            .hset(
                self.graph_instance_set_key(),
                &graph_instance.run_id,
                graph_json,
            )
            .await
            .unwrap();
        let status_key = self.graph_status_key(&graph_instance.run_id);
        let _: () = conn
            .set(status_key, PENDING_STATUS.to_string())
            .await
            .unwrap();
        debug!("Stored GraphInstance: {}", graph_instance.run_id);
    }

    async fn load_graph_instance(&self, run_id: &str) -> Option<GraphInstance> {
        let mut conn = self.get_connection().await;
        let key = self.graph_instance_set_key();
        match conn.hget::<_, _, String>(key, run_id).await {
            Ok(graph_json) => serde_json::from_str(&graph_json).ok(),
            Err(e) => {
                error!("Failed to retrieve GraphInstance: {:?}", e);
                None
            }
        }
    }

    async fn save_dag_template(&self, dag: &DagTemplate) {
        let mut conn = self.get_connection().await;
        let dag_json = serde_json::to_string(dag).unwrap();
        let _: () = conn
            .set(format!("dag_definitions:{}", dag.id), dag_json)
            .await
            .unwrap();
    }

    async fn load_dag_template(&self, dag_id: &str) -> Option<DagTemplate> {
        let mut conn = self.get_connection().await;
        let key = format!("dag_definitions:{}", dag_id);
        if let Ok(dag_json) = conn.get::<_, String>(key).await {
            serde_json::from_str(&dag_json).ok()
        } else {
            None
        }
    }

    async fn publish_graph_update(&self, run_id: &str) {
        let mut conn = self.get_connection().await;
        let _: i32 = conn.publish("graph_update", run_id).await.unwrap();
        debug!("Published graph update for run_id: {}", run_id);
    }

    async fn subscribe_to_graph_updates(self: Arc<Self>) {
        let mut pubsub_conn = self.get_pubsub_connection().await;
        if let Err(e) = pubsub_conn.subscribe("graph_update").await {
            error!("Failed to subscribe to graph updates: {}", e);
            return;
        }
        info!("Subscribed to graph updates.");
        let mut message_stream = pubsub_conn.into_on_message();
        while let Some(msg) = message_stream.next().await {
            let run_id: String = match msg.get_payload() {
                Ok(id) => id,
                Err(e) => {
                    error!("Failed to get payload: {}", e);
                    continue;
                }
            };
            debug!("Graph update received for run_id: {}", run_id);
            if let Some(graph_instance) = self.load_graph_instance(&run_id).await {
                crate::graph::orchestrator::evaluate_graph(self.clone(), &graph_instance)
                    .await;
            } else {
                error!("No GraphInstance found for run_id: {}", run_id);
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
        let _: () = conn.hset(&key, "last_heartbeat", timestamp).await.unwrap();
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

    async fn load_all_agents(&self) -> Vec<AgentStatus> {
        let mut conn = self.get_connection().await;
        let agent_ids: Vec<String> = conn
            .smembers(self.agents_set_key())
            .await
            .unwrap_or_default();
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
        let queue_key = self.queue_key(queue);
        conn.lrange(queue_key, 0, -1).await.unwrap_or_default()
    }

    async fn get_dag_visualization(&self, _run_id: &str) -> Option<String> {
        unimplemented!("Dag visualization not implemented for RedisStateManager yet")
    }

    async fn reset_tasks_from_downed_agents(
        &self,
        heartbeat_threshold: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let current_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let agents = self.load_all_agents().await;
        for agent in agents {
            if current_time - agent.last_heartbeat > heartbeat_threshold as i64 {
                info!("Agent {} is down. Resetting its tasks.", agent.agent_id);
                let mut conn = self.get_connection().await;
                let key = Self::agent_tasks_key(self, &agent.agent_id);
                let assignments: Vec<String> = conn.smembers(&key).await.unwrap_or_default();
                for assignment in assignments {
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
                    if let Some(dag_instance) = self.load_dag_instance(dag_run_id).await {
                        self.save_task_status(task_run_id, PENDING_STATUS).await;
                        let context = self.load_context(&dag_instance.run_id).await.unwrap();
                        let task = self.load_task_instance(task_run_id).await.unwrap();
                        let task_payload = TaskPayload {
                            task_run_id: task_run_id.to_string(),
                            dag_run_id: dag_run_id.to_string(),
                            command: task.clone().command,
                            env_vars: context.get_task_env(task),
                        };
                        self.put_work_on_queue(&task_payload, DEFAULT_QUEUE)
                            .await;
                    } else {
                        warn!(
                            "DagInstance {} not found while resetting task {} for downed agent {}",
                            dag_run_id, task_run_id, agent.agent_id
                        );
                    }
                    let _: () = conn.srem(&key, assignment).await?;
                }
                let _: () = conn.srem(self.agents_set_key(), &agent.agent_id).await?;
                let agent_key = Self::agent_key(self, &agent.agent_id);
                let _: () = conn.del(&agent_key).await?;
                debug!("Agent {} removed from active agents.", agent.agent_id);
            }
        }
        Ok(())
    }

    async fn load_scheduled_dag_keys(&self) -> Vec<String> {
        let mut conn = self.get_connection().await;
        conn.hkeys(self.dag_instance_set_key())
            .await
            .unwrap_or_default()
    }

    async fn load_dag_status(&self, run_id: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut conn = self.get_connection().await;
        let key = self.dag_status_key(run_id);
        let status: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(status.unwrap_or_else(|| PENDING_STATUS.to_string()))
    }
}
