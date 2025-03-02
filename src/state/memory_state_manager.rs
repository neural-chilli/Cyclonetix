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