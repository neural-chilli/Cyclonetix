use crate::state::state_manager::StateManager;
use std::sync::Arc;

/// Represents the shared application state.
#[derive(Clone)]
pub struct AppState {
    pub state_manager: Arc<dyn StateManager + Send + Sync>,
}
