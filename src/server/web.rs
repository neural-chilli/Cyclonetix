use crate::utils::app_state::AppState;
use actix_web::{http::header, web, App, HttpResponse, HttpServer, Responder};
use mime_guess::from_path;
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::sync::Arc;
use tera::Tera;
use crate::server::dashboard::dashboard;

/// Embedded static assets from the `static/` folder.
#[derive(RustEmbed, Clone)]
#[folder = "static/"]
pub struct StaticAssets;

/// Starts the Actix-web server.
pub async fn start_server(app_state: Arc<AppState>) -> std::io::Result<()> {
    // Initialize Tera templates
    let tera = Tera::new("templates/**/*.html").expect("Failed to load templates");

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .app_data(web::Data::new(tera.clone())) // Share Tera instance
            .route("/", web::get().to(dashboard)) // Dashboard handler
            .route("/static/{filename:.*}", web::get().to(static_handler)) // Static files
    })
        .bind("0.0.0.0:3000")?
        .run();

    server.await
}

/// Handler for serving embedded static assets.
async fn static_handler(path: web::Path<String>) -> impl Responder {
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
