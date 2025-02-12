use actix_web::{web, App, HttpResponse, HttpServer, Responder, http::header};
use askama::Template;
use mime_guess::from_path;
use rust_embed::RustEmbed;
use std::borrow::Cow;

/// Embedded static assets from the `static/` folder.
#[derive(RustEmbed, Clone)]
#[folder = "static/"]
pub struct StaticAssets;

/// Askama template for the index page.
/// Make sure you have a file at `templates/index.html`
#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate;

/// Askama template for the dashboard page.
/// Make sure you have a file at `templates/dashboard.html`
#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate;

/// Starts the Actix‑web server.
pub async fn start_server() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(index))
            .route("/dashboard", web::get().to(dashboard))
            .route("/static/{filename:.*}", web::get().to(static_handler))
    })
        .bind("0.0.0.0:3000")?
        .run()
        .await
}

/// Handler for the index page.
async fn index() -> impl Responder {
    // Askama’s Actix‑web integration will call .render() behind the scenes.
    IndexTemplate {}
}

/// Handler for the dashboard page.
async fn dashboard() -> impl Responder {
    DashboardTemplate {}
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

