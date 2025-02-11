use crate::models::context::Context;
use crate::models::dag::{DagDefinition, ScheduledDag};
use crate::models::outcome::Outcome;
use crate::models::parameters::ParameterSet;
use crate::models::task::Task;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait StateManager: Send + Sync {
    // Work
    async fn get_work_from_queue(&self, queue: &str) -> Option<(String, String)>; // (task_id, context_id)
    async fn put_work_in_queue(&self, queue: &str, task: &Task);

    // Tasks
    async fn store_task(&self, task: &Task);
    async fn get_task_definition(&self, task_id: &str) -> Option<Task>;
    async fn get_all_tasks(&self) -> Vec<Task>;
    async fn mark_task_complete(&self, execution_id: &str, task_id: &str);
    async fn update_task_status(&self, task_id: &str, context_id: &str, status: &str);
    async fn enqueue_task_instance(&self, task_id: &str, context_id: &str);
    async fn get_task_status(&self, task_id: &str, context_id: &str) -> Option<String>;
    async fn is_task_scheduled(&self, task_id: &str, context_id: &str) -> bool;

    // Contexts
    async fn store_context(&self, context: &Context);
    async fn get_context(&self, context_id: &str) -> Option<Context>;
    async fn save_context(&self, context_id: &str, context: &Context);

    // Parameter sets
    async fn store_parameter_set(&self, parameter_set: &ParameterSet);

    // Outcomes
    async fn schedule_outcome(&self, outcome: &Outcome);
    async fn get_scheduled_outcomes(&self) -> Vec<Outcome>;
    async fn remove_scheduled_outcome(&self, outcome_id: &str);

    // DAGs
    async fn get_scheduled_dags(&self) -> Vec<ScheduledDag>;
    async fn schedule_dag(&self, scheduled_dag: &ScheduledDag);
    async fn remove_scheduled_dag(&self, run_id: &String);
    async fn get_dag_status(&self, scheduled_dag: &ScheduledDag) -> Result<String, String>;

    // Store & Retrieve DAG Definitions (static DAGs)
    async fn store_dag_definition(&self, dag: &DagDefinition);
    async fn get_dag(&self, dag_id: &str) -> Option<DagDefinition>;

    // Graphs
    async fn notify_graph_update(&self, context_id: &str);
    async fn listen_for_graph_updates(self: Arc<Self>);

    // Registrations
    async fn register_executor(&self, executor_id: &str);
    async fn get_orchestrator_count(&self) -> Result<u32, String>;
    async fn get_orchestrator_id(&self) -> Result<u32, String>;
}
