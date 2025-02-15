use crate::models::context::Context;
use crate::models::dag::{DAGInstance, DAGTemplate};
use crate::models::parameters::ParameterSet;
use crate::models::task::{TaskInstance, TaskTemplate};
use crate::orchestrator::orchestrator::evaluate_graph;
use crate::state::state_manager::{StateManager, WorkerStatus};
use async_trait::async_trait;
use futures_util::stream::StreamExt;
use redis::aio::PubSub;
use redis::{aio::MultiplexedConnection, AsyncCommands};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info};
use tracing::log::warn;

pub struct RedisStateManager {
    client: redis::Client,
    //queues: Vec<String>,
}

impl RedisStateManager {
    pub async fn new(redis_url: &str) -> Self {
        let client = redis::Client::open(redis_url).expect("Failed to connect to Redis");
        RedisStateManager {
            client,
            //queues: queues.to_owned(),
        }
    }

    async fn get_connection(&self) -> MultiplexedConnection {
        self.client
            .get_multiplexed_async_connection()
            .await
            .expect("Failed to get Redis connection")
    }

    async fn get_pubsub_connection(&self) -> PubSub {
        self.client
            .get_async_pubsub()
            .await
            .expect("Failed to get Redis connection")
    }

    // Key builders
    fn build_task_key(task_id: &str) -> String {
        format!("task:{}", task_id)
    }
    fn build_context_key(context_id: &str) -> String {
        format!("context:{}", context_id)
    }
    fn build_parameter_set_key(parameter_set_id: &str) -> String {
        format!("parameter_set:{}", parameter_set_id)
    }
    fn task_instance_key(run_id: &str) -> String {
        format!("task_instance:{}", run_id)
    }
    fn dag_status_key(dag_run_id: &str) -> String {
        format!("dag_status:{}", dag_run_id)
    }
    fn worker_key(worker_id: &str) -> String {
        format!("worker:{}", worker_id)
    }
    fn worker_tasks_key(worker_id: &str) -> String {
        format!("worker:{}:tasks", worker_id)
    }
}

