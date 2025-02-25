use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::models::task::TaskInstance;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Context {
    pub id: String,
    
    pub variables: HashMap<String, String>,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    pub fn new() -> Self {
        Self {
            id: "".to_string(),
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
        task_instance: TaskInstance,
    ) -> HashMap<String, String> {
        let mut env = HashMap::new();

        for (key, value) in &self.variables {
            task_instance.parameters.iter().for_each(|(_param_key, _)| {
                    env.insert(key.clone(), value.clone());
            });
            self.variables.iter().for_each(|(key, value)| {
                env.insert(key.clone(), value.clone());
            });
        }
        env
    }
}
