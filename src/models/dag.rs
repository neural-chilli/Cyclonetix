use crate::models::context::Context;
use crate::models::task::TaskTemplate;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a manually defined DAG template.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DagTemplate {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tasks: Vec<TaskTemplate>,
    pub tags: Option<Vec<String>>,
}

/// Represents the mutable execution state of a DAG.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DagInstance {
    pub run_id: String,
    pub dag_id: String,
    pub context: Context,
    pub task_count: usize, // total number of tasks scheduled
    pub completed_tasks: usize,
    pub status: String,
    pub last_updated: DateTime<Utc>,
    pub tags: Option<Vec<String>>,
}

/// Represents the immutable dependency graph for a DAG instance.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphInstance {
    pub run_id: String, // Should be identical to the DagInstance run_id.
    pub dag_id: String,
    pub graph: crate::graph::graph_manager::ExecutionGraph,
}
