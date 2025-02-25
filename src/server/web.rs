use crate::server::dashboard::dashboard;
use crate::server::state::AppStateWithTera;
use crate::server::tasks::{schedule_task, tasks_api, tasks_page};
use crate::utils::app_state::AppState;
use axum::{
    routing::{get, post},
    Router,
};
use rust_embed::RustEmbed;
use std::sync::Arc;
use tera::Tera;
use tower_http::services::ServeDir;

/// Embedded static assets from the `static/` folder.
#[derive(RustEmbed, Clone)]
#[folder = "static/"]
pub struct StaticAssets;

/// Starts the Axum server.
pub async fn start_server(app_state: Arc<AppState>) -> std::io::Result<()> {
    // Initialize Tera templates
    let tera = Tera::new("templates/**/*.html").expect("Failed to load templates");
    let tera_state = Arc::new(tera);

    // Build application state
    let state = AppStateWithTera {
        app_state,
        tera: tera_state,
    };

    // Build our application with routes
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/tasks", get(tasks_page))
        .route("/api/tasks", get(tasks_api))
        .route("/api/schedule-task", post(schedule_task))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state);

    // Run the server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
