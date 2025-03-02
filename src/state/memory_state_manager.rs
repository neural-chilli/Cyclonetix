use crate::models::context::Context;
use crate::models::dag::{DagInstance, DagTemplate, GraphInstance};
use crate::models::task::{TaskInstance, TaskTemplate};
use crate::state::state_manager::{AgentStatus, StateManager, TaskPayload};
use crate::utils::config::SerializationFormat;
use crate::utils::constants::{PENDING_STATUS};
use async_trait::async_trait;
use dashmap::DashMap;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use std::collections::BTreeMap;
use tracing::{debug, error, info, warn};

// Struct to hold an in-memory queue
struct InMemoryQueue {
    tasks: RwLock<VecDeque<TaskPayload>>,
}

impl InMemoryQueue {
    fn new() -> Self {
        Self {
            tasks: RwLock::new(VecDeque::new()),
        }
    }

    fn push_front(&self, task: TaskPayload) {
        let mut tasks = self.tasks.write().unwrap();
        tasks.push_front(task);
    }

    fn push_back(&self, task: TaskPayload) {
        let mut tasks = self.tasks.write().unwrap();
        tasks.push_back(task);
    }

    fn pop_back(&self) -> Option<TaskPayload> {
        let mut tasks = self.tasks.write().unwrap();
        tasks.pop_back()
    }

    fn get_all(&self) -> Vec<String> {
        let tasks = self.tasks.read().unwrap();
        tasks.iter().map(|task| task.task_run_id.clone()).collect()
    }
}

// In-memory pub/sub channel
#[derive(Clone)]
struct PubSubChannel {
    sender: broadcast::Sender<String>,
}

impl PubSubChannel {
    fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    fn publish(&self, message: String) -> Result<usize, broadcast::error::SendError<String>> {
        self.sender.send(message)
    }

    fn subscribe(&self) -> broadcast::Receiver<String> {
        self.sender.subscribe()
    }
}

pub struct MemoryStateManager {
    // Prefixes and unique identification
    cluster_id: String,
    serialization_format: SerializationFormat,

    // Storage
    task_templates: DashMap<String, TaskTemplate>,
    task_instances: DashMap<String, TaskInstance>,
    dag_templates: DashMap<String, DagTemplate>,
    dag_instances: DashMap<String, DagInstance>,
    dag_statuses: DashMap<String, String>,
    graph_instances: DashMap<String, GraphInstance>,
    graph_statuses: DashMap<String, String>,
    contexts: DashMap<String, Context>,

    // Queues
    queues: DashMap<String, Arc<InMemoryQueue>>,

    // Agents
    agents: DashMap<String, AgentStatus>,
    agent_tasks: DashMap<String, HashSet<String>>,

    // Orchestration
    orchestrator_count: Mutex<u32>,
    orchestrator_id: Mutex<u32>,

    // Pub/Sub
    pubsub_channel: PubSubChannel,
}

impl MemoryStateManager {
    pub async fn new(cluster_id: &str, format: SerializationFormat) -> Self {
        let instance = Self {
            cluster_id: cluster_id.to_string(),
            serialization_format: format,
            task_templates: DashMap::new(),
            task_instances: DashMap::new(),
            dag_templates: DashMap::new(),
            dag_instances: DashMap::new(),
            dag_statuses: DashMap::new(),
            graph_instances: DashMap::new(),
            graph_statuses: DashMap::new(),
            contexts: DashMap::new(),
            queues: DashMap::new(),
            agents: DashMap::new(),
            agent_tasks: DashMap::new(),
            orchestrator_count: Mutex::new(1),
            orchestrator_id: Mutex::new(0),
            pubsub_channel: PubSubChannel::new(100),
        };

        // Initialize default queue
        instance.queues.insert("work_queue".to_string(), Arc::new(InMemoryQueue::new()));

        instance
    }

    // Helper method to get or create a queue
    fn get_queue(&self, queue_name: &str) -> Arc<InMemoryQueue> {
        if let Some(queue) = self.queues.get(queue_name) {
            queue.clone()
        } else {
            let queue = Arc::new(InMemoryQueue::new());
            self.queues.insert(queue_name.to_string(), queue.clone());
            queue
        }
    }

    // Helper to get timestamp
    fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }
}

