use crate::models::context::Context;
use crate::models::dag::{DagDefinition, ScheduledDag};
use crate::models::outcome::Outcome;
use crate::models::parameters::ParameterSet;
use crate::models::task::Task;
use crate::orchestrator::orchestrator::evaluate_graph;
use crate::state::state_manager::StateManager;
use async_trait::async_trait;
use futures_util::stream::StreamExt;
use redis::{aio::MultiplexedConnection, AsyncCommands};
use std::sync::Arc;
use tracing::info;
use tracing::log::warn;
use uuid::Uuid;

pub struct RedisStateManager {
    client: redis::Client,
}

impl RedisStateManager {
    pub async fn new(redis_url: &str) -> Self {
        let client = redis::Client::open(redis_url).expect("Failed to connect to Redis");
        RedisStateManager { client }
    }

    async fn get_connection(&self) -> MultiplexedConnection {
        self.client
            .get_multiplexed_async_connection()
            .await
            .expect("Failed to get Redis connection")
    }

    fn build_task_key(task_id: &str) -> String {
        format!("task:{}", task_id)
    }

    fn build_context_key(context_id: &str) -> String {
        format!("context:{}", context_id)
    }

    fn build_parameter_set_key(parameter_set_id: &str) -> String {
        format!("parameter_set:{}", parameter_set_id)
    }

    fn task_queue_key() -> String {
        "work_queue".to_string() // Single queue for simplicity
    }

    fn work_queue_key() -> String {
        "work_queue".to_string() // Single queue for all workers
    }

    fn task_status_key(task_id: &str, context_id: &str) -> String {
        format!("task_status:{}:{}", task_id, context_id)
    }

    fn task_instance_key(task_id: &str, context_id: &str) -> String {
        format!("task_instance:{}:{}", task_id, context_id)
    }

    fn task_key(task_id: &str) -> String {
        format!("task:{}", task_id) // Consistent namespacing
    }
}

#[async_trait]
impl StateManager for RedisStateManager {
    async fn get_work_from_queue(&self, queue: &str) -> Option<(String, String)> {
        let mut conn = self.get_connection().await;
        if let Ok(instance_id) = conn.lpop::<_, String>(queue, None).await {
            let parts: Vec<&str> = instance_id.split('|').collect();
            if parts.len() == 2 {
                return Some((parts[0].to_string(), parts[1].to_string()));
            }
        }
        None
    }

    async fn put_work_in_queue(&self, queue: &str, task: &Task) {
        let mut conn = self.get_connection().await;
        let task_json = serde_json::to_string(task).unwrap();
        let _: () = conn.rpush(queue, task_json).await.unwrap();
    }

    async fn store_task(&self, task: &Task) {
        let mut conn = self.get_connection().await;
        let key = Self::build_task_key(&task.id);
        let _: () = conn
            .set(key, serde_json::to_string(task).unwrap())
            .await
            .unwrap();
    }

    async fn get_task_definition(&self, task_id: &str) -> Option<Task> {
        let mut conn = self.get_connection().await;
        let key = Self::task_key(task_id);

        if let Ok(task_json) = conn.get::<_, String>(key).await {
            serde_json::from_str(&task_json).ok()
        } else {
            None
        }
    }

    async fn get_all_tasks(&self) -> Vec<Task> {
        let mut conn = self.get_connection().await;
        let keys: Vec<String> = conn.keys("task:*").await.unwrap_or_else(|_| vec![]);
        let mut tasks = Vec::new();
        for key in keys {
            if let Ok(task_json) = conn.get::<_, String>(key).await {
                if let Ok(task) = serde_json::from_str::<Task>(&task_json) {
                    tasks.push(task);
                }
            }
        }
        tasks
    }

    async fn mark_task_complete(&self, execution_id: &str, task_id: &str) {
        let mut conn = self.get_connection().await;
        let key = format!("task_status:{}:{}", execution_id, task_id);
        let _: () = conn.set(key, "completed").await.unwrap();
    }

