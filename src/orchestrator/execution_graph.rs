use crate::models::dag::DAGInstance;
use crate::models::task::{TaskTemplate, TaskInstance};
use crate::state::state_manager::StateManager;
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
pub use petgraph::prelude::EdgeRef;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

pub struct ExecutionGraph {
    pub graph: DiGraph<String, ()>,           // Directed graph of task IDs
    pub node_map: HashMap<String, NodeIndex>, // Maps Task Run ID to graph node
}

impl ExecutionGraph {
    /// Creates an execution graph from a DAGExecution (instead of a plain task list)
    pub fn from_tasks(tasks: Vec<TaskTemplate>) -> Self {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();
        let mut task_map: HashMap<String, TaskTemplate> = HashMap::new();

        // Insert nodes (tasks)
        for task in &tasks {
            let node = graph.add_node(task.id.clone());
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

        ExecutionGraph { graph, node_map }
    }

    /// Creates an execution graph from a DAGExecution
    pub fn from_dag_execution(dag_execution: &DAGInstance) -> Self {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();

        // Insert nodes (task instances)
        for task_instance in &dag_execution.tasks {
            let task_run_id = task_instance.run_id.clone();
            let node = graph.add_node(task_run_id.clone());
            node_map.insert(task_run_id, node);
        }

        // Insert edges (dependencies)
        for task_instance in &dag_execution.tasks {
            let task_run_id = &task_instance.run_id;
            if let Some(&task_node) = node_map.get(task_run_id) {
                for dep_run_id in &task_instance.dependencies {
                    if let Some(&dep_node) = node_map.get(dep_run_id) {
                        graph.add_edge(dep_node, task_node, ());
                        tracing::debug!("Added edge: {} -> {} in DAG {}", dep_run_id, task_run_id, dag_execution.run_id);
                    }
                }
            }
        }

        ExecutionGraph { graph, node_map }
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

    /// Returns executable tasks for a given DAG execution
    pub async fn get_executable_tasks<S: StateManager + ?Sized>(
        &self,
        state_manager: &Arc<S>,
        dag_execution: &DAGInstance,
    ) -> Vec<String> {
        let mut executable = Vec::new();

        for node in self.graph.node_indices() {
            let task_run_id = &self.graph[node]; // Now using task_run_id directly!

            // Retrieve task instance from DAG execution
            let task_instance = dag_execution.tasks.iter().find(|t| t.run_id == *task_run_id);
            if task_instance.is_none() {
                tracing::warn!("TaskInstance {} not found in DAG Execution {}", task_run_id, dag_execution.run_id);
                continue;
            }
            let task_instance = task_instance.unwrap();

            // Retrieve the task status
            let status = state_manager
                .get_task_status(&task_instance.run_id)
                .await
                .unwrap_or("pending".to_string());

            if status == "completed" {
                continue; // Already done
            }

            // Check if dependencies are met
            let ready = self.are_dependencies_completed(state_manager, task_instance).await;

            if ready {
                executable.push(task_instance.run_id.clone());
            } else {
                debug!(
                "Task {} is waiting for dependencies in DAG {}",
                task_instance.run_id, dag_execution.run_id
            );
            }
        }
        executable
    }


    /// Checks if all dependencies for a given task are completed.
    pub async fn is_task_ready<S: StateManager + ?Sized>(
        &self,
        task_id: &str,
        state_manager: &Arc<S>,
        run_id: &str,
    ) -> bool {
        if let Some(&task_node) = self.node_map.get(task_id) {
            for dep_node in self
                .graph
                .neighbors_directed(task_node, petgraph::Direction::Incoming)
            {
                let _ = &self.graph[dep_node];
                let dep_status = state_manager
                    .get_task_status(run_id)
                    .await
                    .unwrap_or_else(|| "pending".to_string());
                if dep_status != "completed" {
                    return false;
                }
            }
        }
        true
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

    /// Checks if all dependencies for a given task are completed.
    pub async fn are_dependencies_completed<S: StateManager + ?Sized>(
        &self,
        state_manager: &Arc<S>,
        task_instance: &TaskInstance,
    ) -> bool {
        for dep_run_id in &task_instance.dependencies {
            let dep_status = state_manager
                .get_task_status(dep_run_id)
                .await
                .unwrap_or("pending".to_string());

            if dep_status != "completed" {
                debug!(
                "Dependency {} is not completed for task {}",
                dep_run_id, task_instance.run_id
            );
                return false;
            }
        }
        true
    }

}
