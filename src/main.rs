use clap::Parser;
use cyclonetix::utils::app_state::AppState;
use cyclonetix::utils::cli::{ensure_config_exists, handle_scheduling, Cli};
use cyclonetix::utils::logging::init_tracing;
use cyclonetix::{
    orchestrator::orchestrator::*, server, state::bootstrapper::Bootstrapper,
    state::redis_state_manager::RedisStateManager, state::state_manager::StateManager,
    utils::config::CyclonetixConfig, agent::agent::Agent,
};
use std::sync::Arc;
use tracing::{error, info};
use cyclonetix::utils::constants::DEFAULT_QUEUE;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging.
    init_tracing();
    info!("Cyclonetix is starting...");

    // Parse CLI arguments.
    let cli = Cli::parse();

    // Ensure the config file exists and load configuration.
    ensure_config_exists(&cli.config_file);
    let config = Arc::new(CyclonetixConfig::load(&cli.config_file));

    // Initialize state manager based on the config.
    let state_manager: Arc<dyn StateManager> = match config.backend.as_str() {
        "redis" => Arc::new(RedisStateManager::new(&config.backend_url).await),
        "memory" => unimplemented!("In-memory backend"),
        "postgresql" => unimplemented!("PostgreSQL backend"),
        other => panic!("Unsupported backend: {}", other),
    };

    // Bootstrap the state.
    let bootstrapper = Bootstrapper::new(state_manager.clone());
    bootstrapper.bootstrap(&config).await;

    // Retrieve the default context ID.
    let default_context_id = state_manager
        .get_context(config.default_context.as_str())
        .await
        .unwrap_or_default()
        .id;

    let app_state = Arc::new(AppState {
        state_manager: state_manager.clone(),
        default_context_id,
    });

    // Handle scheduling command if provided.
    if let Some(command) = cli.command {
        handle_scheduling(command, state_manager.clone()).await;
        return Ok(()); // Exit cleanly instead of calling exit(0)
    }

    // Spawn services based on CLI flags or run all in development mode.
    let mut handles = Vec::new();

    if !cli.ui && !cli.agent && !cli.orchestrator {
        info!("No specific services enabled, running all in development mode.");
        handles.push(tokio::spawn(start_agent(app_state.clone(), config.clone())));
        handles.push(tokio::spawn(start_orchestrator(app_state.clone())));
        handles.push(tokio::spawn(start_dag_monitor(app_state.clone())));
        handles.push(tokio::spawn(start_update_listener(app_state.clone())));
        // Run UI in the main task.
        start_ui(app_state.clone()).await;
    } else {
        if cli.ui {
            handles.push(tokio::spawn(start_ui(app_state.clone())));
        }
        if cli.agent {
            handles.push(tokio::spawn(start_agent(app_state.clone(), config.clone())));
        }
        if cli.orchestrator {
            handles.push(tokio::spawn(start_orchestrator(app_state.clone())));
            handles.push(tokio::spawn(start_dag_monitor(app_state.clone())));
            handles.push(tokio::spawn(start_update_listener(app_state.clone())));
        }
    }
    
    // Wait for all services to finish.
    futures::future::join_all(handles).await;

    Ok(())
}

/// Starts the UI server.
async fn start_ui(app_state: Arc<AppState>) {
    info!("Starting Cyclonetix UI Server...");
    if let Err(e) = server::web::start_server(app_state).await {
        error!("UI server failed: {}", e);
    }
}

/// Starts an agent process.
async fn start_agent(app_state: Arc<AppState>, config: Arc<CyclonetixConfig>) {
    info!("Starting agent process...");

    let agent = Arc::new(Agent::new(app_state.state_manager.clone()));

    // Get queues from config, fallback to default if empty
    let queues = if config.queues.is_empty() {
        vec![DEFAULT_QUEUE.to_string()]
    } else {
        config.queues.clone()
    };

    agent.run(queues).await;
}

/// Starts an orchestrator process.
async fn start_orchestrator(app_state: Arc<AppState>) {
    info!("Starting orchestrator process...");
    recover_orchestrator(app_state.state_manager.clone()).await;
}

/// Starts a DAG monitor process.
async fn start_dag_monitor(app_state: Arc<AppState>) {
    info!("Starting DAG monitor process...");
    monitor_scheduled_dags(app_state.state_manager.clone()).await;
}

async fn start_update_listener(app_state: Arc<AppState>) {
    info!("Starting update listener...");
    app_state.state_manager.clone().listen_for_graph_updates().await;
}
