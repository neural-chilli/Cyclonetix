use crate::models::context::Context;
use crate::models::task::Task;
use std::sync::Arc;
use tokio::process::Command;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};
use crate::state::state_manager::StateManager;

pub struct Worker<S: StateManager + ?Sized> {
    state_manager: Arc<S>,
}

impl<S: StateManager + ?Sized> Worker<S> {
    pub fn new(state_manager: Arc<S>) -> Self {
        Worker { state_manager }
    }

    pub async fn run(&self, queue: &str) {
        loop {
            if let Some((task_id, context_id)) = self.state_manager.get_work_from_queue(queue).await
            {
                info!(
                    "🚀 Worker picked up task instance: {} (Context: {})",
                    task_id, context_id
                );

                // Retrieve task details
                let task = match self.state_manager.get_task_definition(&task_id).await {
                    Some(t) => t,
                    None => {
                        error!("Task definition not found for: {}", task_id);
                        continue;
                    }
                };

                // Retrieve execution context
                let context = self
                    .state_manager
                    .get_context(&context_id)
                    .await
                    .unwrap_or(Context::new(&context_id));

                // Prepare execution environment
                let env_vars = context.get_task_env(&task_id, None, false);

                // Check if task is already completed before executing
                let task_status = self
                    .state_manager
                    .get_task_status(&task_id, &context_id)
                    .await
                    .unwrap_or_else(|| "pending".to_string());
                if task_status == "completed" {
                    info!(
                        "🔍 Task {} is already marked as completed, skipping execution.",
                        task_id
                    );
                    continue;
                }

                // Mark task as running
                self.state_manager
                    .update_task_status(&task_id, &context_id, "running")
                    .await;

                // Execute task
                let success = self.execute_task(&task, env_vars).await;

                // Mark task as completed or failed
                if success {
                    info!(
                        "Task {} successfully completed, updating status in Redis.",
                        task_id
                    );
                    self.state_manager
                        .update_task_status(&task_id, &context_id, "completed")
                        .await;
                } else {
                    error!("Task {} execution failed, marking as failed.", task_id);
                    self.state_manager
                        .update_task_status(&task_id, &context_id, "failed")
                        .await;
                }

                // Trigger graph evaluation only if the task was completed successfully
                if success {
                    self.state_manager.notify_graph_update(&context_id).await;
                }
            }

            sleep(Duration::from_secs(1)).await; // Avoid busy looping
        }
    }

    async fn execute_task(
        &self,
        task: &Task,
        env_vars: std::collections::HashMap<String, String>,
    ) -> bool {
        info!("🔧 Executing task: {} (Command: {})", task.id, task.command);

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&task.command);

        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let output = cmd.output().await;

        match output {
            Ok(output) => {
                if output.status.success() {
                    info!("Task {} completed successfully", task.id);
                    true
                } else {
                    error!(
                        "Task {} failed with exit code {:?}",
                        task.id,
                        output.status.code()
                    );
                    false
                }
            }
            Err(e) => {
                error!("Task {} execution failed: {}", task.id, e);
                false
            }
        }
    }
}
