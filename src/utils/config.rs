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
#[serde(default)] // Use default values for any missing fields.
pub struct CyclonetixConfig {
    pub task_directory: String,
    pub context_directory: String,
    pub parameter_set_directory: String,
    pub dag_directory: String,
    pub backend: String,
    pub backend_url: String,
    pub default_context: String,
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
        }
    }
}

impl CyclonetixConfig {
    pub fn load(config_path: &str) -> Self {
        // Read the file content; you might want to handle errors more gracefully.
        let content = fs::read_to_string(config_path).unwrap_or_else(|_| {
            eprintln!(
                "Warning: Failed to read config file at {}. Using default configuration.",
                config_path
            );
            String::new()
        });

        // If content is empty, deserialize will use the default values.
        serde_yaml::from_str(&content).unwrap_or_default()
    }
}
