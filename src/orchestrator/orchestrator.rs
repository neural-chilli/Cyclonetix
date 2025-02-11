use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use twox_hash::XxHash64;

use crate::orchestrator::execution_graph::ExecutionGraph;
use crate::state::state_manager::StateManager;

/// Computes a 64-bit hash of the given data using `XxHash64`.
fn hash64(data: &[u8]) -> u64 {
    let mut hasher = XxHash64::default();
    data.hash(&mut hasher);
    hasher.finish()
}

/// Orchestrates tasks by retrieving them from the state manager, building the execution graph,
/// and scheduling executable tasks.
pub async fn orchestrate<S: StateManager + ?Sized>(
    state_manager: Arc<S>,
    context_id: Option<&str>,
) {
    let context_id = context_id.unwrap_or("default_context");

    info!("Retrieving tasks from StateManager...");
    let tasks = state_manager.get_all_tasks().await;
    info!("Retrieved {} tasks from storage.", tasks.len());

    let execution_graph = ExecutionGraph::from_tasks(tasks);
    execution_graph.print_graph();

    let execution_order = execution_graph.get_execution_order();
    debug!("Final Execution Order: {:?}", execution_order);

    let executable_tasks = execution_graph
        .get_executable_tasks(&state_manager, context_id)
        .await;
    info!("Tasks ready for execution: {:?}", executable_tasks);

    for task_id in executable_tasks {
        state_manager
            .enqueue_task_instance(&task_id, context_id)
            .await;
    }
}

/// Evaluates the execution graph (for example when a task completes) and schedules new tasks
/// if all their dependencies are met.
pub async fn evaluate_graph<S: StateManager + ?Sized>(state_manager: Arc<S>, context_id: &str) {
    info!("Evaluating graph for context: {}", context_id);

    let tasks = state_manager.get_all_tasks().await;
    let execution_graph = ExecutionGraph::from_tasks(tasks);

    let executable_tasks = execution_graph
        .get_executable_tasks(&state_manager, context_id)
        .await;

    let scheduled_outcomes = state_manager.get_scheduled_outcomes().await;
    let scheduled_dags = state_manager.get_scheduled_dags().await;

    info!("Scheduled Outcomes: {:?}", scheduled_outcomes);
    info!("Scheduled DAGs: {:?}", scheduled_dags);

    let mut final_executable_tasks = Vec::new();

    for task_id in executable_tasks {
        let status = state_manager
            .get_task_status(&task_id, context_id)
            .await
            .unwrap_or_else(|| "pending".to_string());

        if status == "completed" {
            info!("Task {} already completed, skipping scheduling...", task_id);
            continue;
        }

        if state_manager.is_task_scheduled(&task_id, context_id).await {
            info!("Task {} is already in the work queue, skipping...", task_id);
            continue;
        }

        // Ensure task is part of a scheduled outcome or DAG
        let is_part_of_scheduled_execution = scheduled_outcomes
            .iter()
            .any(|outcome| outcome.task_id == task_id)
            || scheduled_dags
                .iter()
                .any(|dag| dag.dag.tasks.iter().any(|t| t.id == task_id));

        if is_part_of_scheduled_execution {
            info!("Scheduling newly executable task: {}", task_id);
            state_manager
                .enqueue_task_instance(&task_id, context_id)
                .await;
            final_executable_tasks.push(task_id.clone());
        } else {
            info!(
                "Task {} is not part of any scheduled execution, skipping.",
                task_id
            );
        }
    }

    if final_executable_tasks.is_empty() {
        info!("No new tasks found for execution.");
    }
}

/// Attempts to recover the orchestrator by checking scheduled outcomes and explicit DAGs,
/// and then re-evaluates the execution graph for any outcome that still has incomplete tasks.
pub async fn recover_orchestrator<S: StateManager + ?Sized>(
    state_manager: Arc<S>,
    context_id: &str,
) {
    info!("Orchestrator recovery: Checking scheduled executions...");

    let outcomes = state_manager.get_scheduled_outcomes().await;
    let explicit_dags = state_manager.get_scheduled_dags().await;

    info!("Scheduled Outcomes: {:?}", outcomes);
    info!("Scheduled DAGs: {:?}", explicit_dags);

    let orchestrator_count = state_manager.get_orchestrator_count().await.unwrap_or(1);
    let orchestrator_id = state_manager.get_orchestrator_id().await.unwrap_or(0);

    if outcomes.is_empty() && explicit_dags.is_empty() {
        info!("No scheduled executions found. Recovery complete.");
        return;
    }

    for outcome in outcomes {
        let assigned_orchestrator =
            (hash64(outcome.run_id.as_bytes()) % orchestrator_count as u64) as u32;

        if assigned_orchestrator == orchestrator_id {
            info!(
                "This orchestrator is responsible for outcome: {}",
                outcome.run_id
            );

            let tasks = state_manager.get_all_tasks().await;
            let execution_graph = ExecutionGraph::from_tasks(tasks);

            let executable_tasks = execution_graph
                .get_executable_tasks(&state_manager, context_id)
                .await;

            if executable_tasks.is_empty() {
                info!(
                    "Outcome {} is already fully executed. Skipping.",
                    outcome.run_id
                );
            } else {
                info!("Rebuilding execution graph for outcome: {}", outcome.run_id);
                evaluate_graph(state_manager.clone(), context_id).await;
            }
        } else {
            info!(
                "Skipping outcome {} (Handled by another orchestrator)",
                outcome.run_id
            );
        }
    }

    for dag in explicit_dags {
        let dag_status = state_manager
            .get_dag_status(&dag)
            .await
            .unwrap_or_else(|_| "pending".to_string());
        if dag_status != "completed" {
            info!("Recovering explicit DAG: {}", dag.run_id);
            evaluate_graph(state_manager.clone(), context_id).await;
        } else {
            info!("DAG {} is already completed. Skipping.", dag.run_id);
        }
    }

    info!("Orchestrator recovery complete.");
}

/// Continuously monitors for scheduled outcomes, evaluates them, and removes outcomes that have
/// all tasks completed.
pub async fn monitor_scheduled_outcomes<S: StateManager + ?Sized>(
    state_manager: Arc<S>,
    context_id: &str,
) {
    loop {
        debug!(target: "orchestrator", "Checking for new scheduled outcomes...");

        let outcomes = state_manager.get_scheduled_outcomes().await;
        for outcome in outcomes {
            info!("Found new scheduled outcome: {}", outcome.run_id);

            let tasks = state_manager.get_all_tasks().await;
            let execution_graph = ExecutionGraph::from_tasks(tasks);

            // If there are no tasks ready to run, assume all tasks are complete and remove the outcome.
            let executable_tasks = execution_graph
                .get_executable_tasks(&state_manager, context_id)
                .await;
            if executable_tasks.is_empty() {
                info!(
                    "Outcome {} is fully executed. Removing from scheduled outcomes.",
                    outcome.run_id
                );
                state_manager
                    .remove_scheduled_outcome(&outcome.run_id)
                    .await;
            } else {
                evaluate_graph(state_manager.clone(), context_id).await;
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
