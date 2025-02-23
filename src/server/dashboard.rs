use crate::utils::app_state::AppState;
use crate::utils::constants::PENDING_STATUS;
use actix_web::{web, HttpResponse, Responder};
use futures_util::future::join_all;
use serde::Serialize;
use std::sync::Arc;
use tera::{Context, Tera};

/// A simplified view of an agent.
#[derive(Serialize)]
pub struct AgentOverview {
    pub agent_id: String,
    pub last_heartbeat: i64,
    pub tasks: Vec<String>, // List of assigned task IDs (or names)
    pub task_count: usize,  // Precomputed count of tasks
}

/// Information about a queue.
#[derive(Serialize)]
pub struct QueueInfo {
    pub name: String,
    pub task_count: usize,
}

/// A summary of a scheduled DAG.
#[derive(Serialize)]
pub struct DAGOverview {
    pub run_id: String,
    pub dag_id: String,
    pub dag_name: String,
    pub status: String,
    pub pending_tasks: usize,
}

/// Handler for the dashboard page.
pub async fn dashboard(
    app_state: web::Data<Arc<AppState>>,
    tera: web::Data<Tera>,
) -> impl Responder {
    let state_manager = &app_state.state_manager;

    // Get agents and convert to overview type.
    let agents_raw = state_manager.load_all_agents().await;
    let agents: Vec<AgentOverview> = agents_raw
        .into_iter()
        .map(|agent| AgentOverview {
            agent_id: agent.agent_id,
            last_heartbeat: agent.last_heartbeat,
            tasks: agent.tasks.clone(),
            task_count: agent.tasks.len(),
        })
        .collect();

    let agents_count = agents.len();

    // Fixed list of queues for demonstration.
    let work_queue_tasks = state_manager.get_queue_tasks("work_queue").await;
    let queues = vec![QueueInfo {
        name: "work_queue".to_string(),
        task_count: work_queue_tasks.len(),
    }];

    // Retrieve mutable DagInstances.
    let scheduled = state_manager.load_scheduled_dag_instances().await;
    // let scheduled_dags: Vec<DAGOverview> = join_all(scheduled.into_iter().map(|dag| async move {
    //     let status = state_manager
    //         .load_dag_status(&dag.run_id)
    //         .await
    //         .unwrap_or_else(|_| PENDING_STATUS.to_string());
    //     // Retrieve the DAG template to get a friendly name.
    //     let dag_template = state_manager.load_dag_template(&dag.dag_id).await;
    //     let dag_name = if let Some(template) = dag_template {
    //         template.name
    //     } else {
    //         dag.dag_id.clone()
    //     };
    //     DAGOverview {
    //         run_id: dag.run_id,
    //         dag_id: dag.dag_id,
    //         dag_name,
    //         status,
    //         pending_tasks: 0,
    //     }
    // }))
    // .await;

    let scheduled_dags_count = scheduled.len();
    let total_queue_tasks = work_queue_tasks.len();

    // Populate the Tera context.
    let mut context = Context::new();
    context.insert("agents", &agents);
    context.insert("queues", &queues);
    //context.insert("scheduled_dags", &scheduled_dags);
    context.insert("total_queue_tasks", &total_queue_tasks);
    context.insert("agent_count", &agents_count);
    context.insert("scheduled_dags_count", &scheduled_dags_count);

    // Render the dashboard template.
    match tera.render("dashboard.html", &context) {
        Ok(html) => HttpResponse::Ok().content_type("text/html").body(html),
        Err(err) => {
            eprintln!("Template rendering error: {}", err);
            HttpResponse::InternalServerError().body("Error rendering page")
        }
    }
}
