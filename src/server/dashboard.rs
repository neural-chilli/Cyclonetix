use crate::server::state::AppStateWithTera;
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
    pub dag_id: String,
    pub dag_name: String,
    pub status: String,
    pub pending_tasks: usize,
}

/// Handler for the dashboard page.
pub async fn dashboard(State(state): State<AppStateWithTera>) -> impl IntoResponse {
    let state_manager = &state.app_state.state_manager;

    // Get agents and convert to overview type.
    let agents_raw = state_manager.load_all_agents().await;
    let agents: Vec<AgentOverview> = agents_raw
        .into_iter()
        .map(|agent| AgentOverview {
            agent_id: agent.agent_id,
            last_heartbeat: agent.last_heartbeat,
            tasks: agent.tasks.clone(),
            task_count: agent.tasks.len(),
        })
        .collect();

    let agents_count = agents.len();

    // Fixed list of queues for demonstration.
    let work_queue_tasks = state_manager.get_queue_tasks("work_queue").await;
    let queues = vec![QueueInfo {
        name: "work_queue".to_string(),
        task_count: work_queue_tasks.len(),
    }];

    // Retrieve mutable DagInstances.
    let scheduled = state_manager.load_scheduled_dag_instances().await;
    let scheduled_dags_count = scheduled.len();
    let total_queue_tasks = work_queue_tasks.len();

    // Populate the Tera context.
    let mut context = Context::new();
    context.insert("agents", &agents);
    context.insert("queues", &queues);
    context.insert("total_queue_tasks", &total_queue_tasks);
    context.insert("agent_count", &agents_count);
    context.insert("scheduled_dags_count", &scheduled_dags_count);

    // Render the dashboard template.
    match state.tera.lock() {
        Ok(tera) => {
            match tera.render("dashboard.html", &context) {
                Ok(html) => Html(html).into_response(),
                Err(err) => {
                    tracing::error!("Template rendering error: {}", err);
                    (StatusCode::INTERNAL_SERVER_ERROR, "Error rendering page").into_response()
                }
            }
        },
        Err(err) => {
            tracing::error!("Failed to lock Tera instance: {}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "Server error").into_response()
        }
    }
}
