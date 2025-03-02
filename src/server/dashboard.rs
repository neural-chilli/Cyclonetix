use crate::server::state::AppStateWithTera;
use crate::utils::constants::{COMPLETED_STATUS, FAILED_STATUS, PENDING_STATUS, RUNNING_STATUS};
use crate::utils::id_tools::strip_guid;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
};
use serde::Serialize;
use tera::Context;

/// A simplified view of an agent.
#[derive(Serialize)]
pub struct AgentOverview {
    pub agent_id: String,
    pub display_id: String,
    pub last_heartbeat: i64,
    pub tasks: Vec<String>,
    pub task_count: usize,
}

/// Information about a queue.
#[derive(Serialize)]
pub struct QueueInfo {
    pub name: String,
    pub task_count: usize,
}

/// A summary of a scheduled DAG.
#[derive(Serialize)]
pub struct DAGOverview {
    pub run_id: String,
    pub display_id: String,
    pub dag_id: String,
    pub status: String,
    pub status_class: String,
    pub task_count: usize,
    pub completed_tasks: usize,
    pub progress_percent: f32,
    pub last_updated: String,
}

/// Handler for the dashboard page.
pub async fn dashboard(State(state): State<AppStateWithTera>) -> impl IntoResponse {
    let state_manager = &state.app_state.state_manager;

    // Get agents and convert to overview type.
    let agents_raw = state_manager.load_all_agents().await;
    let agents: Vec<AgentOverview> = agents_raw
        .into_iter()
        .map(|agent| AgentOverview {
            display_id: strip_guid(agent.agent_id.as_str()),
            agent_id: agent.agent_id.clone(),
            last_heartbeat: agent.last_heartbeat,
            tasks: agent.tasks.clone(),
            task_count: agent.tasks.len(),
        })
        .collect();

    let agents_count = agents.len();

    // Get queue information
    let queue_names = ["work_queue", "high_memory", "gpu_tasks", "default"];
    let mut queues = Vec::new();

    for queue_name in queue_names.iter() {
        let tasks = state_manager.get_queue_tasks(queue_name).await;
        queues.push(QueueInfo {
            name: queue_name.to_string(),
            task_count: tasks.len(),
        });
    }

    // Calculate total tasks in all queues
    let total_queue_tasks: usize = queues.iter().map(|q| q.task_count).sum();

    // Retrieve mutable DagInstances.
    let scheduled = state_manager.load_scheduled_dag_instances().await;
    let scheduled_dags_count = scheduled.len();

    // Convert DAG instances to dashboard-friendly overview objects
    let dags: Vec<DAGOverview> = scheduled
        .iter()
        .map(|dag| {
            let progress_percent = if dag.task_count == 0 {
                0.0
            } else {
                (dag.completed_tasks as f32 / dag.task_count as f32) * 100.0
            };

            // Determine CSS class for status
            let status_class = match dag.status.as_str() {
                s if s == PENDING_STATUS => "secondary",
                s if s == RUNNING_STATUS => "primary",
                s if s == COMPLETED_STATUS => "success",
                s if s == FAILED_STATUS => "danger",
                _ => "info",
            };

            DAGOverview {
                run_id: dag.run_id.clone(),
                display_id: strip_guid(&dag.run_id),
                dag_id: dag.dag_id.clone(),
                status: dag.status.clone(),
                status_class: status_class.to_string(),
                task_count: dag.task_count,
                completed_tasks: dag.completed_tasks,
                progress_percent,
                last_updated: dag.last_updated.format("%Y-%m-%d %H:%M:%S").to_string(),
            }
        })
        .collect();

    // Populate the Tera context.
    let mut context = Context::new();
    context.insert("agents", &agents);
    context.insert("queues", &queues);
    context.insert("dags", &dags);
    context.insert("total_queue_tasks", &total_queue_tasks);
    context.insert("agent_count", &agents_count);
    context.insert("scheduled_dags_count", &scheduled_dags_count);

    // Render the dashboard template.
    match state.tera.lock() {
        Ok(tera) => match tera.render("dashboard.html", &context) {
            Ok(html) => Html(html).into_response(),
            Err(err) => {
                tracing::error!("Template rendering error: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Error rendering page").into_response()
            }
        },
        Err(err) => {
            tracing::error!("Failed to lock Tera instance: {}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "Server error").into_response()
        }
    }
}
