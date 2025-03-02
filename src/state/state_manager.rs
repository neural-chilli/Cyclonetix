use crate::models::context::Context;
use crate::models::dag::{DagInstance, DagTemplate, GraphInstance};
use crate::models::task::{TaskInstance, TaskTemplate};
use async_trait::async_trait;
use std::collections::HashMap;
use std::error::Error;

#[derive(Debug, Clone)]
pub struct AgentStatus {
    pub agent_id: String,
    pub last_heartbeat: i64,
    pub tasks: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TaskPayload {
    pub task_run_id: String,
    pub dag_run_id: String,
    pub command: String,
    pub env_vars: HashMap<String, String>,
}

#[async_trait]
pub trait StateManager: Sync + Send {
    // Work queue operations
    async fn get_work_from_queue(&self, queue: &str) -> Option<TaskPayload>;
    async fn put_work_on_queue(&self, task_payload: &TaskPayload, queue: &str);
    // Task definitions (immutable)
    async fn save_task(&self, task: &TaskTemplate);
    async fn load_task(&self, task_id: &str) -> Option<TaskTemplate>;
    async fn load_all_tasks(&self) -> Vec<TaskTemplate>;
    // Task mutable state
    async fn load_task_status(&self, task_instance_run_id: &str) -> Option<String>;
    async fn save_task_status(&self, task_instance_run_id: &str, status: &str);
    async fn is_task_scheduled(&self, task_instance_run_id: &str) -> bool;
    async fn load_task_instance(&self, task_instance_run_id: &str) -> Option<TaskInstance>;
    async fn save_task_instance(&self, task: &TaskInstance);
    // Context operations
    async fn save_context(&self, context: &Context);
    async fn load_context(&self, context_id: &str) -> Option<Context>;
    // Mutable DAG execution state
    async fn save_dag_instance(&self, dag_instance: &DagInstance);
    async fn load_dag_instance(&self, run_id: &str) -> Option<DagInstance>;
    async fn load_scheduled_dag_instances(&self) -> Vec<DagInstance>;
    async fn delete_dag_instance(
        &self,
        run_id: &str,
        task_run_ids: &[String],
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn load_dag_status(&self, run_id: &str) -> Result<String, Box<dyn Error + Send + Sync>>;
    async fn save_dag_status(
        &self,
        run_id: &str,
        status: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    // Immutable dependency graph
    async fn save_graph_instance(&self, graph_instance: &GraphInstance);
    async fn load_graph_instance(&self, run_id: &str) -> Option<GraphInstance>;
    // DAG template operations
    async fn save_dag_template(&self, dag: &DagTemplate);
    async fn load_dag_template(&self, dag_id: &str) -> Option<DagTemplate>;
    // Notifications
    async fn publish_graph_update(&self, run_id: &str);
    async fn subscribe_to_graph_updates(self: std::sync::Arc<Self>);
    // Orchestrator metadata
    async fn get_orchestrator_count(&self) -> Result<u32, Box<dyn Error + Send + Sync>>;
    async fn get_orchestrator_id(&self) -> Result<u32, Box<dyn Error + Send + Sync>>;
    // Agent operations
    async fn register_agent(&self, agent_id: &str);
    async fn update_agent_heartbeat(&self, agent_id: &str);
    async fn assign_task_to_agent(&self, agent_id: &str, assignment: &str);
    async fn remove_task_from_agent(&self, agent_id: &str, assignment: &str);
    async fn load_all_agents(&self) -> Vec<AgentStatus>;
    // Queue details
    async fn get_queue_tasks(&self, queue: &str) -> Vec<String>;
    // For UI visualization (detailed view)
    async fn get_dag_visualization(&self, run_id: &str) -> Option<String>;
    // Reset tasks from downed agents
    async fn reset_tasks_from_downed_agents(
        &self,
        heartbeat_threshold: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    // For orchestrator recovery – get keys for mutable DagInstances
    async fn load_scheduled_dag_keys(&self) -> Vec<String>;
}
