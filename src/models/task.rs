use crate::utils::cli::TaskInstanceFields;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::debug;
use tracing::log::warn;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

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

impl TaskTemplate {
    pub fn from_yaml(path: &std::path::Path) -> Option<Self> {
        debug!("Trying to load YAML: {:?}", path);
        let content = std::fs::read_to_string(path).ok()?;
        let task: Result<Self, _> = serde_yaml::from_str(&content);

        match task {
            Ok(t) => {
                debug!("Successfully loaded task: {:?}", t);
                Some(t)
            }
            Err(e) => {
                warn!("YAML Parsing Error in {:?}: {:?}", path, e);
                None
            }
        }
    }
}

/// Represents a single task instance within a DAG execution.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskInstance {
    pub run_id: String,
    pub task_id: String,
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub parameters: HashMap<String, String>,
    pub dependencies: Vec<String>,
    pub queue: String,
    pub evaluation_point: bool,
}

impl TaskInstanceFields for TaskTemplate {
    fn id(&self) -> &String {
        &self.id
    }
    fn name(&self) -> &String {
        &self.name
    }
    fn description(&self) -> &Option<String> {
        &self.description
    }
    fn queue(&self) -> &Option<String> {
        &self.queue
    }
    fn dependencies(&self) -> &Vec<String> {
        &self.dependencies
    }
    fn command(&self) -> &String {
        &self.command
    }

    fn evaluation_point(&self) -> &bool {
        &self.evaluation_point
    }
}