#[async_trait]
impl StateManager for MemoryStateManager {
    async fn get_work_from_queue(&self, queue: &str) -> Option<TaskPayload> {
        let queue = self.get_queue(queue);
        queue.pop_back()
    }

    async fn put_work_on_queue(&self, task_payload: &TaskPayload, queue_name: &str) {
        let queue = self.get_queue(queue_name);
        queue.push_back(task_payload.clone());
        debug!(
            "Enqueued task payload {} to queue {}",
            task_payload.task_run_id, queue_name
        );
    }

    async fn save_task(&self, task: &TaskTemplate) {
        self.task_templates.insert(task.id.clone(), task.clone());
    }

    async fn load_task(&self, task_id: &str) -> Option<TaskTemplate> {
        self.task_templates.get(task_id).map(|t| t.clone())
    }

    async fn load_all_tasks(&self) -> Vec<TaskTemplate> {
        self.task_templates.iter().map(|t| t.clone()).collect()
    }

    async fn load_task_status(&self, task_instance_run_id: &str) -> Option<String> {
        self.task_instances.get(task_instance_run_id).map(|t| t.status.clone())
    }

    async fn save_task_status(&self, task_instance_run_id: &str, status: &str) {
        if let Some(mut task_instance) = self.task_instances.get_mut(task_instance_run_id) {
            task_instance.status = status.to_string();
        }
    }

    async fn is_task_scheduled(&self, task_instance_run_id: &str) -> bool {
        if let Some(task_instance) = self.task_instances.get(task_instance_run_id) {
            let queue = self.get_queue(&task_instance.queue);
            let tasks = queue.get_all();
            tasks.contains(&task_instance_run_id.to_string())
        } else {
            false
        }
    }

    async fn load_task_instance(&self, task_run_id: &str) -> Option<TaskInstance> {
        self.task_instances.get(task_run_id).map(|t| t.clone())
    }

    async fn save_task_instance(&self, task: &TaskInstance) {
        self.task_instances.insert(task.run_id.clone(), task.clone());
    }

    async fn save_context(&self, context: &Context) {
        self.contexts.insert(context.id.clone(), context.clone());
    }

    async fn load_context(&self, context_id: &str) -> Option<Context> {
        self.contexts.get(context_id).map(|c| c.clone())
    }

    async fn save_dag_instance(&self, dag_instance: &DagInstance) {
        self.dag_instances.insert(dag_instance.run_id.clone(), dag_instance.clone());
        self.dag_statuses.insert(dag_instance.run_id.clone(), dag_instance.status.clone());
        debug!("Stored DagInstance: {}", dag_instance.run_id);
    }

    async fn load_dag_instance(&self, run_id: &str) -> Option<DagInstance> {
        self.dag_instances.get(run_id).map(|d| d.clone())
    }

    async fn load_scheduled_dag_instances(&self) -> Vec<DagInstance> {
        self.dag_instances.iter().map(|d| d.clone()).collect()
    }

    async fn delete_dag_instance(
        &self,
        run_id: &str,
        task_run_ids: &[String],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.dag_instances.remove(run_id);
        self.dag_statuses.remove(run_id);

        for task_run_id in task_run_ids {
            self.task_instances.remove(task_run_id);
        }

        info!("Cleaned up DagInstance: {}", run_id);
        Ok(())
    }

    async fn load_dag_status(&self, run_id: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        match self.dag_statuses.get(run_id) {
            Some(status) => Ok(status.clone()),
            None => Ok(PENDING_STATUS.to_string())
        }
    }

    async fn save_dag_status(&self, run_id: &str, status: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.dag_statuses.insert(run_id.to_string(), status.to_string());
        debug!("Updated DAG status for {}: {}", run_id, status);
        Ok(())
    }

    async fn save_graph_instance(&self, graph_instance: &GraphInstance) {
        self.graph_instances.insert(graph_instance.run_id.clone(), graph_instance.clone());
        self.graph_statuses.insert(graph_instance.run_id.clone(), PENDING_STATUS.to_string());
        debug!("Stored GraphInstance: {}", graph_instance.run_id);
    }

    async fn load_graph_instance(&self, run_id: &str) -> Option<GraphInstance> {
        self.graph_instances.get(run_id).map(|g| g.clone())
    }

