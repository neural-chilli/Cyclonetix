use askama::Template;
use actix_web::{web, HttpResponse, Responder};
use std::sync::Arc;
use futures_util::future::join_all;
use crate::utils::app_state::AppState;

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

/// Handler for the dashboard page.
pub async fn dashboard(app_state: web::Data<Arc<AppState>>) -> impl Responder {
    let state_manager = &app_state.state_manager;

    // Get workers and convert to overview type.
    let workers_raw = state_manager.get_all_workers().await;
    let workers: Vec<WorkerOverview> = workers_raw.into_iter().map(|w| WorkerOverview {
        worker_id: w.worker_id,
        last_heartbeat: w.last_heartbeat,
        tasks: w.tasks.clone(),
        task_count: w.tasks.len(),
    }).collect();

    let workers_count = workers.len();

    // Fixed list of queues for demonstration.
    let queues = vec![
        QueueInfo {
            name: "work_queue".to_string(),
            task_count: state_manager.get_queue_tasks("work_queue").await.len(),
        },
    ];

    // Retrieve scheduled DAGs and convert to overview type.
    let scheduled = state_manager.get_scheduled_dags().await;
    let scheduled_dags: Vec<DAGOverview> = join_all(scheduled.into_iter().map(|dag| async move {
        let statuses = join_all(dag.tasks.iter().map(|t| state_manager.get_task_status(&t.run_id))).await;
        let pending = statuses.iter().filter(|s| s.as_ref().map_or(true, |v| v != "completed")).count();
        let status = state_manager.get_dag_status(&dag.run_id).await.unwrap_or_else(|_| "pending".to_string());
        let dag_template = state_manager.get_dag(&dag.dag_id).await;
        let dag_name = if let Some(template) = dag_template {
            template.name
        } else {
            dag.dag_id.clone()
        };
        DAGOverview {
            run_id: dag.run_id.clone(),
            dag_id: dag.dag_id.clone(),
            dag_name,
            status,
            pending_tasks: pending,
        }
    })).await;
    let scheduled_dags_count = scheduled_dags.len();
    let total_queue_tasks = state_manager.get_queue_tasks("work_queue").await.len();

    let dashboard = DashboardTemplate {
        workers,
        queues,
        scheduled_dags,
        total_queue_tasks,
        workers_count,
        scheduled_dags_count,
    };

    HttpResponse::Ok().body(dashboard.render().unwrap())
}