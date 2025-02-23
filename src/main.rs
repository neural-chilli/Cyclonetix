use clap::Parser;
use cyclonetix::utils::app_state::AppState;
use cyclonetix::utils::cli::{ensure_config_exists, handle_scheduling, Cli};
use cyclonetix::utils::constants::DEFAULT_QUEUE;
use cyclonetix::utils::logging::init_tracing;
use cyclonetix::{
    agent::agent::Agent,
    graph::orchestrator::*,
    server,
    state::bootstrapper::Bootstrapper,
    state::redis_state_manager::RedisStateManager,
    state::state_manager::StateManager,
    utils::config::CyclonetixConfig,
};
use std::sync::Arc;
use tracing::{error, info};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_tracing();
    info!("Cyclonetix is starting...");
    let cli = Cli::parse();
    ensure_config_exists(&cli.config_file);
    let config = Arc::new(CyclonetixConfig::load(&cli.config_file));
    let state_manager: Arc<dyn StateManager> = match config.backend.as_str() {
        "redis" => Arc::new(RedisStateManager::new(&config.backend_url, &config.cluster_id).await),
        "memory" => unimplemented!("Memory backend"),
        "postgresql" => unimplemented!("PostgreSQL backend"),
        other => panic!("Unsupported backend: {}", other),
    };
    let bootstrapper = Bootstrapper::new(state_manager.clone());
    bootstrapper.bootstrap(&config).await;
    let app_state = Arc::new(AppState {
        state_manager: state_manager.clone(),
    });
    if let Some(command) = cli.command {
        handle_scheduling(command, state_manager.clone()).await;
        return Ok(());
    }
    let mut handles = Vec::new();
    if !cli.ui && !cli.agent && !cli.orchestrator {
        info!("No specific services enabled, running all in development mode.");
        handles.push(tokio::spawn(start_agent(app_state.clone(), config.clone())));
        handles.push(tokio::spawn(start_orchestrator(app_state.clone())));
        handles.push(tokio::spawn(start_graph_monitor(app_state.clone())));
        handles.push(tokio::spawn(start_update_listener(app_state.clone())));
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
            handles.push(tokio::spawn(start_graph_monitor(app_state.clone())));
            handles.push(tokio::spawn(start_update_listener(app_state.clone())));
        }
    }
    futures::future::join_all(handles).await;
    Ok(())
}

async fn start_ui(app_state: Arc<AppState>) {
    info!("Starting Cyclonetix UI Server...");
    if let Err(e) = server::web::start_server(app_state).await {
        error!("UI server failed: {}", e);
    }
}

async fn start_agent(app_state: Arc<AppState>, config: Arc<CyclonetixConfig>) {
    info!("Starting agent process...");
    let agent = Arc::new(Agent::new(app_state.state_manager.clone()));
    let queues = if config.queues.is_empty() { vec![DEFAULT_QUEUE.to_string()] } else { config.queues.clone() };
    agent.run(queues).await;
}

async fn start_orchestrator(app_state: Arc<AppState>) {
    info!("Starting orchestrator process...");
    recover_orchestrator(app_state.state_manager.clone()).await;
}

async fn start_graph_monitor(app_state: Arc<AppState>) {
    info!("Starting graph monitor process...");
    monitor_scheduled_graphs(app_state.state_manager.clone()).await;
}

async fn start_update_listener(app_state: Arc<AppState>) {
    info!("Starting update listener...");
    app_state.state_manager.clone().subscribe_to_graph_updates().await;
}
