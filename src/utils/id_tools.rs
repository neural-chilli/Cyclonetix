use uuid::Uuid;

/// Creates a new run identifier.
pub fn generate_run_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn generate_compound_run_id(prefix: &str) -> String {
    format!("{}::{}", prefix, Uuid::new_v4())
}

pub fn strip_guid(string: &str) -> String {
    string.split("::").next().unwrap().to_string()
}
