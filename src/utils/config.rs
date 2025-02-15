use clap::Parser;
use serde::Deserialize;
use std::fs;

#[derive(Parser)]
#[command(name = "cyclonetix")]
#[command(version = "0.1")]
#[command(about = "A high-performance orchestration framework")]
pub struct Config {
    #[arg(long, default_value = "config.yaml")]
    pub config_file: String,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CyclonetixConfig {
    pub task_directory: String,
    pub context_directory: String,
    pub parameter_set_directory: String,
    pub dag_directory: String,
    pub backend: String,
    pub backend_url: String,
    pub default_context: String,
    pub queues: Vec<String>,
}

impl Default for CyclonetixConfig {
    fn default() -> Self {
        CyclonetixConfig {
            task_directory: "tasks/".to_string(),
            context_directory: "contexts/".to_string(),
            parameter_set_directory: "params/".to_string(),
            dag_directory: "dags/".to_string(),
            backend: "redis".to_string(),
            backend_url: "redis://127.0.0.1:6379".to_string(),
            default_context: "default_ctx".to_string(),
            queues: vec!["default".to_string()],
        }
    }
}

impl CyclonetixConfig {
    pub fn load(config_path: &str) -> Self {
        // Attempt to read the config file.
        let content = fs::read_to_string(config_path);
        let content = match content {
            Ok(text) if !text.trim().is_empty() => text,
            _ => {
                tracing::warn!(
                    "Config file at '{}' not found or empty. Using default configuration.",
                    config_path
                );
                return Self::default();
            }
        };

        // Attempt to parse the config, falling back to default if parsing fails.
        serde_yaml::from_str(&content).unwrap_or_else(|err| {
            tracing::warn!(
                "Failed to parse config file at '{}': {}. Using default configuration.",
                config_path,
                err
            );
            Self::default()
        })
    }
}
