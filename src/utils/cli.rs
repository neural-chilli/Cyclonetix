use crate::models::dag::{DagTemplate};
use crate::graph::orchestrator::{schedule_dag, schedule_dag_from_task};
use crate::state::state_manager::StateManager;
use clap::{ArgAction, Parser, Subcommand};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::{error, info, warn};

/// CLI arguments and subcommands.
#[derive(Parser)]
#[command(
    name = "Cyclonetix",
    about = "A lightweight Rust-based workflow orchestrator"
)]
pub struct Cli {
    /// Path to the configuration file (if none provided, uses `config.yaml`)
    #[arg(short, long, default_value = "config.yaml")]
    pub config: String,

    /// Start the UI server
    #[arg(long, action = ArgAction::SetTrue)]
    pub ui: bool,

    /// Start an agent process
    #[arg(long, action = ArgAction::SetTrue)]
    pub agent: bool,

    /// Start an orchestrator process
    #[arg(long, action = ArgAction::SetTrue)]
    pub orchestrator: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Supported scheduling commands.
#[derive(Subcommand)]
pub enum Commands {
    /// Schedule a DAG by ID
    ScheduleDag {
        /// DAG ID to schedule
        dag_id: String,
    },

    /// Schedule a single task (automatically resolves dependencies)
    ScheduleTask {
        /// Task ID to schedule
        task_id: String,
    },

    /// Schedule a DAG from a user-provided file
    ScheduleDagFile {
        /// Path to a DAG file (YAML)
        file_path: String,
    },
}

pub fn ensure_config_exists(config_path: &str) {
    let config_file = Path::new(config_path);
    if !config_file.exists() {
        warn!(
            "Config file {} not found, creating default config.",
            config_path
        );
        let default_config = r#"
task_directory: "./data/tasks"
context_directory: "./data/contexts"
parameter_set_directory: "./data/parameters"
dag_directory: "./data/dags"
"#;
        if let Err(e) = fs::write(config_path, default_config) {
            error!("Failed to create default config file: {}", e);
            return;
        }
        for dir in &[
            "./data/tasks",
            "./data/contexts",
            "./data/parameters",
            "./data/dags",
        ] {
            if let Err(e) = fs::create_dir_all(dir) {
                error!("Failed to create directory {}: {}", dir, e);
            }
        }
        info!("Default config and directories created.");
    }
}

pub async fn handle_scheduling(command: Commands, state_manager: Arc<dyn StateManager>) {
    match command {
        Commands::ScheduleDag { dag_id } => {
            info!("Scheduling predefined DAG: {}", dag_id);
            let dag_definition = match state_manager.load_dag_template(&dag_id).await {
                Some(dag) => dag,
                None => {
                    error!("DAG ID {} not found.", dag_id);
                    return;
                }
            };
            schedule_dag(state_manager.clone(), dag_definition, None).await;
        }
        Commands::ScheduleTask { task_id } => {
            info!("Scheduling Task: {}", task_id);
            schedule_dag_from_task(state_manager.clone(), &task_id, None).await;
        }
        Commands::ScheduleDagFile { file_path } => {
            info!("Scheduling DAG from file: {}", file_path);
            let file = match fs::File::open(&file_path) {
                Ok(f) => f,
                Err(e) => {
                    error!("Failed to open DAG file {}: {}", file_path, e);
                    return;
                }
            };
            let dag_definition: DagTemplate = match serde_yaml::from_reader(file) {
                Ok(dag) => dag,
                Err(err) => {
                    error!("Invalid DAG file format: {:?}", err);
                    return;
                }
            };
            schedule_dag(state_manager.clone(), dag_definition, None).await;
        }
    }
}
