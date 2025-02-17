use crate::models::context::Context;
use crate::models::dag::DAGInstance;
use crate::models::task::TaskInstance;
use crate::orchestrator::execution_graph::ExecutionGraph;
use crate::state::state_manager::StateManager;
use crate::utils::constants::{COMPLETED_STATUS, DEFAULT_QUEUE, PENDING_STATUS};
use async_recursion::async_recursion;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use twox_hash::XxHash64;
use uuid::Uuid;

/// Evaluates the execution graph for a given DAG execution and schedules tasks whose dependencies are met.
/// Evaluates the execution graph for a given DAG execution and schedules tasks whose dependencies are met.
/// Now, it only schedules a task if its current status is "pending" and then marks it as "queued" to avoid re‑scheduling.
pub async fn evaluate_graph<S: StateManager + ?Sized>(
    state_manager: Arc<S>,
    dag_execution: &DAGInstance,
) {
    debug!("Evaluating DAG execution: {}", dag_execution.run_id);

    let execution_graph = ExecutionGraph::from_dag_execution(dag_execution);
    let executable_tasks = execution_graph
        .get_executable_tasks(&state_manager, dag_execution)
        .await;

    for task_run_id in executable_tasks {
        if let Some(task_instance) = dag_execution.tasks.iter().find(|t| t.run_id == task_run_id) {
            // Retrieve the current status. If it's not "pending", we skip scheduling.
            let current_status = state_manager
                .get_task_status(&task_instance.run_id)
                .await
                .unwrap_or_else(|| PENDING_STATUS.to_string());
            if current_status != PENDING_STATUS {
                debug!(
                    "Task {} is already in status '{}', skipping scheduling.",
                    task_instance.run_id, current_status
                );
                continue;
            }

            if execution_graph
                .are_dependencies_completed(&state_manager, task_instance)
                .await
            {
                debug!("Task {} is ready, scheduling it...", task_instance.run_id);
                // Mark the task as "queued" to prevent re-scheduling.
                state_manager
                    .update_task_status(&task_instance.run_id, "queued")
                    .await;
                state_manager
                    .enqueue_task_instance(task_instance, &dag_execution.run_id)
                    .await;
            } else {
                debug!(
                    "Task {} is waiting for dependencies...",
                    task_instance.run_id
                );
            }
        } else {
            warn!(
                "TaskInstance {} not found in DAG Execution {}",
                task_run_id, dag_execution.run_id
            );
        }
    }
}

/// Recovers orchestrator state by re-evaluating scheduled DAG executions.
pub async fn recover_orchestrator<S: StateManager + ?Sized>(state_manager: Arc<S>) {
    info!("Orchestrator recovery: Checking scheduled executions...");

    let scheduled_dags = state_manager.get_scheduled_dags().await;
    let orchestrator_count = state_manager.get_orchestrator_count().await.unwrap_or(1);
    let orchestrator_id = state_manager.get_orchestrator_id().await.unwrap_or(0);

    for dag_execution in scheduled_dags {
        let assigned_orchestrator =
            (hash64(dag_execution.run_id.as_bytes()) % orchestrator_count as u64) as u32;

        if assigned_orchestrator == orchestrator_id {
            info!(
                "This orchestrator is responsible for DAG execution: {}",
                dag_execution.run_id
            );

            let dag_status = state_manager
                .get_dag_status(&dag_execution.run_id)
                .await
                .unwrap_or_else(|_| PENDING_STATUS.to_string());

            if dag_status != COMPLETED_STATUS {
                info!("Recovering DAG execution: {}", dag_execution.run_id);
                evaluate_graph(state_manager.clone(), &dag_execution).await;
            } else {
                debug!(
                    "DAG execution {} is already completed. Skipping.",
                    dag_execution.run_id
                );
            }
        } else {
            info!(
                "Skipping DAG execution {} (Handled by another orchestrator)",
                dag_execution.run_id
            );
        }
    }

    info!("Orchestrator recovery complete.");
}

