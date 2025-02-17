use crate::models::context::Context;
use crate::models::dag::{DAGInstance, DAGTemplate};
use crate::models::task::TaskInstance;
use crate::orchestrator::orchestrator::build_dag_from_task;
use crate::state::state_manager::StateManager;
use crate::utils::constants::DEFAULT_QUEUE;
use chrono::Utc;
use clap::{ArgAction, Parser, Subcommand};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

/// CLI arguments and subcommands.
#[derive(Parser)]
#[command(
    name = "Cyclonetix",
    about = "A lightweight Rust-based workflow orchestrator"
)]
pub struct Cli {
    /// Path to the configuration file (if none provided, uses `config.yaml`)
    #[arg(short, long, default_value = "config.yaml")]
    pub config_file: String,

    /// Start the UI server
    #[arg(long, action = ArgAction::SetTrue)]
    pub ui: bool,

    /// Start a agent process
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

/// Creates a new run identifier.
fn generate_run_id() -> String {
    Uuid::new_v4().to_string()
}

/// Converts a list of task definitions into task instances.
/// The mapping logic is identical for both the predefined DAG and DAG file flows.
fn map_tasks<T: TaskInstanceFields>(tasks: &[T]) -> Vec<TaskInstance> {
    tasks
        .iter()
        .map(|t| TaskInstance {
            run_id: format!("{}_{}", t.id(), generate_run_id()),
            task_id: t.id().clone(),
            name: t.name().clone(),
            description: Some(t.description().clone().expect("REASON")),
            parameters: HashMap::new(),
            queue: t
                .queue()
                .clone()
                .unwrap_or_else(|| DEFAULT_QUEUE.to_string()),
            dependencies: t.dependencies().clone(),
            command: t.command().clone(),
            evaluation_point: t.evaluation_point().clone(),
        })
        .collect()
}

/// Creates a DAG execution from a given DAG definition (predefined or from file).
fn create_dag_execution<T: TaskInstanceFields>(dag_id: &str, tasks: &[T]) -> DAGInstance {
    let run_id = generate_run_id();
    DAGInstance {
        run_id: run_id.clone(),
        dag_id: dag_id.to_string(),
        context: Context {
            id: run_id.clone(),
            variables: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        tasks: map_tasks(tasks),
        scheduled_at: Utc::now(),
        tags: None,
    }
}

/// Trait to abstract over task definition fields shared by different DAG definitions.
pub trait TaskInstanceFields {
    fn id(&self) -> &String;
    fn name(&self) -> &String;
    fn description(&self) -> &Option<String>;
    fn queue(&self) -> &Option<String>;
    fn dependencies(&self) -> &Vec<String>;
    fn command(&self) -> &String;
    fn evaluation_point(&self) -> &bool;
}

// Implement the trait for the types used in your DAG definitions.
// For example, if your DAGTemplate::tasks has these fields, implement it accordingly.
// impl TaskInstanceFields for YourTaskDefinition { ... }

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

        // Ensure required directories exist
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
            let dag_definition = match state_manager.get_dag(&dag_id).await {
                Some(dag) => dag,
                None => {
                    error!("DAG ID {} not found.", dag_id);
                    return;
                }
            };

            let dag_execution = create_dag_execution(&dag_definition.id, &dag_definition.tasks);
            state_manager.store_dag_execution(&dag_execution).await;
            info!("Scheduled DAG execution: {}", dag_execution.run_id);
        }

        Commands::ScheduleTask { task_id } => {
            info!("Scheduling Task: {}", task_id);
            let dag_execution = build_dag_from_task(state_manager.clone(), &task_id, None).await;
            state_manager.store_dag_execution(&dag_execution).await;
            info!(
                "Scheduled DAG execution from task: {}",
                dag_execution.run_id
            );
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

            let dag_definition: DAGTemplate = match serde_yaml::from_reader(file) {
                Ok(dag) => dag,
                Err(err) => {
                    error!("Invalid DAG file format: {:?}", err);
                    return;
                }
            };

            let dag_execution = create_dag_execution(&dag_definition.id, &dag_definition.tasks);
            state_manager.store_dag_execution(&dag_execution).await;
            info!(
                "Scheduled DAG execution from file: {}",
                dag_execution.run_id
            );
        }
    }
}
