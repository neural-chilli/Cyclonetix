use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Represents a task definition template. This is immutable.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TaskTemplate {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub parameters: Value,
    pub dependencies: Vec<String>,
    pub queue: Option<String>,
    #[serde(default)]
    pub evaluation_point: bool,
}

/// Represents a task execution instance with UI-friendly metadata.
/// This is used as part of the mutable execution state.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TaskInstance {
    pub run_id: String,
    pub task_id: String,
    pub dag_id: String,
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub parameters: HashMap<String, String>,
    pub queue: String,
    pub status: String,
    pub last_updated: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    #[serde(default)]
    pub evaluation_point: bool,
}
