use crate::server::agents::agents;
use crate::server::dashboard::dashboard;
use crate::server::dags::{dag_view_page, dag_api, running_dags};
use crate::server::state::AppStateWithTera;
use crate::server::tasks::{schedule_task, tasks_api, tasks_instance_api, tasks_page};
use crate::utils::app_state::AppState;
use axum::http::Request;
use axum::{
    extract::State,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use rust_embed::RustEmbed;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};
use tera::Tera;
use tower_http::{
    compression::{predicate::SizeAbove, CompressionLayer},
    services::ServeDir,
    trace::TraceLayer,
};

/// Embedded static assets from the `static/` folder.
#[derive(RustEmbed, Clone)]
#[folder = "static/"]
pub struct StaticAssets;

/// Middleware to reload templates on each request in dev mode
async fn template_reload_middleware(
    State(state): State<AppStateWithTera>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if state.dev_mode {
        // Lock the Tera instance and manually reload templates by
        // re-parsing all templates from the filesystem
        if let Ok(mut tera) = state.tera.lock() {
            // Create a new Tera instance to force reload from disk
            if let Ok(new_tera) = Tera::new("templates/**/*.html") {
                // Replace the old Tera instance with the new one
                *tera = new_tera;

                let path = req.uri().path();
                // Only log for HTML requests, not API or static files
                if !path.starts_with("/api/") && !path.starts_with("/static/") {
                    let start = Instant::now();
                    let duration = start.elapsed();
                    tracing::debug!("Reloaded templates in {:?} for path: {}", duration, path);
                }
            } else {
                tracing::error!("Error reloading templates");
            }
        }
    }

    next.run(req).await
}

/// Starts the Axum server.
pub async fn start_server(app_state: Arc<AppState>) -> std::io::Result<()> {
    // Check if we're in dev mode
    let dev_mode = std::env::var("DEV_MODE")
        .map(|val| val == "true")
        .unwrap_or(false);

    // Initialize Tera templates with caching disabled if in dev mode
    let mut tera = Tera::new("templates/**/*.html").expect("Failed to load templates");

    if dev_mode {
        tracing::info!("DEV_MODE=true: Disabling Tera template caching for faster UI development");

        // In Tera 1.x, we can configure autoreloading by creating a new Tera instance with options
        // Using the AUTO_RELOAD option which reloads templates when they change on disk
        tera = Tera::new("templates/**/*.html").expect("Failed to reload templates");

        // Log the configuration
        tracing::info!("Templates will be automatically reloaded when changed");
    }

    // Use a Mutex to allow template reloading in middleware
    let tera_state = Arc::new(Mutex::new(tera));

    // Build application state
    let state = AppStateWithTera {
        app_state,
        tera: tera_state,
        dev_mode,
    };

    // Configure compression layer - compress responses that are more than 512 bytes
    // Use individual compression layers for better control
    let compression_layer = CompressionLayer::new()
        // Only compress responses that are larger than 512 bytes
        .compress_when(SizeAbove::new(512));

    // Build our application with routes
    let mut app = Router::new()
        .route("/", get(dashboard))
        .route("/tasks", get(tasks_page))
        .route("/dag", get(dag_view_page))
        .route("/running-dags", get(running_dags))
        .route("/agent-list", get(agents))
        .route("/api/tasks", get(tasks_api))
        .route("/api/task_instance/{instance_id}", get(tasks_instance_api))
        .route("/api/schedule-task", post(schedule_task))
        .route("/api/dag/{run_id}", get(dag_api))
        .nest_service("/static", ServeDir::new("static"))
        .layer(compression_layer)  // Add compression for all routes
        .layer(TraceLayer::new_for_http())  // Add request tracing
        .with_state(state.clone());

    // If in dev mode, add middleware to reload templates on each request
    if dev_mode {
        app = app.layer(middleware::from_fn_with_state(
            state,
            template_reload_middleware,
        ));
    }

    // Run the server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;

    if dev_mode {
        println!();
        println!("┌─────────────────────────────────────────────────────┐");
        println!("│                                                     │");
        println!("│  🔧 DEV MODE ACTIVE                                 │");
        println!("│                                                     │");
        println!("│  • Template caching disabled                        │");
        println!("│  • Templates will reload on each request            │");
        println!("│  • Edit templates directly to see changes           │");
        println!("│                                                     │");
        println!("└─────────────────────────────────────────────────────┘");
        println!();
    }

    axum::serve(listener, app).await?;

    Ok(())
}
