use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::log::warn;
use tracing::{debug, info};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub command: String,
    pub dependencies: Vec<String>,
    pub parameters: Value, // Uses `Value` to handle flexible JSON-like parameters
}

impl Task {
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
