use crate::server::dashboard::DAGOverview;
use crate::server::state::AppStateWithTera;
use crate::utils::constants::{COMPLETED_STATUS, FAILED_STATUS, PENDING_STATUS, RUNNING_STATUS};
use crate::utils::id_tools::strip_guid;
use axum::{extract::State, http::StatusCode, response::{Html, IntoResponse}, Json};
use axum::extract::Path;
use serde::Serialize;
use tera::Context;
use crate::graph::cache::GRAPH_CACHE;
use crate::models;

/// Render the DAG visualization page
pub async fn dag_view_page(State(state): State<AppStateWithTera>) -> impl IntoResponse {
    // Prepare template context
    let context = Context::new();

    // Render the dag view template
    match state.tera.lock() {
        Ok(tera) => match tera.render("dag_view.html", &context) {
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

/// DAG API response containing all data needed to visualize a DAG
#[derive(Serialize)]
pub struct DagApiResponse {
    dag: Option<models::dag::DagInstance>,
    tasks: Vec<models::task::TaskInstance>,
    graph: Option<models::dag::GraphInstance>,
}

/// Endpoint to get DAG data for visualization
pub async fn dag_api(
    State(state): State<AppStateWithTera>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    let state_manager = &state.app_state.state_manager;

    // Load the DAG instance
    let dag = state_manager.load_dag_instance(&run_id).await;

    // Get all task instances related to this DAG
    let mut tasks = Vec::new();

    // Attempt to get the graph from the cache or load it from storage
    let graph = if let Some(graph_ref) = GRAPH_CACHE.get(&run_id) {
        // We're getting a graph from the cache - just clone the entire GraphInstance
        Some(graph_ref.clone())
    } else {
        state_manager.load_graph_instance(&run_id).await
    };

    // If we have a graph, fetch all tasks referenced by it
    if let Some(graph_instance) = &graph {
        for task_id in graph_instance.graph.graph.node_weights() {
            if let Some(task) = state_manager.load_task_instance(task_id).await {
                tasks.push(task);
            }
        }
    }

    // Build the response
    let response = DagApiResponse { dag, tasks, graph };

    (StatusCode::OK, Json(response)).into_response()
}

