use crate::server::dashboard::DAGOverview;
use crate::server::state::AppStateWithTera;
use crate::utils::constants::{COMPLETED_STATUS, FAILED_STATUS, PENDING_STATUS, RUNNING_STATUS};
use crate::utils::id_tools::strip_guid;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
};
use tera::Context;

/// Handler for the dashboard page.
pub async fn running_dags(State(state): State<AppStateWithTera>) -> impl IntoResponse {
    let state_manager = &state.app_state.state_manager;

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
    context.insert("dags", &dags);
    context.insert("scheduled_dags_count", &scheduled_dags_count);

    // Render the dashboard template.
    match state.tera.lock() {
        Ok(tera) => match tera.render("running_dags.html", &context) {
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
