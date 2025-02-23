use crate::models::context::Context;
use crate::models::dag::{DagInstance, DagTemplate, GraphInstance};
use crate::models::task::{TaskInstance, TaskTemplate};
use crate::state::state_manager::StateManager;
use crate::utils::constants::{COMPLETED_STATUS, PENDING_STATUS};
use chrono::Utc;
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::debug;
use crate::utils::id_tools::generate_run_id;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecutionGraph {
    pub graph: DiGraph<String, ()>,
    pub node_map: HashMap<String, NodeIndex>,
}

impl ExecutionGraph {
    pub(crate) fn create_execution_from_dag(
        dag_template: DagTemplate,
        provided_context: Option<Context>,
    ) -> (DagInstance, Vec<TaskInstance>, GraphInstance) {
        
        // Create a new dag_run_id
        let dag_run_id = generate_run_id();

        // Create TaskInstances for each task in DagTemplate.
        let now = Utc::now();
        let mut task_instances = Vec::new();
        // We'll also build a mapping from task id to the corresponding node in the graph.
        let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

        for task in &dag_template.tasks {
            let instance_run_id = generate_run_id();
            let parameters_map = if let Some(obj) = task.parameters.as_object() {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.to_string()))
                    .collect()
            } else {
                HashMap::new()
            };
            let instance = TaskInstance {
                run_id: instance_run_id,
                task_id: task.id.clone(),
                dag_id: dag_template.id.clone(),
                name: task.name.clone(),
                description: task.description.clone(),
                command: task.command.clone(),
                parameters: parameters_map,
                queue: task.queue.clone().unwrap_or_else(|| "default".to_string()),
                status: PENDING_STATUS.to_string(),
                last_updated: now,
                started_at: None,
                completed_at: None,
                error_message: None,
                evaluation_point: task.evaluation_point.clone(),
            };
            task_instances.push(instance);
        }

        // Build the dependency graph using petgraph.
        // Nodes are the run IDs of the TaskInstances.
        let mut graph: DiGraph<String, ()> = DiGraph::new();
        // Add each TaskInstance as a node.
        for instance in &task_instances {
            let node = graph.add_node(instance.run_id.clone());
            node_map.insert(instance.task_id.clone(), node);
        }

        // Add edges: for each task, add an edge from each dependency to the task.
        for task in &dag_template.tasks {
            if let Some(&current_node) = node_map.get(&task.id) {
                for dep in &task.dependencies {
                    if let Some(&dep_node) = node_map.get(dep) {
                        // This adds an edge from the dependency (dep_node) to the current task (current_node)
                        graph.add_edge(dep_node, current_node, ());
                    }
                }
            }
        }

        let context = {
            if let Some(context) = provided_context {
                context
            } else {
                Context::new(&dag_run_id.clone())
            }
        };

        // Create the mutable DAG execution state.
        let dag_instance = DagInstance {
            run_id: dag_run_id.clone(),
            dag_id: dag_template.id.clone(),
            context,
            task_count: task_instances.len(),
            completed_tasks: 0,
            status: PENDING_STATUS.to_string(),
            last_updated: now,
            tags: None,
        };

        // Wrap the execution graph into a GraphInstance.
        // (Assuming ExecutionGraph is defined similarly to the previous implementation.)
        let exec_graph = ExecutionGraph { graph, node_map };
        let graph_instance = GraphInstance {
            run_id: dag_run_id,
            dag_id: dag_template.id,
            graph: exec_graph,
        };

        (dag_instance, task_instances, graph_instance)
    }

    pub fn create_execution_from_task(
        root: &TaskTemplate,
        all_tasks: &[TaskTemplate],
        provided_context: Option<Context>,
    ) -> (DagInstance, Vec<TaskInstance>, GraphInstance) {
        // Build a map for easy lookup: task id -> TaskTemplate.
        let task_map: HashMap<String, TaskTemplate> = all_tasks
            .iter()
            .cloned()
            .map(|t| (t.id.clone(), t))
            .collect();

        // Recursively collect all task ids that are in the dependency closure.
        let mut collected_ids = HashSet::new();
        fn collect_dependencies(
            task: &TaskTemplate,
            map: &HashMap<String, TaskTemplate>,
            collected: &mut HashSet<String>,
        ) {
            if collected.contains(&task.id) {
                return;
            }
            collected.insert(task.id.clone());
            for dep_id in &task.dependencies {
                if let Some(dep_task) = map.get(dep_id) {
                    collect_dependencies(dep_task, map, collected);
                }
            }
        }
        collect_dependencies(root, &task_map, &mut collected_ids);

        // Gather the relevant TaskTemplates.
        let relevant_tasks: Vec<TaskTemplate> = collected_ids
            .iter()
            .filter_map(|id| task_map.get(id).cloned())
            .collect();

        // (Optional) sort the tasks if you require deterministic ordering.
        // For example, you might perform a topological sort here.

        // Create a new dag_run_id and use the root task's id as the dag_id (or generate a new one).
        let dag_run_id = generate_run_id();
        let dag_id = root.id.clone();

        // Create TaskInstances for each resolved TaskTemplate.
        let now = Utc::now();
        let mut task_instances = Vec::new();
        // We'll also build a mapping from task id to the corresponding node in the graph.
        let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

        for task in &relevant_tasks {
            let instance_run_id = generate_run_id();
            let parameters_map = if let Some(obj) = task.parameters.as_object() {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.to_string()))
                    .collect()
            } else {
                HashMap::new()
            };
            let instance = TaskInstance {
                run_id: instance_run_id,
                task_id: task.id.clone(),
                dag_id: dag_id.clone(),
                name: task.name.clone(),
                description: task.description.clone(),
                command: task.command.clone(),
                parameters: parameters_map,
                queue: task.queue.clone().unwrap_or_else(|| "default".to_string()),
                status: PENDING_STATUS.to_string(),
                last_updated: now,
                started_at: None,
                completed_at: None,
                error_message: None,
                evaluation_point: task.evaluation_point.clone(),
            };
            task_instances.push(instance);
        }

        // Build the dependency graph using petgraph.
        // Nodes are the run IDs of the TaskInstances.
        let mut graph: DiGraph<String, ()> = DiGraph::new();
        // Add each TaskInstance as a node.
        for instance in &task_instances {
            let node = graph.add_node(instance.run_id.clone());
            node_map.insert(instance.task_id.clone(), node);
        }

        // Add edges: for each task, add an edge from each dependency to the task.
        for task in &relevant_tasks {
            if let Some(&current_node) = node_map.get(&task.id) {
                for dep in &task.dependencies {
                    if collected_ids.contains(dep) {
                        if let Some(&dep_node) = node_map.get(dep) {
                            // This adds an edge from the dependency (dep_node) to the current task (current_node)
                            graph.add_edge(dep_node, current_node, ());
                        }
                    }
                }
            }
        }

        let context = {
            if let Some(context) = provided_context {
                context
            } else {
                Context::new(&dag_run_id.clone())
            }
        };

        // Create the mutable DAG execution state.
        let dag_instance = DagInstance {
            run_id: dag_run_id.clone(),
            dag_id: dag_id.clone(),
            context,
            task_count: task_instances.len(),
            completed_tasks: 0,
            status: PENDING_STATUS.to_string(),
            last_updated: now,
            tags: None,
        };

        // Wrap the execution graph into a GraphInstance.
        // (Assuming ExecutionGraph is defined similarly to the previous implementation.)
        let exec_graph = ExecutionGraph { graph, node_map };
        let graph_instance = GraphInstance {
            run_id: dag_run_id,
            dag_id,
            graph: exec_graph,
        };

        (dag_instance, task_instances, graph_instance)
    }

    pub fn get_execution_order(&self) -> Vec<String> {
        match toposort(&self.graph, None) {
            Ok(sorted_nodes) => {
                let sorted_tasks: Vec<String> = sorted_nodes
                    .iter()
                    .map(|node| self.graph[*node].clone())
                    .collect();
                debug!("Topological Sorting Complete. Order: {:?}", sorted_tasks);
                sorted_tasks
            }
            Err(_) => panic!("Dependency cycle detected!"),
        }
    }

    pub async fn get_executable_tasks<S: StateManager + ?Sized>(
        &self,
        state_manager: &Arc<S>,
        _graph_instance: &GraphInstance,
    ) -> Vec<String> {
        let mut executable = Vec::new();
        for node in self.graph.node_indices() {
            let task_run_id = &self.graph[node];
            let status = state_manager
                .load_task_status(task_run_id)
                .await
                .unwrap_or(PENDING_STATUS.to_string());
            if status == COMPLETED_STATUS {
                continue;
            }
            let mut ready = true;
            for dep_node in self
                .graph
                .neighbors_directed(node, petgraph::Direction::Incoming)
            {
                let dep_id = &self.graph[dep_node];
                let dep_status = state_manager
                    .load_task_status(dep_id)
                    .await
                    .unwrap_or(PENDING_STATUS.to_string());
                if dep_status != COMPLETED_STATUS {
                    ready = false;
                    break;
                }
            }
            if ready {
                executable.push(task_run_id.clone());
            }
        }
        executable
    }
}
