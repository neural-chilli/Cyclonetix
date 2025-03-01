use crate::graph::orchestrator::schedule_dag_from_task;
use crate::models;
use crate::server::state::AppStateWithTera;
use axum::{
    extract::{Json, State, Path},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use petgraph::visit::IntoNodeReferences;
use serde::{Deserialize, Serialize};
use crate::graph::cache::GRAPH_CACHE;
use tera::Context;

#[derive(Deserialize)]
pub struct ScheduleTaskRequest {
    task_id: String,
    env_vars: std::collections::HashMap<String, String>,
}

#[derive(Serialize)]
pub struct ScheduleTaskResponse {
    success: bool,
    error: Option<String>,
    dag_run_id: Option<String>,
}

pub async fn tasks_page(State(state): State<AppStateWithTera>) -> impl IntoResponse {
    let state_manager = &state.app_state.state_manager;

    // Get all task templates
    let tasks = state_manager.load_all_tasks().await;

    // Prepare template context
    let mut context = Context::new();
    context.insert("tasks", &tasks);

    // Render the tasks template
    match state.tera.lock() {
        Ok(tera) => {
            match tera.render("tasks.html", &context) {
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

pub async fn tasks_api(State(state): State<AppStateWithTera>) -> impl IntoResponse {
    let state_manager = &state.app_state.state_manager;
    let tasks = state_manager.load_all_tasks().await;

    (StatusCode::OK, Json(tasks)).into_response()
}

/// Render the DAG visualization page
pub async fn dag_view_page(State(state): State<AppStateWithTera>) -> impl IntoResponse {
    // Prepare template context
    let mut context = Context::new();
    
    // Render the dag view template
    match state.tera.lock() {
        Ok(tera) => {
            match tera.render("dag_view.html", &context) {
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
    let response = DagApiResponse {
        dag,
        tasks,
        graph,
    };
    
    (StatusCode::OK, Json(response)).into_response()
}

pub async fn schedule_task(
    State(state): State<AppStateWithTera>,
    Json(request): Json<ScheduleTaskRequest>,
) -> impl IntoResponse {
    let state_manager = &state.app_state.state_manager;

    // Load the task template
    if let Some(_task_template) = state_manager.load_task(&request.task_id).await {
        // Create a context with the provided environment variables
        let mut context = models::context::Context::new();
        context.variables = request.env_vars.clone();

        // Create execution graph from the task and get the run_id
        let dag_run_id = schedule_dag_from_task(state_manager.clone(), &request.task_id, Some(context)).await;

        (
            StatusCode::OK,
            Json(ScheduleTaskResponse {
                success: true,
                error: None,
                dag_run_id: Some(dag_run_id),
            }),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ScheduleTaskResponse {
                success: false,
                error: Some("Task template not found".to_string()),
                dag_run_id: None,
            }),
        )
            .into_response()
    }
}
