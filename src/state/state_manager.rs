use crate::models::context::Context;
use crate::models::dag::{DAGInstance, DAGTemplate};
use crate::models::parameters::ParameterSet;
use crate::models::task::{TaskInstance, TaskTemplate};
use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

/// A struct representing the status of a agent.
#[derive(Debug, Clone)]
pub struct AgentStatus {
    pub agent_id: String,
    pub last_heartbeat: i64, // Unix timestamp in seconds.
    pub tasks: Vec<String>,  // Task instance run IDs assigned to this agent.
}

/// The StateManager trait defines asynchronous methods for both executing and querying workflow state.
/// It is designed to be backend-agnostic so that UIs (or recovery code) can query the current queues,
/// agent statuses, scheduled DAGs, and even a DAG visualization.
#[async_trait]
pub trait StateManager: Send + Sync {
    // ----- Existing Methods -----
    async fn get_work_from_queue(&self, queue: &str) -> Option<(String, String)>;
    async fn store_task(&self, task: &TaskTemplate);
    async fn get_task_definition(&self, task_id: &str) -> Option<TaskTemplate>;
    async fn get_all_tasks(&self) -> Vec<TaskTemplate>;
    async fn enqueue_task_instance(&self, task_instance: &TaskInstance, dag_run_id: &str);
    async fn get_task_status(&self, task_instance_run_id: &str) -> Option<String>;
    async fn update_task_status(&self, task_instance_run_id: &str, status: &str);
    async fn is_task_scheduled(&self, task_instance_run_id: &str) -> bool;
    async fn store_context(&self, context: &Context);
    async fn get_context(&self, context_id: &str) -> Option<Context>;
    async fn store_parameter_set(&self, parameter_set: &ParameterSet);
    async fn remove_scheduled_dag(&self, run_id: &str);
    async fn get_dag_status(&self, run_id: &str) -> Result<String, Box<dyn Error + Send + Sync>>;
    async fn get_scheduled_dags(&self) -> Vec<DAGInstance>;
    async fn store_dag_execution(&self, dag_execution: &DAGInstance);
    async fn get_dag_execution(&self, run_id: &str) -> Option<DAGInstance>;
    async fn store_dag_definition(&self, dag: &DAGTemplate);
    async fn get_dag(&self, dag_id: &str) -> Option<DAGTemplate>;
    async fn cleanup_dag_execution(
        &self,
        dag_run_id: &str,
        task_run_ids: &[String],
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn notify_graph_update(&self, dag_run_id: &str);
    async fn listen_for_graph_updates(self: Arc<Self>);
    async fn get_orchestrator_count(&self) -> Result<u32, Box<dyn Error + Send + Sync>>;
    async fn get_orchestrator_id(&self) -> Result<u32, Box<dyn Error + Send + Sync>>;

    // ----- New Methods for Agent Tracking & UI -----
    /// Registers a agent with the given agent_id and records its first heartbeat.
    async fn register_agent(&self, agent_id: &str);
    /// Updates the heartbeat timestamp for the given agent.
    async fn update_agent_heartbeat(&self, agent_id: &str);
    /// Assigns a task (by run_id) to a agent.
    async fn assign_task_to_agent(&self, agent_id: &str, task_run_id: &str);
    /// Removes a task (by run_id) from a agent's assignment list.
    async fn remove_task_from_agent(&self, agent_id: &str, task_run_id: &str);
    /// Returns a list of all registered agents along with their last heartbeat and assigned tasks.
    async fn get_all_agents(&self) -> Vec<AgentStatus>;
    /// Returns a list of task instance run IDs currently in the specified queue.
    async fn get_queue_tasks(&self, queue: &str) -> Vec<String>;
    /// Returns a JSON string representing the DAG visualization (nodes with statuses and edges).
    async fn get_dag_visualization(&self, dag_run_id: &str) -> Option<String>;
    async fn reset_tasks_from_downed_agents(
        &self,
        heartbeat_threshold: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
}
