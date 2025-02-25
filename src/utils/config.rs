use crate::utils::constants::{DEFAULT_CLUSTER_ID, DEFAULT_QUEUE};
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

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum SerializationFormat {
    #[default]
    Json,
    Binary,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CyclonetixConfig {
    pub task_directory: String,
    pub context_directory: String,
    pub dag_directory: String,
    pub backend: String,
    pub backend_url: String,
    pub queues: Vec<String>,
    pub cluster_id: String,
    pub serialization_format: SerializationFormat,
    pub security: SecurityConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct SecurityConfig {
    pub enabled: bool,
    pub oauth: OAuthConfig,
    pub cookie_secret: String,
    pub session_timeout_minutes: u64,
    // Paths that don't require authentication even when security is enabled
    pub public_paths: Vec<String>,
    // Paths that require API key authentication (rather than OAuth)
    pub api_paths: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct OAuthConfig {
    pub provider: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub allowed_domains: Vec<String>,
    pub allowed_emails: Vec<String>,
}

impl Default for CyclonetixConfig {
    fn default() -> Self {
        CyclonetixConfig {
            task_directory: "tasks/".to_string(),
            context_directory: "contexts/".to_string(),
            dag_directory: "dags/".to_string(),
            backend: "redis".to_string(),
            backend_url: "redis://127.0.0.1:6379".to_string(),
            queues: vec![DEFAULT_QUEUE.to_string()],
            cluster_id: DEFAULT_CLUSTER_ID.to_string(),
            serialization_format: SerializationFormat::Json,
            security: SecurityConfig::default(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        SecurityConfig {
            enabled: false,
            oauth: OAuthConfig::default(),
            cookie_secret: "change-me-in-production".to_string(),
            session_timeout_minutes: 120, // 2 hours
            public_paths: vec![
                "/static/*".to_string(),
                "/login".to_string(),
                "/auth/*".to_string(),
                "/health".to_string(),
            ],
            api_paths: vec![
                "/api/*".to_string(),
            ],
        }
    }
}

impl Default for OAuthConfig {
    fn default() -> Self {
        OAuthConfig {
            provider: "google".to_string(),
            client_id: "".to_string(),
            client_secret: "".to_string(),
            redirect_url: "http://localhost:3000/auth/callback".to_string(),
            allowed_domains: vec![],
            allowed_emails: vec![],
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