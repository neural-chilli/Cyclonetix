use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParameterSet {
    pub id: String,
    pub variables: HashMap<String, String>,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}

impl ParameterSet {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            variables: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.variables.insert(key.to_string(), value.to_string());
        self.updated_at = Utc::now();
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.variables.get(key)
    }

    /// Retrieves all environment variables for a specific task instance
    pub fn get_task_env(
        &self,
        task_id: &str,
        param_set: Option<&str>,
        is_global_task: bool,
    ) -> HashMap<String, String> {
        let param_set = param_set.unwrap_or("default"); // Use "default" if no param_set
        let mut env = HashMap::new();
        let instance_key = format!("{}|{}", task_id, param_set);

        for (key, value) in &self.variables {
            if key.starts_with('~') {
                // Expose global variables to all tasks
                let clean_key = key.trim_start_matches('~').to_string();
                env.insert(clean_key, value.clone());
            } else if key.starts_with(&instance_key) {
                // Expose instance-specific variables
                let clean_key = key.split_once(':').unwrap().1.to_string();
                env.insert(clean_key, value.clone());
            } else if !is_global_task {
                // Expose non-prefixed variables to non-global tasks
                env.insert(key.clone(), value.clone());
            }
        }

        env
    }
}
