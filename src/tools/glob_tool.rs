use async_trait::async_trait;
use tokio::process::Command;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in (defaults to cwd)"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let pattern = ctx
            .input
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Glob".into(),
                message: "Missing 'pattern' parameter".into(),
            })?;

        let path = ctx
            .input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        // Use find command for glob matching
        let output = Command::new("find")
            .arg(path)
            .arg("-name")
            .arg(pattern)
            .arg("-type")
            .arg("f")
            .output()
            .await
            .map_err(|e| crate::types::AtomError::Tool {
                tool_name: "Glob".into(),
                message: format!("Failed to execute find: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let files: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

        if files.is_empty() {
            Ok(ToolResult {
                content: format!("No files matched pattern '{}'", pattern),
                is_error: false,
            })
        } else {
            Ok(ToolResult {
                content: files.join("\n"),
                is_error: false,
            })
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn permission_metadata(&self, _input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            file_path: None,
            command: None,
            url: None,
        }
    }
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
    async fn test_glob_finds_files() {
        let tool = GlobTool;
        let ctx = make_ctx(serde_json::json!({"pattern": "Cargo.toml", "path": "."}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Cargo.toml"));
    }

    #[tokio::test]
    async fn test_glob_no_match() {
        let tool = GlobTool;
        let ctx = make_ctx(serde_json::json!({"pattern": "*.nonexistent_ext"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.content.contains("No files matched"));
    }
}
