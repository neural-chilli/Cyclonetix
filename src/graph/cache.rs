use dashmap::DashMap;
use once_cell::sync::Lazy;
use crate::models::dag::GraphInstance;

pub static GRAPH_CACHE: Lazy<DashMap<String, GraphInstance>> = Lazy::new(|| {
    DashMap::new()
});
