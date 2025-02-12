mod server;

use actix_web::main;
use tokio::task;
use std::sync::Arc;
use clap::{Parser, Subcommand};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{error, info, Level};
use tracing_subscriber::EnvFilter;
use cyclonetix::{
    models::dag::ScheduledDag,
    models::outcome::Outcome,
    orchestrator::orchestrator::*,
    state::bootstrapper::Bootstrapper,
    state::redis_state_manager::RedisStateManager,
    state::state_manager::StateManager,
    utils::config::CyclonetixConfig,
    worker::worker::Worker,
};
use cyclonetix::utils::cli::{schedule_commands, Cli};
use cyclonetix::utils::logging::init_tracing;


struct AppState {
    state_manager: Arc<dyn StateManager>,
    default_context_id: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging.
    init_tracing();

    info!("Cyclonetix is starting...");

    // Parse CLI arguments.
    let cli = Cli::parse();
    info!("Using config file: {}", cli.config_file);

    // Load configuration from the given file.
    let config = CyclonetixConfig::load(&cli.config_file);

    // Choose the state backend based on config.
    let state_manager: Arc<dyn StateManager> = match config.backend.as_str() {
        "redis" => Arc::new(RedisStateManager::new(&config.backend_url).await),
        other => panic!("Unsupported backend: {}", other),
    };

    // Retrieve the default context ID.
    let default_context_id = state_manager
        .get_context(config.default_context.as_str())
        .await
        .unwrap_or_default()
        .id;

    let app_state = Arc::new(AppState {
        state_manager: state_manager.clone(),
        default_context_id: default_context_id.clone(),
    });

    // Spawn orchestrator scheduling.
    {
        info!("Starting Cyclonetix orchestrator");
        let state = app_state.clone();
        task::spawn({
            async move {
                orchestrate(state.state_manager.clone(), Some(&state.default_context_id)).await;
            }
        });
    }

    // Monitor scheduled outcomes.
    {
        let state = app_state.clone();
        task::spawn(async move {
            monitor_scheduled_outcomes(state.state_manager.clone(), &state.default_context_id).await;
        });
    }

    // Listen for graph updates.
    {
        let state = app_state.clone();
        task::spawn(async move {
            state.state_manager.clone().listen_for_graph_updates().await;
        });
    }

    // Start the worker on the specified queue.
    {
        info!("Starting Cyclonetix worker");
        let worker = Arc::new(Worker::new(app_state.state_manager.clone()));
        let queue_name = cli.queue;
        let worker_clone = worker.clone();
        task::spawn(async move {
            worker_clone.run(&queue_name).await;
        });
    }

    // Start the Cyclonetix UI server.
    info!("Starting Cyclonetix UI Server");
    server::web::start_server().await
}