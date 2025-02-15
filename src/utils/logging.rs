use tracing::Level;
use tracing_subscriber::EnvFilter;

/// Initializes tracing/logging based on environment variables.
pub fn init_tracing() {
    let enable_json =
        std::env::var("CYCLO_LOG_JSON").unwrap_or_else(|_| "false".to_string()) == "true";

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(true)
        .with_thread_ids(false)
        .with_env_filter(EnvFilter::from_default_env());

    if enable_json {
        subscriber.json().init();
    } else {
        subscriber.init();
    }
}
