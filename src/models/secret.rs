use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Defines how a secret should be exposed to a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecretExposureType {
    /// Exposed as an environment variable
    EnvironmentVariable,
    /// Written to a temporary file, path provided as env var
    File,
    /// Provided in memory (for specific use cases only)
    InMemory,
}

/// A reference to a secret without the actual value
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretReference {
    /// Unique identifier for the secret
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Where the secret can be found (vault, aws, etc)
    pub provider: String,

    /// Path or key within the provider
    pub path: String,

    /// How this secret should be exposed to tasks
    pub exposure_type: SecretExposureType,

    /// Optional override name when exposing (e.g. env var name)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Last time this secret reference was updated
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}

/// Represents a SecretTemplate that can be stored and referenced
/// in tasks. This never contains the actual secret value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretTemplate {
    /// Unique identifier for the secret
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Optional description of what this secret is used for
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The backend provider for this secret
    pub provider: String,

    /// Path or identifier within the provider
    pub path: String,

    /// Key or field if the secret has multiple values
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,

    /// Default exposure method
    pub default_exposure: SecretExposureType,

    /// Tags for organizing secrets
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// When this secret was created
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,

    /// Last time this secret template was updated
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}

/// A runtime instance of a secret reference used in a specific DAG run
/// Never contains the actual secret value
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretInstance {
    /// Unique run ID for this instance
    pub run_id: String,

    /// ID of the secret template this references
    pub secret_id: String,

    /// ID of the DAG run this secret belongs to
    pub dag_run_id: String,

    /// How this secret will be exposed to tasks
    pub exposure_type: SecretExposureType,

    /// Name to use when exposing (env var name or file name)
    pub exposure_name: String,

    /// List of task IDs that need this secret
    #[serde(default)]
    pub required_by: Vec<String>,

    /// Creation timestamp for this instance
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
}

/// Result from resolving a secret, containing the actual value
/// Never persisted in state storage, only held in memory for task execution
#[derive(Debug, Clone)]
pub struct ResolvedSecret {
    /// Reference to the secret instance
    pub instance: SecretInstance,

    /// The actual secret value (sensitive!)
    pub value: String,
}

/// Collection of resolved secrets ready for task execution
#[derive(Debug, Clone, Default)]
pub struct TaskSecrets {
    /// Secrets to expose as environment variables
    pub environment_variables: HashMap<String, String>,

    /// Secrets to expose as files (path -> content)
    pub secret_files: HashMap<String, String>,

    /// Secrets to pass in memory (special cases only)
    pub in_memory_secrets: HashMap<String, String>,
}

impl TaskSecrets {
    /// Create a new empty collection
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a resolved secret to the appropriate collection
    pub fn add_secret(&mut self, secret: ResolvedSecret) {
        match secret.instance.exposure_type {
            SecretExposureType::EnvironmentVariable => {
                self.environment_variables
                    .insert(secret.instance.exposure_name.clone(), secret.value);
            }
            SecretExposureType::File => {
                self.secret_files
                    .insert(secret.instance.exposure_name.clone(), secret.value);
            }
            SecretExposureType::InMemory => {
                self.in_memory_secrets
                    .insert(secret.instance.exposure_name.clone(), secret.value);
            }
        }
    }

    /// Clear all secrets from memory
    pub fn clear(&mut self) {
        self.environment_variables.clear();
        self.secret_files.clear();
        self.in_memory_secrets.clear();
    }
}
