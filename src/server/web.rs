use crate::server::dashboard::{DAGOverview, DashboardTemplate, QueueInfo, WorkerOverview};
use crate::utils::app_state::AppState;
use actix_web::{http::header, web, App, HttpResponse, HttpServer, Responder};
use askama::Template;
use mime_guess::from_path;
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::sync::Arc;
use futures_util::future::join_all;

/// Embedded static assets from the `static/` folder.
#[derive(RustEmbed, Clone)]
#[folder = "static/"]
pub struct StaticAssets;

/// Askama template for the index page.
/// Make sure you have a file at `templates/index.html`
#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate;



/// Starts the Actix‑web server.
pub async fn start_server(app_state: Arc<AppState>) -> std::io::Result<()> {
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .route("/", web::get().to(index))
            .route("/dashboard", web::get().to(dashboard))
            .route("/static/{filename:.*}", web::get().to(static_handler))
    })
    .bind("0.0.0.0:3000")?
    .run();

    server.await
}

/// Handler for the index page.
async fn index() -> impl Responder {
    // Askama’s Actix‑web integration will call .render() behind the scenes.
    IndexTemplate {}
}

/// Handler for the dashboard page.
async fn dashboard(app_state: web::Data<Arc<AppState>>) -> impl Responder {
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



/// Handler for serving embedded static assets.
async fn static_handler(path: web::Path<String>) -> impl Responder {
    // Extract the inner String from web::Path.
    let filename = path.into_inner();

    match StaticAssets::get(&filename) {
        Some(content) => {
            let mime_type = from_path(&filename).first_or_octet_stream();
            HttpResponse::Ok()
                .insert_header((header::CONTENT_TYPE, mime_type.to_string()))
                .body(match content.data {
                    Cow::Borrowed(b) => b.to_vec(),
                    Cow::Owned(b) => b,
                })
        }
        None => HttpResponse::NotFound().body("404 - Not Found"),
    }
}
