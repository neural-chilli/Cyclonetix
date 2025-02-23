use uuid::Uuid;

/// Creates a new run identifier.
pub fn generate_run_id() -> String {
    Uuid::new_v4().to_string()
}