use crate::models::context::Context;
use crate::models::task::{TaskTemplate, TaskInstance};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a manually defined DAG in Cyclonetix
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DAGTemplate {
    /// Unique identifier for the DAG
    pub id: String,

    /// Optional description of the DAG
    pub description: Option<String>,

    /// List of tasks in the DAG, with their dependencies
    pub tasks: Vec<TaskTemplate>,
}

/// Represents a DAG execution, whether predefined or dynamically created.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DAGInstance {
    pub run_id: String,           // Unique identifier for this DAG execution
    pub dag_id: String,           // Original DAG ID (if predefined) or generated
    pub context: Context,         // Execution context (e.g., env variables)
    pub tasks: Vec<TaskInstance>, // Tasks within this DAG
    pub scheduled_at: DateTime<Utc>,
}

impl DAGTemplate {
    /// Creates a new DAG definition
    pub fn new(id: &str, description: Option<String>, tasks: Vec<TaskTemplate>) -> Self {
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
                for dep in &task.dependencies {
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