/// Continuously monitors for scheduled DAG executions and evaluates them.
pub async fn monitor_scheduled_dags<S: StateManager + ?Sized>(state_manager: Arc<S>) {
    loop {
        debug!("Checking for new scheduled DAGs...");

        let scheduled_dags = state_manager.get_scheduled_dags().await;
        if scheduled_dags.is_empty() {
            debug!("No scheduled DAGs found.");
        }

        for dag_execution in scheduled_dags {
            debug!("Found scheduled DAG execution: {}", dag_execution.run_id);
            let execution_graph = ExecutionGraph::from_dag_execution(&dag_execution);
            let executable_tasks = execution_graph
                .get_executable_tasks(&state_manager, &dag_execution)
                .await;

            if executable_tasks.is_empty() {
                info!(
                    "DAG {} is fully executed. Removing from schedule.",
                    dag_execution.run_id
                );
                let task_run_ids: Vec<String> = dag_execution
                    .tasks
                    .iter()
                    .map(|t| t.run_id.clone())
                    .collect();
                let _ = state_manager
                    .cleanup_dag_execution(&dag_execution.run_id, &task_run_ids)
                    .await;
            } else {
                debug!(
                    "DAG {} still has {} tasks to execute. Running evaluation.",
                    dag_execution.run_id,
                    executable_tasks.len()
                );
                evaluate_graph(state_manager.clone(), &dag_execution).await;
            }
        }

        if let Err(e) = state_manager.reset_tasks_from_downed_agents(15).await {
            error!("Error resetting tasks from downed agents: {}", e);
        }

        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

/// Builds a dynamic DAG execution starting from a given root task.
/// This function resolves the entire dependency tree and assigns each task instance
/// a single, consistent run ID that is reused whenever the task is referenced.
pub async fn build_dag_from_task<S: StateManager + ?Sized>(
    state_manager: Arc<S>,
    root_task_id: &str,
    provided_context: Option<Context>,
) -> DAGInstance {
    info!("Building DAGExecution from root task: {}", root_task_id);
    let dag_run_id = Uuid::new_v4().to_string(); // Unique DAG execution ID

    // Map to store each task's run ID so that the same ID is reused.
    let mut run_ids: HashMap<String, String> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut tasks: Vec<TaskInstance> = Vec::new();

    resolve_task_dependencies(
        &state_manager,
        root_task_id,
        &mut tasks,
        &mut visited,
        &mut run_ids,
    )
    .await;

    let context = provided_context.unwrap_or_else(|| Context {
        id: dag_run_id.clone(),
        variables: HashMap::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    });

    DAGInstance {
        run_id: dag_run_id,
        dag_id: format!("dynamic_dag_{}", root_task_id),
        context,
        tasks,
        scheduled_at: Utc::now(),
        tags: None,
    }
}

/// Recursively resolves a task and its dependencies, ensuring that each task is assigned
/// a single run ID. The same run ID is used for both the task instance and in its dependency list.
#[async_recursion]
async fn resolve_task_dependencies<S: StateManager + ?Sized>(
    state_manager: &Arc<S>,
    task_id: &str,
    tasks: &mut Vec<TaskInstance>,
    visited: &mut HashSet<String>,
    run_ids: &mut HashMap<String, String>,
) {
    if visited.contains(task_id) {
        return;
    }
    visited.insert(task_id.to_string());

    // Retrieve the task definition.
    if let Some(task) = state_manager.get_task_definition(task_id).await {
        // Assign a unique run ID for this task if not already assigned.
        let run_id = run_ids
            .entry(task.id.clone())
            .or_insert_with(|| format!("{}_{}", task.id, Uuid::new_v4()))
            .clone();

        // Resolve dependencies and collect their run IDs.
        let mut dependency_run_ids = Vec::new();
        for dep_task_id in &task.dependencies {
            // Recursively resolve the dependency.
            resolve_task_dependencies(state_manager, dep_task_id, tasks, visited, run_ids).await;
            if let Some(dep_run_id) = run_ids.get(dep_task_id) {
                dependency_run_ids.push(dep_run_id.clone());
            } else {
                warn!("No run ID found for dependency task {}", dep_task_id);
            }
        }

        // Create the task instance with the assigned run ID and dependency run IDs.
        let task_instance = TaskInstance {
            run_id: run_id.clone(),
            task_id: task.id.clone(),
            name: task.name.clone(),
            description: task.description.clone(),
            command: task.command.clone(),
            parameters: HashMap::new(),
            dependencies: dependency_run_ids,
            queue: task
                .queue
                .clone()
                .unwrap_or_else(|| DEFAULT_QUEUE.to_string()),
            evaluation_point: task.evaluation_point
        };

        tasks.push(task_instance);
    } else {
        warn!("Task {} not found in StateManager!", task_id);
    }
}

/// Computes a 64-bit hash from the given data using XxHash64.
fn hash64(data: &[u8]) -> u64 {
    let mut hasher = XxHash64::default();
    data.hash(&mut hasher);
    hasher.finish()
}