    async fn update_task_status(&self, task_id: &str, context_id: &str, status: &str) {
        let mut conn = self.get_connection().await;
        let status_key = Self::task_status_key(task_id, context_id); // Use correct key for task status
        conn.set::<String, String, ()>(status_key.clone(), status.to_string())
            .await
            .unwrap();
        info!(
            "Task Instance {} updated to status: {}",
            format!("{}|{}", task_id, context_id),
            status
        );
        if status == "completed" || status == "failed" {
            // Remove task instance from work queue
            conn.lrem::<String, String, ()>(
                "work_queue".to_string(),
                1,
                format!("{}|{}", task_id, context_id),
            )
            .await
            .unwrap();
            info!(
                "🗑️ Task Instance {} removed from queue after completion.",
                format!("{}|{}", task_id, context_id)
            );
        }
    }

    async fn enqueue_task_instance(&self, task_id: &str, context_id: &str) {
        let mut conn = self.get_connection().await;
        // Store task instance in Redis queue
        let queue_key = Self::work_queue_key();
        let instance_id = format!("{}|{}", task_id, context_id);
        conn.rpush::<String, String, ()>(queue_key, instance_id.clone())
            .await
            .unwrap();
        // Store status for tracking
        let instance_status_key = Self::task_instance_key(task_id, context_id);
        conn.set::<String, String, ()>(instance_status_key, "pending".to_string())
            .await
            .unwrap();
        info!("Task Instance {} added to work queue!", instance_id);
    }

    async fn get_task_status(&self, task_id: &str, context_id: &str) -> Option<String> {
        let mut conn = self.get_connection().await;
        let key = format!("task_status:{}:{}", task_id, context_id);
        conn.get(key).await.unwrap_or(None)
    }

