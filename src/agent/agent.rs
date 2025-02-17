use crate::models::context::Context;
use crate::models::task::TaskInstance;
use crate::state::state_manager::StateManager;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;
use tracing::{error, info};
use uuid::Uuid;
use crate::utils::constants::{COMPLETED_STATUS, FAILED_STATUS, PENDING_STATUS, RUNNING_STATUS};

pub struct Agent<S: 'static + StateManager + ?Sized> {
    state_manager: Arc<S>,
    agent_id: String,
}

impl<S: 'static + StateManager + ?Sized> Agent<S> {
    /// Creates a new agent with a generated agent_id.
    pub fn new(state_manager: Arc<S>) -> Self {
        let agent_id = Uuid::new_v4().to_string();
        Agent {
            state_manager,
            agent_id,
        }
    }

    /// Starts the agent process:
    /// - Registers the agent  
    /// - Spawns a background heartbeat updater  
    /// - Enters a loop processing queues
    pub async fn run(&self, queues: Vec<String>) {
        // Register self with the state manager.
        self.state_manager.register_agent(&self.agent_id).await;
        info!("Agent {} registered.", self.agent_id);

        // Spawn a heartbeat task to update the agent's heartbeat every 5 seconds.
        let heartbeat_sm = self.state_manager.clone();
        let agent_id_clone = self.agent_id.clone();
        tokio::spawn(async move {
            loop {
                heartbeat_sm.update_agent_heartbeat(&agent_id_clone).await;
                sleep(Duration::from_secs(5)).await;
            }
        });

        // Main processing loop: iterate over the queues.
        loop {
            for queue in &queues {
                self.process_queue(queue).await;
            }
            sleep(Duration::from_secs(1)).await;
        }
    }

    /// Processes one queue iteration:
    /// - Pops a task from the queue  
    /// - Retrieves the corresponding DAG execution and task instance  
    /// - Registers assignment to this agent  
    /// - Executes the task, then removes the assignment
    async fn process_queue(&self, queue: &str) {
        if let Some((task_run_id, dag_run_id)) = self.state_manager.get_work_from_queue(queue).await
        {
            info!(
                "Agent {} picked up task instance: {} (DAG Run: {}) from queue: {}",
                self.agent_id, task_run_id, dag_run_id, queue
            );

            // Retrieve the DAG execution. If missing, the DAG is likely complete.
            let dag_execution = match self.state_manager.get_dag_execution(&dag_run_id).await {
                Some(d) => d,
                None => {
                    info!(
                        "DAG execution {} not found (likely completed). Discarding task {}.",
                        dag_run_id, task_run_id
                    );
                    return;
                }
            };

            // Find the specific task instance in the DAG execution.
            let task_instance = match dag_execution.tasks.iter().find(|t| t.run_id == task_run_id) {
                Some(t) => t,
                None => {
                    info!(
                        "Task instance {} not found in DAG {}. Skipping.",
                        task_run_id, dag_run_id
                    );
                    return;
                }
            };

            // Register assignment of this task to our agent.
            let assignment = format!("{}|{}", task_run_id, dag_run_id);
            self.state_manager
                .assign_task_to_agent(&self.agent_id, &assignment)
                .await;

            // Retrieve context and extract environment variables.
            let context = self
                .state_manager
                .get_context(&dag_execution.run_id)
                .await
                .unwrap_or_else(|| Context::new(&dag_execution.run_id));
            let env_vars = context.get_task_env(&task_instance.task_id, None, false);

            // Check if the task is already completed.
            let task_status = self
                .state_manager
                .get_task_status(&task_instance.run_id)
                .await
                .unwrap_or_else(|| PENDING_STATUS.to_string());
            if task_status == COMPLETED_STATUS {
                info!(
                    "Task {} is already completed, skipping execution.",
                    task_instance.run_id
                );
                self.state_manager
                    .remove_task_from_agent(&self.agent_id, &assignment)
                    .await;
                return;
            }

            // Mark the task as running.
            self.state_manager
                .update_task_status(&task_instance.run_id, RUNNING_STATUS)
                .await;

            // Execute the task.
            let success = self.execute_task(task_instance, env_vars).await;

            // Update the task status based on execution result.
            let new_status = if success { COMPLETED_STATUS } else { FAILED_STATUS };
            self.state_manager
                .update_task_status(&task_instance.run_id, new_status)
                .await;

            // Remove task assignment from agent.
            self.state_manager
                .remove_task_from_agent(&self.agent_id, &assignment)
                .await;

            // If task succeeded, notify graph update.
            if success {
                self.state_manager.notify_graph_update(&dag_run_id).await;
            }
        }
    }

    /// Executes the given task command with its environment variables.
    async fn execute_task(
        &self,
        task: &TaskInstance,
        env_vars: std::collections::HashMap<String, String>,
    ) -> bool {
        info!(
            "Executing task: {} (Command: {})",
            task.task_id, task.command
        );

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&task.command);

        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        match cmd.output().await {
            Ok(output) if output.status.success() => {
                info!("Task {} completed successfully", task.task_id);
                true
            }
            Ok(output) => {
                error!(
                    "Task {} failed with exit code {:?}",
                    task.task_id,
                    output.status.code()
                );
                false
            }
            Err(e) => {
                error!("Task {} execution failed: {}", task.task_id, e);
                false
            }
        }
    }
}
