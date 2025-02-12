use crate::models::task::Task;
use crate::state::state_manager::StateManager;
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::prelude::EdgeRef;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

pub struct ExecutionGraph {
    pub graph: DiGraph<String, ()>,           // Directed graph of task IDs
    pub node_map: HashMap<String, NodeIndex>, // Maps Task ID to graph node
    pub tasks: HashMap<String, Task>,
}

impl ExecutionGraph {
    /// Checks if all dependencies for a given task are completed.
    pub async fn is_task_ready<S: StateManager + ?Sized>(
        &self,
        task_id: &str,
        state_manager: &Arc<S>,
        context_id: &str,
    ) -> bool {
        if let Some(&task_node) = self.node_map.get(task_id) {
            for dep_node in self
                .graph
                .neighbors_directed(task_node, petgraph::Direction::Incoming)
            {
                let dep_id = &self.graph[dep_node];
                let dep_status = state_manager
                    .get_task_status(dep_id, context_id)
                    .await
                    .unwrap_or_else(|| "pending".to_string());
                if dep_status != "completed" {
                    return false;
                }
            }
        }
        true
    }

    pub fn from_tasks(tasks: Vec<Task>) -> Self {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();
        let mut task_map = HashMap::new();

        // Insert nodes (tasks)
        for task in &tasks {
            let node = graph.add_node(task.id.clone()); // Now `String` works
            node_map.insert(task.id.clone(), node);
            task_map.insert(task.id.clone(), task.clone());
        }

        // Insert edges (dependencies)
        for task in &tasks {
            if let Some(&task_node) = node_map.get(&task.id) {
                for dep in &task.dependencies {
                    if let Some(&dep_node) = node_map.get(dep) {
                        graph.add_edge(dep_node, task_node, ());
                    }
                }
            }
        }

        ExecutionGraph {
            graph,
            node_map,
            tasks: task_map,
        }
    }

    /// Perform topological sorting to determine execution order
    pub fn get_execution_order(&self) -> Vec<String> {
        match toposort(&self.graph, None) {
            Ok(sorted_nodes) => {
                let sorted_tasks: Vec<String> = sorted_nodes
                    .iter()
                    .map(|node| self.graph[*node].clone()) // Convert NodeIndex back to task ID
                    .collect();

                debug!(
                    "Topological Sorting Complete. Execution Order: {:?}",
                    sorted_tasks
                );
                sorted_tasks
            }
            Err(_) => {
                panic!("Dependency cycle detected in task graph! Execution cannot proceed.");
            }
        }
    }

    pub async fn get_executable_tasks<S: StateManager + ?Sized>(
        &self,
        state_manager: &Arc<S>,
        context_id: &str,
    ) -> Vec<String> {
        let mut executable = Vec::new();

        for node in self.graph.node_indices() {
            let task_id = &self.graph[node];
            let status = state_manager
                .get_task_status(task_id, context_id)
                .await
                .unwrap_or_else(|| "pending".to_string());

            if status == "completed" {
                continue;
            }

            if self.is_task_ready(task_id, state_manager, context_id).await {
                executable.push(task_id.clone());
            }
        }
        executable
    }

    pub fn print_graph(&self) {
        for node in self.graph.node_indices() {
            let task_id = &self.graph[node];
            let deps: Vec<_> = self
                .graph
                .neighbors_directed(node, petgraph::Direction::Incoming)
                .map(|n| self.graph[n].clone())
                .collect();
            debug!("Task: {} depends on {:?}", task_id, deps);
        }
    }

    pub fn get_dependencies(&self, task_id: &str) -> Vec<String> {
        let mut deps = Vec::new();
        if let Some(task_node) = self.node_map.get(task_id) {
            for dep_node in self
                .graph
                .neighbors_directed(*task_node, petgraph::Direction::Incoming)
            {
                let dep_task_id = &self.graph[dep_node];
                deps.push(dep_task_id.clone());
            }
        }
        deps
    }

    pub fn get_all_tasks(&self) -> Vec<String> {
        self.graph
            .node_indices()
            .map(|node| self.graph[node].clone())
            .collect()
    }
}
