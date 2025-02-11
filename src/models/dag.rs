use crate::models::context::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a scheduled DAG in Cyclonetix
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ScheduledDag {
    pub run_id: String,
    pub dag: DagDefinition,
    pub context: Context,
    pub scheduled_at: DateTime<Utc>,
}

/// Represents a manually defined DAG in Cyclonetix
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DagDefinition {
    /// Unique identifier for the DAG
    pub id: String,

    /// Optional description of the DAG
    pub description: Option<String>,

    /// List of tasks in the DAG, with their dependencies
    pub tasks: Vec<DagTask>,
}

/// Represents a single task inside a manually defined DAG
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DagTask {
    /// Unique task identifier
    pub id: String,

    /// List of task IDs that this task depends on
    pub depends_on: Vec<String>,

    /// Optional environment variables (overrides global context)
    pub env: Option<HashMap<String, String>>,

    /// Command to execute (e.g., "python train.py")
    pub command: String,
}

impl DagDefinition {
    /// Creates a new DAG definition
    pub fn new(id: &str, description: Option<String>, tasks: Vec<DagTask>) -> Self {
        Self {
            id: id.to_string(),
            description,
            tasks,
        }
    }

    /// Checks if the DAG is valid (e.g., no circular dependencies)
    pub fn validate(&self) -> Result<(), String> {
        let mut visited = HashMap::new();
        for task in &self.tasks {
            if self.has_cycle(task.id.clone(), &mut visited) {
                return Err(format!("Cycle detected in DAG: {}", self.id));
            }
        }
        Ok(())
    }

    /// Recursively checks for circular dependencies
    fn has_cycle(&self, task_id: String, visited: &mut HashMap<String, bool>) -> bool {
        if let Some(&true) = visited.get(&task_id) {
            return true; // Cycle detected
        }
        visited.insert(task_id.clone(), true);

        for task in &self.tasks {
            if task.id == task_id {
                for dep in &task.depends_on {
                    if self.has_cycle(dep.clone(), visited) {
                        return true;
                    }
                }
            }
        }
        visited.insert(task_id, false);
        false
    }
}

impl DagTask {
    /// Creates a new DAG task
    pub fn new(
        id: &str,
        depends_on: Vec<String>,
        command: &str,
        env: Option<HashMap<String, String>>,
    ) -> Self {
        Self {
            id: id.to_string(),
            depends_on,
            env,
            command: command.to_string(),
        }
    }
}
