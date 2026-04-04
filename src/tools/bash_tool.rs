use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::process::Command;

use crate::state::{BackgroundTask, BackgroundTaskStatus};
use crate::tool::{Tool, ToolProgress, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command and return its output."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds (max 600000)"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this command in the background"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let command = ctx
            .input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Bash".into(),
                message: "Missing 'command' parameter".into(),
            })?;

        let run_in_background = ctx
            .input
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if run_in_background {
            return self.execute_background(command, &ctx).await;
        }

        let timeout_ms = ctx
            .input
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000)
            .min(600_000);

        let _ = ctx
            .progress_tx
            .send(ToolProgress::Output(format!("$ {}\n", command)));

        let result = tokio::select! {
            result = run_command(command) => result,
            _ = tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)) => {
                return Ok(ToolResult {
                    content: format!("Command timed out after {}ms", timeout_ms),
                    is_error: true,
                    content_blocks: None,
                });
            }
            _ = ctx.abort_signal.cancelled() => {
                return Err(crate::types::AtomError::Cancelled);
            }
        };

        match result {
            Ok((stdout, stderr, exit_code)) => {
                let mut output = String::new();
                if !stdout.is_empty() {
                    output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(&stderr);
                }
                if output.is_empty() {
                    output = format!("Command completed with exit code {}", exit_code);
                }

                Ok(ToolResult {
                    content: output,
                    is_error: exit_code != 0,
                    content_blocks: None,
                })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Failed to execute command: {}", e),
                is_error: true,
                content_blocks: None,
            }),
        }
    }

    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            command: input
                .get("command")
                .and_then(|v| v.as_str())
                .map(String::from),
            file_path: None,
            url: None,
        }
    }
}

impl BashTool {
    async fn execute_background(
        &self,
        command: &str,
        ctx: &ToolUseContext,
    ) -> crate::types::Result<ToolResult> {
        let task_id = format!("bg-{}", uuid::Uuid::new_v4());
        let output_buf = Arc::new(Mutex::new(String::new()));
        let cancel_token = tokio_util::sync::CancellationToken::new();

        let bg_task = BackgroundTask {
            id: task_id.clone(),
            task_type: "shell".to_string(),
            command: Some(command.to_string()),
            status: BackgroundTaskStatus::Running,
            cancel_token: cancel_token.clone(),
            output: output_buf.clone(),
            created_at: chrono::Utc::now(),
        };

        ctx.app_state
            .write()
            .await
            .background_tasks
            .insert(task_id.clone(), bg_task);

        let cmd_owned = command.to_string();
        let app_state = ctx.app_state.clone();
        let task_id_clone = task_id.clone();

        tokio::spawn(async move {
            let result = Command::new("bash")
                .arg("-c")
                .arg(&cmd_owned)
                .output()
                .await;

            let mut state = app_state.write().await;
            if let Some(task) = state.background_tasks.get_mut(&task_id_clone) {
                match result {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let mut buf = task.output.lock().unwrap();
                        if !stdout.is_empty() {
                            buf.push_str(&stdout);
                        }
                        if !stderr.is_empty() {
                            if !buf.is_empty() {
                                buf.push('\n');
                            }
                            buf.push_str(&stderr);
                        }
                        task.status = if output.status.success() {
                            BackgroundTaskStatus::Completed
                        } else {
                            BackgroundTaskStatus::Failed
                        };
                    }
                    Err(e) => {
                        let mut buf = task.output.lock().unwrap();
                        buf.push_str(&format!("Error: {}", e));
                        task.status = BackgroundTaskStatus::Failed;
                    }
                }
            }
        });

        Ok(ToolResult {
            content: serde_json::json!({
                "task_id": task_id,
                "status": "running",
                "command": command,
            })
            .to_string(),
            is_error: false,
            content_blocks: None,
        })
    }
}

async fn run_command(command: &str) -> std::io::Result<(String, String, i32)> {
    let output = Command::new("bash")
        .arg("-c")
        .arg(command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok((stdout, stderr, exit_code))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        ToolUseContext::test(input)
    }

    #[tokio::test]
    async fn test_echo_command() {
        let tool = BashTool;
        let ctx = make_ctx(serde_json::json!({"command": "echo hello"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn test_failing_command() {
        let tool = BashTool;
        let ctx = make_ctx(serde_json::json!({"command": "exit 1"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_missing_command() {
        let tool = BashTool;
        let ctx = make_ctx(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_permission_metadata() {
        let tool = BashTool;
        let meta = tool.permission_metadata(&serde_json::json!({"command": "ls -la"}));
        assert_eq!(meta.command.as_deref(), Some("ls -la"));
    }

    #[test]
    fn test_schema() {
        let tool = BashTool;
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());
    }

    #[tokio::test]
    async fn test_background_returns_task_id() {
        let ctx = make_ctx(serde_json::json!({
            "command": "echo hello",
            "run_in_background": true
        }));
        let app_state = ctx.app_state.clone();
        let result = BashTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["status"], "running");
        assert!(parsed["task_id"].as_str().unwrap().starts_with("bg-"));

        // Verify task was registered in app_state
        let task_id = parsed["task_id"].as_str().unwrap();
        let state = app_state.read().await;
        assert!(state.background_tasks.contains_key(task_id));
    }
}
