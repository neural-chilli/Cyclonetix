use hostname::get;

pub fn get_hostname() -> String {
    match get() {
        Ok(name) => name.to_string_lossy().to_string(),
        _ => "unknown".to_string(),
    }
}
