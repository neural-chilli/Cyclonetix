use crate::models::context::Context;
use crate::models::dag::{DagTemplate, GraphInstance};
use crate::graph::cache::GRAPH_CACHE;
use crate::graph::graph_manager;
use crate::state::state_manager::{StateManager, TaskPayload};
use crate::utils::constants::{COMPLETED_STATUS, PENDING_STATUS};
use futures::stream::{FuturesUnordered, StreamExt};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use twox_hash::XxHash64;

/// Builds a mutable DagInstance from a given root task by resolving dependencies.
pub async fn schedule_dag_from_task<S: StateManager + ?Sized>(
    state_manager: Arc<S>,
    root_task_id: &str,
    provided_context: Option<Context>,
) {
    let tasks = state_manager.load_all_tasks().await;
    let root_task = state_manager
        .load_task(root_task_id)
        .await
        .unwrap();
    let (dag, tasks, graph) = graph_manager::ExecutionGraph::create_execution_from_task(
        &root_task,
        &tasks,
        provided_context.clone(),
    );
    state_manager.save_dag_instance(&dag).await;
    state_manager.save_graph_instance(&graph).await;
    GRAPH_CACHE.insert(dag.run_id.clone(), graph.clone());
    for task in tasks {
        state_manager.save_task_instance(&task).await;
    }
    evaluate_graph(state_manager, &graph).await;
}

pub async fn schedule_dag<S: StateManager + ?Sized>(
    state_manager: Arc<S>,
    dag_template: DagTemplate,
    provided_context: Option<Context>,
) {
    let (dag, tasks, graph) =
        graph_manager::ExecutionGraph::create_execution_from_dag(dag_template, provided_context.clone());
    state_manager.save_dag_instance(&dag).await;
    state_manager.save_graph_instance(&graph).await;
    GRAPH_CACHE.insert(dag.run_id.clone(), graph.clone());
    for task in tasks {
        state_manager.save_task_instance(&task).await;
    }
    evaluate_graph(state_manager, &graph).await;
}

pub async fn evaluate_graph<S: StateManager + ?Sized>(
    state_manager: Arc<S>,
    graph_instance: &GraphInstance,
) {
    // Log the evaluation start.
    debug!("Evaluating GraphInstance: {}", graph_instance.run_id);

    // Get the immutable dependency graph.
    let exec_graph = &graph_instance.graph;

    // Ask the execution graph which tasks are executable based on current mutable statuses.
    let ready_task_ids = exec_graph
        .get_executable_tasks(&state_manager, graph_instance)
        .await;

    for task_run_id in ready_task_ids {
        let current_status = state_manager
            .load_task_status(&task_run_id)
            .await
            .unwrap_or_else(|| PENDING_STATUS.to_string());

        if current_status != PENDING_STATUS {
            debug!(
                "Task {} is in status '{}', skipping.",
                task_run_id, current_status
            );
            continue;
        }

        debug!("Task {} is ready, scheduling it...", task_run_id);

        // Mark the task as queued.
        state_manager
            .save_task_status(&task_run_id, "queued")
            .await;

        // Retrieve task definition (or summary) so that we can extract details like command and queue.
        if let Some(task_def) = state_manager.load_task_instance(&task_run_id).await {
            let env_vars = state_manager
                .load_dag_instance(&graph_instance.run_id)
                .await
                .map(|dag| dag.context.get_task_env(task_def.clone()))
                .unwrap_or_default();

            let task_payload = TaskPayload {
                task_run_id: task_run_id.clone(),
                dag_run_id: graph_instance.run_id.clone(),
                command: task_def.command.clone(),
                env_vars,
            };

            // Use the trait method queue() to get a &str.
            state_manager
                .put_work_on_queue(&task_payload, &task_def.queue)
                .await;
        } else {
            warn!("No task definition found for task id {}.", task_run_id);
        }
    }
}

fn hash64(data: &[u8]) -> u64 {
    let mut hasher = XxHash64::default();
    data.hash(&mut hasher);
    hasher.finish()
}

pub async fn recover_orchestrator<S: StateManager + ?Sized>(state_manager: Arc<S>) {
    info!("Orchestrator recovery: Checking scheduled DagInstances...");
    let dag_instances = state_manager.load_scheduled_dag_instances().await;
    for dag_instance in &dag_instances {
        if let Some(graph_instance) = state_manager.load_graph_instance(&dag_instance.run_id).await {
            GRAPH_CACHE.insert(dag_instance.run_id.clone(), graph_instance);
        }
    }
    let cache_snapshot = GRAPH_CACHE
        .iter()
        .map(|entry| entry.value().clone())
        .collect::<Vec<GraphInstance>>();
    let orchestrator_count = state_manager.get_orchestrator_count().await.unwrap_or(1);
    let orchestrator_id = state_manager.get_orchestrator_id().await.unwrap_or(0);
    for graph_instance in cache_snapshot {
        let assigned_orchestrator =
            (hash64(graph_instance.run_id.as_bytes()) % orchestrator_count as u64) as u32;
        if assigned_orchestrator == orchestrator_id {
            info!(
                "Orchestrator is responsible for GraphInstance: {}",
                graph_instance.run_id
            );
            let graph_status = state_manager
                .load_dag_status(&graph_instance.run_id)
                .await
                .unwrap_or_else(|_| PENDING_STATUS.to_string());
            if graph_status != COMPLETED_STATUS {
                info!("Recovering GraphInstance: {}", graph_instance.run_id);
                evaluate_graph(state_manager.clone(), &graph_instance).await;
            } else {
                debug!(
                    "GraphInstance {} is completed. Skipping.",
                    graph_instance.run_id
                );
            }
        } else {
            info!(
                "Skipping GraphInstance {} (handled by another orchestrator)",
                graph_instance.run_id
            );
        }
    }
    info!("Orchestrator recovery complete.");
}

pub async fn monitor_scheduled_graphs<S: StateManager + ?Sized + 'static>(state_manager: Arc<S>) {
    loop {
        debug!("Checking for new scheduled GraphInstances...");
        let keys = state_manager.load_scheduled_dag_keys().await;
        for key in &keys {
            if !GRAPH_CACHE.contains_key(key) {
                if let Some(graph_instance) = state_manager.load_graph_instance(key).await {
                    GRAPH_CACHE.insert(key.clone(), graph_instance);
                }
            }
        }
        let cached_graphs = GRAPH_CACHE
            .iter()
            .map(|entry| entry.value().clone())
            .collect::<Vec<GraphInstance>>();
        let mut evaluations = FuturesUnordered::new();
        for graph_instance in cached_graphs {
            let sm = state_manager.clone();
            evaluations.push(tokio::spawn(async move {
                debug!("Processing GraphInstance: {}", graph_instance.run_id);
                evaluate_graph(sm.clone(), &graph_instance).await;
            }));
        }
        while let Some(res) = evaluations.next().await {
            if let Err(e) = res {
                error!("Error processing GraphInstance evaluation: {:?}", e);
            }
        }
        if let Err(e) = state_manager.reset_tasks_from_downed_agents(15).await {
            error!("Error resetting tasks from downed agents: {}", e);
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}
