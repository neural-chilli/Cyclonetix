use crate::models::dag::GraphInstance;
use dashmap::DashMap;
use once_cell::sync::Lazy;

pub static GRAPH_CACHE: Lazy<DashMap<String, GraphInstance>> = Lazy::new(|| DashMap::new());
