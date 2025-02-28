use crate::graph::orchestrator::schedule_dag_from_task;
use crate::models;
use crate::server::state::AppStateWithTera;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use serde::{Deserialize, Serialize};
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

        // Create execution graph from the task
        schedule_dag_from_task(state_manager.clone(), &request.task_id, Some(context)).await;

        (
            StatusCode::OK,
            Json(ScheduleTaskResponse {
                success: true,
                error: None,
            }),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ScheduleTaskResponse {
                success: false,
                error: Some("Task template not found".to_string()),
            }),
        )
            .into_response()
    }
}
