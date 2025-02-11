use axum::{
    body::Body,
    extract::Path,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use chrono::Utc;
use clap::{Parser, Subcommand};
use cyclonetix::{
    models::dag::ScheduledDag, models::outcome::Outcome, orchestrator::orchestrator::*,
    state::bootstrapper::Bootstrapper, state::redis_state_manager::RedisStateManager,
    state::state_manager::StateManager, utils::config::CyclonetixConfig, worker::worker::Worker,
};
use mime_guess::from_path;
use rust_embed::RustEmbed;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::task;
use tracing::{error, info, Level};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

/// Embeds files from the `static/` folder.
#[derive(RustEmbed)]
#[folder = "static/"]
struct StaticAssets;

/// CLI arguments and subcommands.
#[derive(Parser)]
#[command(
    name = "Cyclonetix",
    about = "A lightweight Rust-based workflow orchestrator"
)]
struct Cli {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.yaml")]
    config_file: String,

    /// Name of the queue for the worker to listen on
    #[arg(short, long, default_value = "work_queue")]
    queue: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

/// Supported scheduling commands.
#[derive(Subcommand)]
enum Commands {
    /// Schedule an outcome-task (the final desired end state)
    ScheduleOutcome {
        /// Outcome ID to schedule (must match one of the tasks loaded by the bootstrapper)
        task_id: String,
        #[arg(long, value_parser = parse_key_val)]
        context_overrides: Vec<(String, String)>,
    },
    /// Schedule a predefined DAG (loaded from YAML by the bootstrapper)
    ScheduleDag {
        /// DAG ID to schedule.
        dag_id: String,
        /// Additional context overrides in key=value format.
        #[arg(long, value_parser = parse_key_val)]
        context_overrides: Vec<(String, String)>,
    },
}

#[tokio::main]
async fn main() {
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

    // Handle scheduling commands (if provided) and then exit.
    if schedule_commands(&cli, &config, &state_manager).await {
        return;
    }

    // Create the bootstrapper which loads predefined DAGs and contexts.
    let bootstrapper = Bootstrapper::new(state_manager.clone());

    // Perform bootstrap: load tasks, DAGs, and contexts from YAML.
    bootstrapper.bootstrap(&config).await;

    // Retrieve the default context ID
    let default_context_id = state_manager
        .get_context(config.default_context.as_str())
        .await
        .unwrap_or_default()
        .id;

    // Clone before moving into async tasks
    let default_context_clone_1 = default_context_id.clone();
    let default_context_clone_2 = default_context_id.clone();

    // Recover orchestrator state
    recover_orchestrator(state_manager.clone(), &default_context_id).await;

    // Spawn orchestrator scheduling
    let sm_for_orchestrator = state_manager.clone();
    task::spawn(async move {
        orchestrate(sm_for_orchestrator, Some(&default_context_clone_1)).await;
    });

    // Monitor scheduled outcomes in the background.
    let sm_for_monitor = state_manager.clone();
    task::spawn(async move {
        monitor_scheduled_outcomes(sm_for_monitor, &default_context_clone_2).await;
    });

    // Listen for graph updates.
    let sm_for_listener = state_manager.clone();
    task::spawn(async move {
        sm_for_listener.listen_for_graph_updates().await;
    });

    // Start the worker on the queue specified via CLI.
    let worker = Arc::new(Worker::new(state_manager.clone()));
    let queue_name = cli.queue;
    let worker_clone = Arc::clone(&worker);
    task::spawn(async move {
        worker_clone.run(&queue_name).await;
    });

    // Start the Cyclonetix UI server.
    info!("Starting Cyclonetix UI Server");
    let app = Router::new()
        .route(
            "/",
            get(|| async { serve_static(Path("index.html".to_string())).await }),
        )
        .route(
            "/dashboard",
            get(|| async { serve_static(Path("dashboard.html".to_string())).await }),
        )
        .route("/{filename}", get(serve_static));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    info!(
        "Cyclonetix UI running at http://{}",
        listener.local_addr().unwrap().to_string()
    );
    axum::serve(listener, app).await.unwrap();
}

async fn schedule_commands(
    cli: &Cli,
    config: &CyclonetixConfig,
    state_manager: &Arc<dyn StateManager>,
) -> bool {
    if let Some(command) = &cli.command {
        return match command {
            Commands::ScheduleOutcome {
                task_id,
                context_overrides,
            } => {
                info!("Scheduling outcome: {}", task_id);
                let mut context = state_manager
                    .get_context(config.default_context.as_str())
                    .await
                    .unwrap_or_default();
                for (key, value) in context_overrides {
                    context.variables.insert(key.clone(), value.clone());
                }
                let outcome = Outcome {
                    run_id: Uuid::new_v4().to_string(),
                    task_id: task_id.clone(),
                    params: None,
                    context: Option::from(context),
                    scheduled_at: Utc::now(),
                };
                state_manager.schedule_outcome(&outcome).await;
                true
            }
            Commands::ScheduleDag {
                dag_id,
                context_overrides,
            } => {
                info!("Scheduling DAG: {}", dag_id);
                let mut context = state_manager
                    .get_context(config.default_context.as_str())
                    .await
                    .unwrap_or_default();
                for (key, value) in context_overrides {
                    context.variables.insert(key.clone(), value.clone());
                }
                let dag_definition = state_manager.get_dag(dag_id).await.unwrap();
                let scheduled_dag = ScheduledDag {
                    run_id: Uuid::new_v4().to_string(),
                    context,
                    dag: dag_definition,
                    scheduled_at: Utc::now(),
                };
                state_manager.schedule_dag(&scheduled_dag).await;
                true
            }
        };
    }
    false
}

/// Initializes tracing/logging based on environment variables.
fn init_tracing() {
    let enable_json =
        std::env::var("CYCLO_LOG_JSON").unwrap_or_else(|_| "false".to_string()) == "true";

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(true)
        .with_thread_ids(false)
        .with_env_filter(EnvFilter::from_default_env());

    if enable_json {
        subscriber.json().init();
    } else {
        subscriber.init();
    }
}

/// Serves static files (e.g. for the UI).
async fn serve_static(Path(filename): Path<String>) -> impl IntoResponse {
    // If no file is specified, serve index.html.
    let path = if filename.is_empty() || filename == "/" {
        "index.html".to_string()
    } else {
        filename
    };

    info!("Serving file: {}", path);

    match StaticAssets::get(&path) {
        Some(content) => {
            let mime_type = from_path(&path).first_or_octet_stream();
            Response::builder()
                .header("Content-Type", mime_type.to_string())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            error!("404 Not Found: {}", path);
            Response::builder()
                .status(404)
                .body(Body::from("404 - Not Found"))
                .unwrap()
        }
    }
}

/// Parses a key-value pair provided as `KEY=VALUE`
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("Invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}