    async fn save_dag_template(&self, dag: &DagTemplate) {
        self.dag_templates.insert(dag.id.clone(), dag.clone());
    }

    async fn load_dag_template(&self, dag_id: &str) -> Option<DagTemplate> {
        self.dag_templates.get(dag_id).map(|d| d.clone())
    }

    async fn publish_graph_update(&self, run_id: &str) {
        if let Err(e) = self.pubsub_channel.publish(run_id.to_string()) {
            error!("Failed to publish graph update: {}", e);
        }
        debug!("Published graph update for run_id: {}", run_id);
    }

    async fn subscribe_to_graph_updates(self: Arc<Self>) {
        let mut receiver = self.pubsub_channel.subscribe();

        info!("Subscribed to graph updates.");

        while let Ok(run_id) = receiver.recv().await {
            debug!("Graph update received for run_id: {}", run_id);
            if let Some(graph_instance) = self.load_graph_instance(&run_id).await {
                crate::graph::orchestrator::evaluate_graph(self.clone(), &graph_instance).await;
            } else {
                error!("No GraphInstance found for run_id: {}", run_id);
            }
        }
    }

    async fn get_orchestrator_count(&self) -> Result<u32, Box<dyn Error + Send + Sync>> {
        Ok(*self.orchestrator_count.lock().unwrap())
    }

    async fn get_orchestrator_id(&self) -> Result<u32, Box<dyn Error + Send + Sync>> {
        Ok(*self.orchestrator_id.lock().unwrap())
    }

    async fn register_agent(&self, agent_id: &str) {
        let agent_status = AgentStatus {
            agent_id: agent_id.to_string(),
            last_heartbeat: Self::current_timestamp(),
            tasks: Vec::new(),
        };

        self.agents.insert(agent_id.to_string(), agent_status);
        self.agent_tasks.insert(agent_id.to_string(), HashSet::new());

        info!("Registered agent {}", agent_id);
    }

    async fn update_agent_heartbeat(&self, agent_id: &str) {
        if let Some(mut agent) = self.agents.get_mut(agent_id) {
            agent.last_heartbeat = Self::current_timestamp();
            debug!("Updated heartbeat for agent {}", agent_id);
        }
    }

    async fn assign_task_to_agent(&self, agent_id: &str, assignment: &str) {
        if let Some(mut tasks) = self.agent_tasks.get_mut(agent_id) {
            tasks.insert(assignment.to_string());

            // Update the tasks list in the agent status
            if let Some(mut agent) = self.agents.get_mut(agent_id) {
                agent.tasks = tasks.iter().cloned().collect();
            }

            debug!("Assigned task {} to agent {}", assignment, agent_id);
        }
    }

    async fn remove_task_from_agent(&self, agent_id: &str, assignment: &str) {
        if let Some(mut tasks) = self.agent_tasks.get_mut(agent_id) {
            tasks.remove(assignment);

            // Update the tasks list in the agent status
            if let Some(mut agent) = self.agents.get_mut(agent_id) {
                agent.tasks = tasks.iter().cloned().collect();
            }

            debug!("Removed task {} from agent {}", assignment, agent_id);
        }
    }

    async fn load_all_agents(&self) -> Vec<AgentStatus> {
        self.agents.iter().map(|a| a.clone()).collect()
    }

    async fn get_queue_tasks(&self, queue: &str) -> Vec<String> {
        let queue = self.get_queue(queue);
        queue.get_all()
    }

    async fn get_dag_visualization(&self, _run_id: &str) -> Option<String> {
        None // Not implemented
    }

