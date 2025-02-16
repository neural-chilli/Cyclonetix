use askama::Template;

/// The DashboardTemplate now holds all precomputed values so that Askama doesn’t have to call filters on raw vectors.
#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub workers: Vec<WorkerOverview>,
    pub queues: Vec<QueueInfo>,
    pub scheduled_dags: Vec<DAGOverview>,
    pub total_queue_tasks: usize,
    pub workers_count: usize,
    pub scheduled_dags_count: usize,
}

/// A simplified view of a worker.
pub struct WorkerOverview {
    pub worker_id: String,
    pub last_heartbeat: i64,
    pub tasks: Vec<String>,  // The actual list of assigned tasks
    pub task_count: usize,   // Precomputed count of tasks
}

/// Information about a queue.
pub struct QueueInfo {
    pub name: String,
    pub task_count: usize,
}

/// A summary of a scheduled DAG.
pub struct DAGOverview {
    pub run_id: String,
    pub dag_id: String,
    pub dag_name: String,
    pub status: String,
    pub pending_tasks: usize,
}
