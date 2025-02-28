use crate::utils::app_state::AppState;
use std::sync::{Arc, Mutex};
use tera::Tera;

/// Shared application state for all handlers
#[derive(Clone)]
pub struct AppStateWithTera {
    pub app_state: Arc<AppState>,
    pub tera: Arc<Mutex<Tera>>,
    pub dev_mode: bool,
}