    async fn reset_tasks_from_downed_agents(
        &self,
        heartbeat_threshold: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let current_time = Self::current_timestamp();
        let agents = self.load_all_agents().await;

        for agent in agents {
            if current_time - agent.last_heartbeat > heartbeat_threshold as i64 {
                info!("Agent {} is down. Resetting its tasks.", agent.agent_id);

                if let Some(tasks) = self.agent_tasks.get(&agent.agent_id) {
                    let assignments: Vec<String> = tasks.iter().cloned().collect();

                    for assignment in assignments {
                        let parts: Vec<&str> = assignment.split('|').collect();
                        if parts.len() != 2 {
                            warn!(
                                "Malformed assignment {} for agent {}",
                                assignment, agent.agent_id
                            );
                            continue;
                        }

                        let task_run_id = parts[0];
                        let dag_run_id = parts[1];

                        if let Some(dag_instance) = self.load_dag_instance(dag_run_id).await {
                            self.save_task_status(task_run_id, PENDING_STATUS).await;
                            if let Some(task) = self.load_task_instance(task_run_id).await {
                                let task_payload = TaskPayload {
                                    task_run_id: task_run_id.to_string(),
                                    dag_run_id: dag_run_id.to_string(),
                                    command: task.command.clone(),
                                    env_vars: dag_instance.context.get_task_env(task.clone()),
                                };
                                self.put_work_on_queue(&task_payload, &task.queue).await;
                            } else {
                                warn!("Task instance {} not found during reset", task_run_id);
                            }
                        } else {
                            warn!(
                                "DagInstance {} not found while resetting task {} for downed agent {}",
                                dag_run_id, task_run_id, agent.agent_id
                            );
                        }
                    }
                }

                // Remove the agent and its tasks
                self.agents.remove(&agent.agent_id);
                self.agent_tasks.remove(&agent.agent_id);
                debug!("Agent {} removed from active agents.", agent.agent_id);
            }
        }

        Ok(())
    }

