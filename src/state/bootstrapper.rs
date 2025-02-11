use crate::models::context::Context;
use crate::models::dag::DagDefinition;
use crate::models::parameters::ParameterSet;
use crate::models::task::Task;
use crate::state::state_manager::StateManager;
use crate::utils::config::CyclonetixConfig;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

pub struct Bootstrapper<S: StateManager + ?Sized + 'static> {
    state_manager: Arc<S>,
}

impl<S: StateManager + ?Sized + 'static> Bootstrapper<S> {
    pub fn new(state_manager: Arc<S>) -> Self {
        Bootstrapper { state_manager }
    }

    pub async fn bootstrap(&self, config: &CyclonetixConfig) {
        self.load::<Task, _>(&config.task_directory, "task", |s, v| {
            let s = Arc::clone(s);
            Box::pin(async move { s.store_task(&v).await })
        })
        .await;

        self.load::<Context, _>(&config.context_directory, "context", |s, v| {
            let s = Arc::clone(s);
            Box::pin(async move { s.store_context(&v).await })
        })
        .await;

        self.load::<ParameterSet, _>(&config.parameter_set_directory, "parameter_set", |s, v| {
            let s = Arc::clone(s);
            Box::pin(async move { s.store_parameter_set(&v).await })
        })
        .await;

        self.load::<DagDefinition, _>(&config.dag_directory, "dag", |s, v| {
            let s = Arc::clone(s);
            Box::pin(async move { s.store_dag_definition(&v).await })
        })
        .await;
    }

    async fn load<T, F>(&self, directory: &str, label: &str, store: F)
    where
        T: DeserializeOwned + Send + Sync + std::fmt::Debug,
        F: Fn(&Arc<S>, T) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
    {
        let dir = Path::new(directory);
        info!("Scanning {} directory: {}", label, dir.display());

        let all_items = Self::collect_from_directory::<T>(dir);
        for item in all_items {
            debug!("Loaded {}: {:?}", label, item);
            store(&self.state_manager, item).await;
        }
    }

    fn collect_from_directory<T: DeserializeOwned + std::fmt::Debug>(directory: &Path) -> Vec<T> {
        let mut items = Vec::new();
        for entry in fs::read_dir(directory).expect("Failed to read directory") {
            let path = entry.unwrap().path();
            if path.is_dir() {
                debug!("Entering subdirectory: {:?}", path);
                items.extend(Self::collect_from_directory::<T>(&path));
            } else if Self::is_yaml_file(&path) {
                debug!("Found YAML file: {:?}", path);
                match serde_yaml::from_reader::<_, T>(
                    fs::File::open(&path).expect("Failed to open file"),
                ) {
                    Ok(item) => items.push(item),
                    Err(_) => warn!("Skipping invalid YAML file: {:?}", path),
                }
            }
        }
        items
    }

    fn is_yaml_file(path: &Path) -> bool {
        path.extension().and_then(|s| s.to_str()) == Some("yaml")
    }
}
