use crate::server::dashboard::AgentOverview;
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

/// Handler for the dashboard page.
pub async fn agents(State(state): State<AppStateWithTera>) -> impl IntoResponse {
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

    // Populate the Tera context.
    let mut context = Context::new();
    context.insert("agents", &agents);

    // Render the dashboard template.
    match state.tera.lock() {
        Ok(tera) => match tera.render("agent_list.html", &context) {
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
