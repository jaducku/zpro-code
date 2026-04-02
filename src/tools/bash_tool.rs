use async_trait::async_trait;
use tokio::process::Command;

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
                })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Failed to execute command: {}", e),
                is_error: true,
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
    use tokio_util::sync::CancellationToken;

    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        ToolUseContext {
            input,
            abort_signal: CancellationToken::new(),
            progress_tx: tx,
        }
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
}
