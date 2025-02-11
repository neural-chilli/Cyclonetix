use std::sync::Arc;
use chrono::Utc;
use clap::{Parser, Subcommand};
use tracing::info;
use uuid::Uuid;
use crate::models::dag::ScheduledDag;
use crate::models::outcome::Outcome;
use crate::state::state_manager::StateManager;
use crate::utils::config::CyclonetixConfig;
use crate::utils::parsing::parse_key_val;

/// CLI arguments and subcommands.
#[derive(Parser)]
#[command(
    name = "Cyclonetix",
    about = "A lightweight Rust-based workflow orchestrator"
)]
pub struct Cli {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.yaml")]
    pub config_file: String,

    /// Name of the queue for the worker to listen on
    #[arg(short, long, default_value = "work_queue")]
    pub queue: String,

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

/// Handles scheduling commands from the CLI.
pub async fn schedule_commands(
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
                    context: Some(context),
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
