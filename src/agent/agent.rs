use crate::state::state_manager::{StateManager, TaskPayload};
use crate::utils::constants::{COMPLETED_STATUS, FAILED_STATUS, PENDING_STATUS, RUNNING_STATUS};
use crate::utils::hostname_helper::get_hostname;
use crate::utils::id_tools::{generate_compound_run_id, strip_guid};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;
use tracing::{error, info};

pub struct Agent<S: 'static + StateManager + ?Sized> {
    state_manager: Arc<S>,
    agent_id: String,
}

impl<S: 'static + StateManager + ?Sized> Agent<S> {
    /// Creates a new agent with a generated agent_id.
    pub fn new(state_manager: Arc<S>) -> Self {
        let agent_id = generate_compound_run_id(get_hostname().as_str());
        Agent {
            state_manager,
            agent_id,
        }
    }

    /// Starts the agent process by registering, spawning a heartbeat, and processing queues.
    pub async fn run(&self, queues: Vec<String>) {
        self.state_manager.register_agent(&self.agent_id).await;
        info!("Agent {} registered.", self.agent_id);

        // Spawn heartbeat updater.
        let heartbeat_sm = self.state_manager.clone();
        let agent_id_clone = self.agent_id.clone();
        tokio::spawn(async move {
            loop {
                heartbeat_sm.update_agent_heartbeat(&agent_id_clone).await;
                sleep(Duration::from_secs(5)).await;
            }
        });

        // Main loop: process each queue.
        loop {
            for queue in &queues {
                self.process_queue(queue).await;
            }
            sleep(Duration::from_millis(100)).await;
        }
    }

    /// Processes a single queue iteration.
    async fn process_queue(&self, queue: &str) {
        if let Some(task_payload) = self.state_manager.get_work_from_queue(queue).await {
            info!(
                "Agent {} picked up task instance: {} (DAG Run: {}) from queue: {}",
                strip_guid(self.agent_id.as_str()),
                strip_guid(task_payload.task_run_id.as_str()),
                strip_guid(task_payload.dag_run_id.as_str()),
                queue
            );

            // Check if the DAG is still active.
            let dag_status = self
                .state_manager
                .load_dag_status(&task_payload.dag_run_id)
                .await;
            if let Ok(status) = dag_status {
                if status == COMPLETED_STATUS {
                    info!(
                        "DAG {} is completed. Discarding task {}.",
                        task_payload.dag_run_id, task_payload.task_run_id
                    );
                    return;
                }
            } else {
                info!(
                    "DAG {} status unavailable. Discarding task {}.",
                    task_payload.dag_run_id, task_payload.task_run_id
                );
                return;
            }

            let assignment = format!("{}|{}", task_payload.task_run_id, task_payload.dag_run_id);
            self.state_manager
                .assign_task_to_agent(&self.agent_id, &assignment)
                .await;

            // Check task status.
            let task_status = self
                .state_manager
                .load_task_status(&task_payload.task_run_id)
                .await
                .unwrap_or_else(|| PENDING_STATUS.to_string());
            if task_status == COMPLETED_STATUS {
                info!(
                    "Task {} is already completed, skipping execution.",
                    task_payload.task_run_id
                );
                self.state_manager
                    .remove_task_from_agent(&self.agent_id, &assignment)
                    .await;
                return;
            }

            // Mark as running and execute.
            self.state_manager
                .save_task_status(&task_payload.task_run_id, RUNNING_STATUS)
                .await;
            let success = self.execute_task(&task_payload).await;
            let new_status = if success {
                COMPLETED_STATUS
            } else {
                FAILED_STATUS
            };
            self.state_manager
                .save_task_status(&task_payload.task_run_id, new_status)
                .await;
            self.state_manager
                .remove_task_from_agent(&self.agent_id, &assignment)
                .await;

            if success {
                self.state_manager
                    .publish_graph_update(&task_payload.dag_run_id)
                    .await;
            }
        }
    }

    /// Executes the command in the task payload.
    async fn execute_task(&self, task_payload: &TaskPayload) -> bool {
        info!(
            "Executing task: {} (Command: {})",
            strip_guid(task_payload.task_run_id.as_str()),
            task_payload.command
        );
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&task_payload.command);
        for (key, value) in &task_payload.env_vars {
            cmd.env(key, value);
        }
        match cmd.output().await {
            Ok(output) if output.status.success() => {
                info!(
                    "Task {} completed successfully",
                    strip_guid(task_payload.task_run_id.as_str())
                );
                true
            }
            Ok(output) => {
                error!(
                    "Task {} failed with exit code {:?}",
                    task_payload.task_run_id,
                    output.status.code()
                );
                false
            }
            Err(e) => {
                error!("Task {} execution failed: {}", task_payload.task_run_id, e);
                false
            }
        }
    }
}