    async fn load_scheduled_dag_keys(&self) -> Vec<String> {
        self.dag_instances.iter().map(|d| d.key().clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::models::context::Context;
    use crate::models::dag::{DagInstance, DagTemplate, GraphInstance};
    use crate::models::task::{TaskInstance, TaskTemplate};
    use crate::state::memory_state_manager::MemoryStateManager;
    use crate::state::state_manager::{StateManager, TaskPayload};
    use crate::utils::config::SerializationFormat;
    use crate::utils::constants::{COMPLETED_STATUS, FAILED_STATUS, PENDING_STATUS, RUNNING_STATUS};
    use crate::graph::graph_manager::ExecutionGraph;
    use chrono::Utc;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    async fn create_test_state_manager() -> Arc<MemoryStateManager> {
        Arc::new(MemoryStateManager::new("test-cluster", SerializationFormat::Json).await)
    }

    #[tokio::test]
    async fn test_task_operations() {
        let state_manager = create_test_state_manager().await;

        // Create a test task template
        let task = TaskTemplate {
            id: "task1".to_string(),
            name: "Test Task".to_string(),
            description: Some("A test task".to_string()),
            command: "echo 'test'".to_string(),
            parameters: json!({}),
            dependencies: vec![],
            queue: Some("test_queue".to_string()),
            evaluation_point: false,
        };

        // Save the task
        state_manager.save_task(&task).await;

        // Load the task and verify
        let loaded_task = state_manager.load_task("task1").await.expect("Failed to load task");
        assert_eq!(loaded_task.id, task.id);
        assert_eq!(loaded_task.name, task.name);
        assert_eq!(loaded_task.command, task.command);

        // Load all tasks
        let all_tasks = state_manager.load_all_tasks().await;
        assert_eq!(all_tasks.len(), 1);
        assert_eq!(all_tasks[0].id, task.id);
    }

    #[tokio::test]
    async fn test_task_instance_operations() {
        let state_manager = create_test_state_manager().await;

        // Create a test task instance
        let task_instance = TaskInstance {
            run_id: "run1".to_string(),
            task_id: "task1".to_string(),
            dag_id: "dag1".to_string(),
            name: "Test Task Instance".to_string(),
            description: Some("A test task instance".to_string()),
            command: "echo 'test'".to_string(),
            parameters: HashMap::new(),
            queue: "test_queue".to_string(),
            status: PENDING_STATUS.to_string(),
            last_updated: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            evaluation_point: false,
        };

        // Save the task instance
        state_manager.save_task_instance(&task_instance).await;

        // Load the task instance and verify
        let loaded_instance = state_manager.load_task_instance("run1").await.expect("Failed to load task instance");
        assert_eq!(loaded_instance.run_id, task_instance.run_id);
        assert_eq!(loaded_instance.status, PENDING_STATUS);

        // Update task status
        state_manager.save_task_status("run1", RUNNING_STATUS).await;
        let updated_status = state_manager.load_task_status("run1").await.expect("Failed to load task status");
        assert_eq!(updated_status, RUNNING_STATUS);
    }

    #[tokio::test]
    async fn test_context_operations() {
        let state_manager = create_test_state_manager().await;

        // Create test context
        let mut context = Context::new();
        context.id = "ctx1".to_string();
        context.set("key1", "value1");
        context.set("key2", "value2");

        // Save the context
        state_manager.save_context(&context).await;

        // Load the context and verify
        let loaded_context = state_manager.load_context("ctx1").await.expect("Failed to load context");
        assert_eq!(loaded_context.id, context.id);
        assert_eq!(loaded_context.get("key1"), Some(&"value1".to_string()));
        assert_eq!(loaded_context.get("key2"), Some(&"value2".to_string()));
    }

    #[tokio::test]
    async fn test_queue_operations() {
        let state_manager = create_test_state_manager().await;

        // Create task payload
        let task_payload = TaskPayload {
            task_run_id: "task1".to_string(),
            dag_run_id: "dag1".to_string(),
            command: "echo 'test'".to_string(),
            env_vars: HashMap::new(),
        };

        // Push to queue
        state_manager.put_work_on_queue(&task_payload, "test_queue").await;

        // Check queue contents
        let queue_tasks = state_manager.get_queue_tasks("test_queue").await;
        assert_eq!(queue_tasks.len(), 1);

        // Get work from queue
        let retrieved_payload = state_manager.get_work_from_queue("test_queue").await.expect("Failed to get work from queue");
        assert_eq!(retrieved_payload.task_run_id, task_payload.task_run_id);
        assert_eq!(retrieved_payload.dag_run_id, task_payload.dag_run_id);

        // Queue should now be empty
        assert!(state_manager.get_work_from_queue("test_queue").await.is_none());
    }

    #[tokio::test]
    async fn test_agent_operations() {
        let state_manager = create_test_state_manager().await;

        // Register agent
        state_manager.register_agent("agent1").await;

        // Update heartbeat
        state_manager.update_agent_heartbeat("agent1").await;

        // Assign task to agent
        state_manager.assign_task_to_agent("agent1", "task1|dag1").await;

        // Load all agents
        let agents = state_manager.load_all_agents().await;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].agent_id, "agent1");
        assert_eq!(agents[0].tasks.len(), 1);
        assert!(agents[0].tasks.contains(&"task1|dag1".to_string()));

        // Remove task from agent
        state_manager.remove_task_from_agent("agent1", "task1|dag1").await;

        // Check that task was removed
        let updated_agents = state_manager.load_all_agents().await;
        assert_eq!(updated_agents[0].tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_dag_operations() {
        let state_manager = create_test_state_manager().await;

        // Create DAG template
        let dag_template = DagTemplate {
            id: "dag1".to_string(),
            name: "Test DAG".to_string(),
            description: Some("A test DAG".to_string()),
            tasks: vec![],
            tags: Some(vec!["test".to_string()]),
        };

        // Save DAG template
        state_manager.save_dag_template(&dag_template).await;

        // Load DAG template
        let loaded_template = state_manager.load_dag_template("dag1").await.expect("Failed to load DAG template");
        assert_eq!(loaded_template.id, dag_template.id);
        assert_eq!(loaded_template.name, dag_template.name);

        // Create DAG instance
        let dag_instance = DagInstance {
            run_id: "run1".to_string(),
            dag_id: "dag1".to_string(),
            context: Context::new(),
            task_count: 2,
            completed_tasks: 0,
            status: PENDING_STATUS.to_string(),
            last_updated: Utc::now(),
            tags: Some(vec!["test".to_string()]),
        };

        // Save DAG instance
        state_manager.save_dag_instance(&dag_instance).await;

        // Load DAG instance
        let loaded_instance = state_manager.load_dag_instance("run1").await.expect("Failed to load DAG instance");
        assert_eq!(loaded_instance.run_id, dag_instance.run_id);
        assert_eq!(loaded_instance.status, PENDING_STATUS);

        // Update DAG status
        state_manager.save_dag_status("run1", RUNNING_STATUS).await.expect("Failed to save DAG status");
        let updated_status = state_manager.load_dag_status("run1").await.expect("Failed to load DAG status");
        assert_eq!(updated_status, RUNNING_STATUS);

        // Load scheduled DAG instances
        let scheduled_dags = state_manager.load_scheduled_dag_instances().await;
        assert_eq!(scheduled_dags.len(), 1);
        assert_eq!(scheduled_dags[0].run_id, dag_instance.run_id);

        // Load scheduled DAG keys
        let keys = state_manager.load_scheduled_dag_keys().await;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], "run1");
    }