    async fn is_task_scheduled(&self, task_id: &str, context_id: &str) -> bool {
        let mut conn = self.get_connection().await;
        let queue_key = "work_queue".to_string();
        let instance_id = format!("{}|{}", task_id, context_id);

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

    async fn save_context(&self, context_id: &str, context: &Context) {
        let mut conn = self.get_connection().await;
        let context_json = serde_json::to_string(context).unwrap();
        let _: () = conn.set(context_id, context_json).await.unwrap();
    }

    async fn store_parameter_set(&self, parameter_set: &ParameterSet) {
        let mut conn = self.get_connection().await;
        let key = Self::build_parameter_set_key(&parameter_set.id);
        let _: () = conn
            .set(key, serde_json::to_string(parameter_set).unwrap())
            .await
            .unwrap();
    }

    async fn schedule_outcome(&self, outcome: &Outcome) {
        let mut conn = self.get_connection().await;
        let outcome_key = format!("scheduled_outcome:{}", outcome.run_id);
        // Check if the outcome is already scheduled by checking the existence of the hash.
        let already_exists: bool = conn.exists(&outcome_key).await.unwrap_or(false);
        if already_exists {
            warn!("Re-adding scheduled outcome: {}", outcome.run_id);
        }
        // Store the outcome details along with its context in a Redis hash.
        // You could add additional fields here as needed.
        let _: () = conn
            .hset(&outcome_key, "outcome_id", &outcome.run_id)
            .await
            .unwrap();
        // Add the outcome id to the scheduled outcomes set (for fast membership queries).
        let _: () = conn
            .sadd("scheduled_outcomes", &outcome.run_id)
            .await
            .unwrap();
    }

    async fn get_scheduled_outcomes(&self) -> Vec<Outcome> {
        let mut conn = self.get_connection().await;
        let outcomes: Vec<String> = conn
            .smembers("scheduled_outcomes")
            .await
            .unwrap_or_default();
        outcomes
            .into_iter()
            .filter_map(|o| serde_json::from_str(&o).ok())
            .collect()
    }

    async fn remove_scheduled_outcome(&self, outcome_id: &str) {
        let mut conn = self.get_connection().await;
        // Fetch all stored outcomes
        let outcomes: Vec<String> = conn
            .smembers("scheduled_outcomes")
            .await
            .unwrap_or_default();
        for outcome in &outcomes {
            if let Ok(parsed) = serde_json::from_str::<Outcome>(outcome) {
                if parsed.run_id == outcome_id {
                    let removed: i32 = conn
                        .srem::<&str, String, i32>("scheduled_outcomes", outcome.clone())
                        .await
                        .unwrap();
                    if removed > 0 {
                        info!("Successfully removed scheduled outcome: {}", outcome_id);
                        return;
                    } else {
                        warn!("Failed to remove matched outcome: {}", outcome_id);
                    }
                }
            }
        }
        warn!("Outcome {} not found in Redis set.", outcome_id);
    }

    async fn get_scheduled_dags(&self) -> Vec<ScheduledDag> {
        let mut conn = self.get_connection().await;
        let scheduled_dags: Vec<String> = conn.smembers("scheduled_dags").await.unwrap_or_default();
        scheduled_dags
            .into_iter()
            .filter_map(|dag| serde_json::from_str::<ScheduledDag>(&dag).ok())
            .collect()
    }

    async fn schedule_dag(&self, scheduled_dag: &ScheduledDag) {
        let mut conn = self.get_connection().await;
        let dag_json = serde_json::to_string(scheduled_dag).unwrap();
        conn.sadd::<&str, String, ()>("scheduled_dags", dag_json)
            .await
            .unwrap();
    }

    // Remove a completed DAG execution
    async fn remove_scheduled_dag(&self, run_id: &String) {
        let mut conn = self.get_connection().await;
        let scheduled_dags: Vec<String> = conn.smembers("scheduled_dags").await.unwrap_or_default();
        for dag in &scheduled_dags {
            if let Ok(parsed) = serde_json::from_str::<ScheduledDag>(dag) {
                if &parsed.run_id == run_id {
                    conn.srem::<&str, String, ()>("scheduled_dags", dag.clone())
                        .await
                        .unwrap();
                    info!("Successfully removed scheduled DAG: {}", run_id);
                    return;
                }
            }
        }
        warn!("Scheduled DAG {} not found in Redis.", run_id);
    }

    async fn get_dag_status(&self, scheduled_dag: &ScheduledDag) -> Result<String, String> {
        let mut conn = self.get_connection().await;
        let key = format!(
            "dag_status:{}:{}",
            scheduled_dag.dag.id, scheduled_dag.run_id
        );
        let status: Option<String> = conn.get(&key).await.unwrap_or(None);
        Ok(status.unwrap_or("pending".to_string()))
    }

    // Store DAG Definitions (static DAGs)
    async fn store_dag_definition(&self, dag: &DagDefinition) {
        let mut conn = self.get_connection().await;
        let dag_json = serde_json::to_string(dag).unwrap();
        conn.set::<String, String, ()>(format!("dag_definitions:{}", dag.id), dag_json)
            .await
            .unwrap();
    }

    // Retrieve a DAG definition
    async fn get_dag(&self, dag_id: &str) -> Option<DagDefinition> {
        let mut conn = self.get_connection().await;
        let dag: Option<String> = conn
            .get(format!("dag_definitions:{}", dag_id))
            .await
            .unwrap_or(None);
        dag.and_then(|d| serde_json::from_str(&d).ok())
    }

    async fn notify_graph_update(&self, context_id: &str) {
        let mut conn = self.get_connection().await;
        conn.publish::<String, String, ()>("graph_update".to_string(), context_id.to_string())
            .await
            .unwrap();
        info!("Published graph update for context: {}", context_id);
    }

    async fn listen_for_graph_updates(self: Arc<Self>) {
        let client = redis::Client::open("redis://127.0.0.1:6379").unwrap();
        let mut pubsub_conn = client.get_async_pubsub().await.unwrap();
        pubsub_conn.subscribe("graph_update").await.unwrap();

        let state_manager: Arc<dyn StateManager> = self.clone();
        let mut message_stream = pubsub_conn.into_on_message();

        while let Some(msg) = message_stream.next().await {
            let payload: String = msg.get_payload().unwrap();
            info!("Graph update received for context: {}", payload);
            evaluate_graph(state_manager.clone(), &payload).await;
        }
    }

    async fn register_executor(&self, executor_id: &str) {
        let mut conn = self.get_connection().await;
        let key = format!("executor:{}", executor_id);
        let _: () = conn.set_ex(key, "alive", 60).await.unwrap();
    }

    async fn get_orchestrator_count(&self) -> Result<u32, String> {
        let mut conn = self.get_connection().await;
        let count: Option<u32> = conn.get("orchestrator_count").await.unwrap_or(None);
        Ok(count.unwrap_or(1)) // Default to 1 orchestrator
    }

    async fn get_orchestrator_id(&self) -> Result<u32, String> {
        let mut conn = self.get_connection().await;
        let id: Option<u32> = conn.get("orchestrator_id").await.unwrap_or(None);
        Ok(id.unwrap_or(0)) // Default to orchestrator 0
    }
}
