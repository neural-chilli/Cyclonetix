use crate::models::context::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub run_id: String,
    pub task_id: String,
    pub params: Option<HashMap<String, String>>,
    pub context: Option<Context>,
    pub scheduled_at: DateTime<Utc>,
}