    #[tokio::test]
    async fn test_graph_operations() {
        let state_manager = create_test_state_manager().await;

        // Create mock execution graph
        let mut graph = petgraph::graph::DiGraph::new();
        let node1 = graph.add_node("task1".to_string());
        let node2 = graph.add_node("task2".to_string());
        graph.add_edge(node1, node2, ());

        let mut node_map = HashMap::new();
        node_map.insert("task1".to_string(), node1);
        node_map.insert("task2".to_string(), node2);

        let exec_graph = ExecutionGraph {
            graph,
            node_map,
        };

        // Create graph instance
        let graph_instance = GraphInstance {
            run_id: "run1".to_string(),
            dag_id: "dag1".to_string(),
            graph: exec_graph,
        };

        // Save graph instance
        state_manager.save_graph_instance(&graph_instance).await;

        // Load graph instance
        let loaded_graph = state_manager.load_graph_instance("run1").await.expect("Failed to load graph instance");
        assert_eq!(loaded_graph.run_id, graph_instance.run_id);
        assert_eq!(loaded_graph.dag_id, graph_instance.dag_id);
    }

    #[tokio::test]
    async fn test_pub_sub() {
        let state_manager = Arc::new(MemoryStateManager::new("test-cluster", SerializationFormat::Json).await);

        // Create mock graph instance for updates
        let mut graph = petgraph::graph::DiGraph::new();
        let node = graph.add_node("task1".to_string());

        let mut node_map = HashMap::new();
        node_map.insert("task1".to_string(), node);

        let exec_graph = ExecutionGraph {
            graph,
            node_map,
        };

        let graph_instance = GraphInstance {
            run_id: "run1".to_string(),
            dag_id: "dag1".to_string(),
            graph: exec_graph,
        };

        // Save the graph instance
        state_manager.save_graph_instance(&graph_instance).await;

        // Create DAG instance
        let dag_instance = DagInstance {
            run_id: "run1".to_string(),
            dag_id: "dag1".to_string(),
            context: Context::new(),
            task_count: 1,
            completed_tasks: 0,
            status: PENDING_STATUS.to_string(),
            last_updated: Utc::now(),
            tags: None,
        };

        // Save the DAG instance
        state_manager.save_dag_instance(&dag_instance).await;

        // Create a flag to track if we received the update
        let received = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let received_clone = received.clone();

        // Create a subscriber in a separate task
        let state_manager_for_subscriber = state_manager.clone();
        tokio::spawn(async move {
            let mut rx = state_manager_for_subscriber.pubsub_channel.subscribe();
            if let Ok(msg) = rx.recv().await {
                if msg == "run1" {
                    received_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
        });

        // Give the subscriber time to set up
        sleep(Duration::from_millis(100)).await;

        // Publish a graph update
        state_manager.publish_graph_update("run1").await;

        // Give time for the update to propagate
        sleep(Duration::from_millis(500)).await;

        // Check if we received the update
        assert!(received.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_reset_tasks_from_downed_agents() {
        let state_manager = create_test_state_manager().await;

        // Register agent
        state_manager.register_agent("agent1").await;

        // Create a task instance
        let task_instance = TaskInstance {
            run_id: "task1".to_string(),
            task_id: "task-id1".to_string(),
            dag_id: "dag1".to_string(),
            name: "Test Task".to_string(),
            description: None,
            command: "echo 'test'".to_string(),
            parameters: HashMap::new(),
            queue: "test_queue".to_string(),
            status: RUNNING_STATUS.to_string(),
            last_updated: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: None,
            error_message: None,
            evaluation_point: false,
        };

        // Create DAG instance
        let dag_instance = DagInstance {
            run_id: "dag1".to_string(),
            dag_id: "dag-template-1".to_string(),
            context: Context::new(),
            task_count: 1,
            completed_tasks: 0,
            status: RUNNING_STATUS.to_string(),
            last_updated: Utc::now(),
            tags: None,
        };

        // Save task and DAG
        state_manager.save_task_instance(&task_instance).await;
        state_manager.save_dag_instance(&dag_instance).await;

        // Assign task to agent
        let assignment = format!("{}|{}", task_instance.run_id, dag_instance.run_id);
        state_manager.assign_task_to_agent("agent1", &assignment).await;

        // Force the heartbeat to be old (directly modify the agent)
        {
            if let Some(mut agent) = state_manager.agents.get_mut("agent1") {
                agent.last_heartbeat -= 100; // Set heartbeat 100 seconds in the past
            }
        }

        // Call reset tasks
        state_manager.reset_tasks_from_downed_agents(10).await.expect("Failed to reset tasks");

        // Check if agent is removed
        let agents = state_manager.load_all_agents().await;
        assert_eq!(agents.len(), 0);

        // Check if task is back in the queue
        let queue_tasks = state_manager.get_queue_tasks("test_queue").await;
        assert_eq!(queue_tasks.len(), 1);

        // Verify task status was reset to pending
        let updated_status = state_manager.load_task_status("task1").await.expect("Failed to load task status");
        assert_eq!(updated_status, PENDING_STATUS);
    }

    #[tokio::test]
    async fn test_orchestrator_settings() {
        let state_manager = create_test_state_manager().await;

        // Default values
        let count = state_manager.get_orchestrator_count().await.expect("Failed to get orchestrator count");
        assert_eq!(count, 1);

        let id = state_manager.get_orchestrator_id().await.expect("Failed to get orchestrator ID");
        assert_eq!(id, 0);

        // Update values directly for testing
        {
            let mut count_lock = state_manager.orchestrator_count.lock().unwrap();
            *count_lock = 3;

            let mut id_lock = state_manager.orchestrator_id.lock().unwrap();
            *id_lock = 2;
        }

        // Check updated values
        let updated_count = state_manager.get_orchestrator_count().await.expect("Failed to get updated count");
        assert_eq!(updated_count, 3);

        let updated_id = state_manager.get_orchestrator_id().await.expect("Failed to get updated ID");
        assert_eq!(updated_id, 2);
    }

    #[tokio::test]
    async fn test_delete_dag_instance() {
        let state_manager = create_test_state_manager().await;

        // Create DAG instance
        let dag_instance = DagInstance {
            run_id: "run1".to_string(),
            dag_id: "dag1".to_string(),
            context: Context::new(),
            task_count: 2,
            completed_tasks: 0,
            status: PENDING_STATUS.to_string(),
            last_updated: Utc::now(),
            tags: None,
        };

        // Create task instances
        let task1 = TaskInstance {
            run_id: "task1".to_string(),
            task_id: "task-id1".to_string(),
            dag_id: "dag1".to_string(),
            name: "Task 1".to_string(),
            description: None,
            command: "echo 'task1'".to_string(),
            parameters: HashMap::new(),
            queue: "test_queue".to_string(),
            status: COMPLETED_STATUS.to_string(),
            last_updated: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
            error_message: None,
            evaluation_point: false,
        };

        let task2 = TaskInstance {
            run_id: "task2".to_string(),
            task_id: "task-id2".to_string(),
            dag_id: "dag1".to_string(),
            name: "Task 2".to_string(),
            description: None,
            command: "echo 'task2'".to_string(),
            parameters: HashMap::new(),
            queue: "test_queue".to_string(),
            status: PENDING_STATUS.to_string(),
            last_updated: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            evaluation_point: false,
        };

        // Save DAG and tasks
        state_manager.save_dag_instance(&dag_instance).await;
        state_manager.save_task_instance(&task1).await;
        state_manager.save_task_instance(&task2).await;

        // Verify they exist
        assert!(state_manager.load_dag_instance("run1").await.is_some());
        assert!(state_manager.load_task_instance("task1").await.is_some());
        assert!(state_manager.load_task_instance("task2").await.is_some());

        // Delete DAG instance
        state_manager.delete_dag_instance("run1", &["task1".to_string(), "task2".to_string()]).await
            .expect("Failed to delete DAG instance");

        // Verify they're gone
        assert!(state_manager.load_dag_instance("run1").await.is_none());
        assert!(state_manager.load_task_instance("task1").await.is_none());
        assert!(state_manager.load_task_instance("task2").await.is_none());
    }
}