#[async_trait]
impl StateManager for RedisStateManager {
    async fn get_work_from_queue(&self, queue: &str) -> Option<(String, String)> {
        let mut conn = self.get_connection().await;

        if let Ok(queue_item) = conn.lpop::<_, String>(queue, None).await {
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
        let key = Self::build_task_key(&task.id);
        let _: () = conn
            .set(key, serde_json::to_string(task).unwrap())
            .await
            .unwrap();
    }

    async fn get_task_definition(&self, task_id: &str) -> Option<TaskTemplate> {
        let mut conn = self.get_connection().await;
        let key = Self::build_task_key(task_id);

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

        info!(
            "Task Instance {} added to queue {} for DAG {}",
            task_instance.run_id, task_instance.queue, dag_run_id
        );
    }

    async fn get_task_status(&self, task_instance_run_id: &str) -> Option<String> {
        let mut conn = self.get_connection().await;
        let key = Self::task_instance_key(task_instance_run_id);
        conn.get(key).await.unwrap_or(None)
    }

    async fn update_task_status(&self, task_instance_run_id: &str, status: &str) {
        let mut conn = self.get_connection().await;
        let key = Self::task_instance_key(task_instance_run_id);
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
        let key = Self::build_context_key(&context.id);
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
        let key = Self::build_parameter_set_key(&parameter_set.id);
        let _: () = conn
            .set(key, serde_json::to_string(parameter_set).unwrap())
            .await
            .unwrap();
    }

    // Remove a completed DAG execution
    async fn remove_scheduled_dag(&self, run_id: &str) {
        let mut conn = self.get_connection().await;

        // Try to remove by run_id directly
        let removed: i32 = conn.hdel("dag_execution", run_id).await.unwrap_or(0);

        if removed > 0 {
            info!("Successfully removed scheduled DAG: {}", run_id);
        } else {
            tracing::warn!("Scheduled DAG {} not found in Redis.", run_id);
        }
    }

    async fn get_dag_status(&self, run_id: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut conn = self.get_connection().await;
        let key = Self::dag_status_key(run_id);
        // Map any error to a boxed error.
        let status: Option<String> = conn.get(&key).await.map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(status.unwrap_or_else(|| "pending".to_string()))
    }

    async fn get_scheduled_dags(&self) -> Vec<DAGInstance> {
        let mut conn = self.get_connection().await;
        let dag_map: HashMap<String, String> =
            conn.hgetall("dag_execution").await.unwrap_or_default();

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
        conn.hset::<&str, &str, String, ()>("dag_execution", &dag_execution.run_id, dag_json)
            .await
            .unwrap();

        // Set initial DAG execution status
        let status_key = format!("dag_status:{}", dag_execution.run_id);
        conn.set::<String, String, ()>(status_key, "pending".to_string())
            .await
            .unwrap();

        info!("Stored DAG execution: {}", dag_execution.run_id);
    }

    async fn get_dag_execution(&self, run_id: &str) -> Option<DAGInstance> {
        let mut conn = self.get_connection().await;
        let key = "dag_execution";

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
        let dag_status_key = format!("dag_status:{}", dag_run_id);
        pipe.del(dag_status_key);

        // Delete each task_instance key.
        for task_run_id in task_run_ids {
            let task_instance_key = Self::task_instance_key(task_run_id);
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
        info!("Published graph update for DAG: {}", dag_run_id);
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

            // Retrieve the DAG execution based on the run ID and immediately evaluate it.
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

    async fn register_worker(&self, worker_id: &str) {
        let mut conn = self.get_connection().await;
        let key = Self::worker_key(worker_id);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let _: () = conn.hset(key.clone(), "last_heartbeat", timestamp).await.unwrap();
        // Also add the worker to a global set of workers.
        let _: () = conn.sadd("workers", worker_id).await.unwrap();
        info!("Registered worker {}", worker_id);
    }

    async fn update_worker_heartbeat(&self, worker_id: &str) {
        let mut conn = self.get_connection().await;
        let key = Self::worker_key(worker_id);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let _: () = conn.hset(key, "last_heartbeat", timestamp).await.unwrap();
        debug!("Updated heartbeat for worker {}", worker_id);
    }

    async fn assign_task_to_worker(&self, worker_id: &str, assignment: &str) {
        let mut conn = self.get_connection().await;
        let key = Self::worker_tasks_key(worker_id);
        let _: () = conn.sadd(key, assignment).await.unwrap();
        info!("Assigned task {} to worker {}", assignment, worker_id);
    }

    async fn remove_task_from_worker(&self, worker_id: &str, assignment: &str) {
        let mut conn = self.get_connection().await;
        let key = Self::worker_tasks_key(worker_id);
        let _: () = conn.srem(key, assignment).await.unwrap();
        info!("Removed task {} from worker {}", assignment, worker_id);
    }

    async fn get_all_workers(&self) -> Vec<WorkerStatus> {
        let mut conn = self.get_connection().await;
        let worker_ids: Vec<String> = conn.smembers("workers").await.unwrap_or_default();
        let mut workers = Vec::new();
        for worker_id in worker_ids {
            let key = Self::worker_key(&worker_id);
            let heartbeat: Option<i64> = conn.hget(&key, "last_heartbeat").await.unwrap_or(None);
            let tasks: Vec<String> = conn
                .smembers(Self::worker_tasks_key(&worker_id))
                .await
                .unwrap_or_default();
            if let Some(ts) = heartbeat {
                workers.push(WorkerStatus {
                    worker_id: worker_id.clone(),
                    last_heartbeat: ts,
                    tasks,
                });
            }
        }
        workers
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


    async fn reset_tasks_from_dead_workers(
        &self,
        heartbeat_threshold: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64;
        // Get all workers with their statuses.
        let workers = self.get_all_workers().await;
        for worker in workers {
            if current_time - worker.last_heartbeat > heartbeat_threshold as i64 {
                info!("Worker {} is dead. Resetting its tasks.", worker.worker_id);
                let mut conn = self.get_connection().await;
                let key = Self::worker_tasks_key(&worker.worker_id);
                // Get the composite assignment strings.
                let assignments: Vec<String> = conn.smembers(&key).await.unwrap_or_default();
                for assignment in assignments {
                    // Each assignment is in the form "task_run_id|dag_run_id"
                    let parts: Vec<&str> = assignment.split('|').collect();
                    if parts.len() != 2 {
                        warn!(
                            "Malformed assignment {} for worker {}",
                            assignment, worker.worker_id
                        );
                        continue;
                    }
                    let task_run_id = parts[0];
                    let dag_run_id = parts[1];
                    // Retrieve the DAG execution.
                    if let Some(dag_execution) = self.get_dag_execution(dag_run_id).await {
                        if let Some(task_instance) = dag_execution
                            .tasks
                            .iter()
                            .find(|t| t.run_id == task_run_id)
                        {
                            info!(
                                "Resetting task {} from dead worker {}",
                                task_run_id, worker.worker_id
                            );
                            // Update task status to "pending".
                            self.update_task_status(task_run_id, "pending").await;
                            // Re-enqueue the task.
                            self.enqueue_task_instance(task_instance, dag_run_id)
                                .await;
                        } else {
                            warn!(
                                "Task instance {} not found in DAG {} for dead worker {}",
                                task_run_id, dag_run_id, worker.worker_id
                            );
                        }
                    } else {
                        warn!(
                            "DAG execution {} not found while resetting task {} for dead worker {}",
                            dag_run_id, task_run_id, worker.worker_id
                        );
                    }
                    // Remove the assignment from the worker.
                    let _: () = conn.srem(&key, assignment).await?;
                }
                // Optionally remove the dead worker from the global set.
                let _: () = conn.srem("workers", &worker.worker_id).await?;
                info!("Worker {} removed from active workers.", worker.worker_id);
            }
        }
        Ok(())
    }
}
