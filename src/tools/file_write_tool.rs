use async_trait::async_trait;
use std::path::Path;
use tokio::fs;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "Write"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let file_path = ctx
            .input
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Write".into(),
                message: "Missing 'file_path' parameter".into(),
            })?;

        let content = ctx
            .input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Write".into(),
                message: "Missing 'content' parameter".into(),
            })?;

        // Create parent directories if needed
        if let Some(parent) = Path::new(file_path).parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent).await {
                    return Ok(ToolResult {
                        content: format!(
                            "Failed to create directory '{}': {}",
                            parent.display(),
                            e
                        ),
                        is_error: true,
                    });
                }
            }
        }

        match fs::write(file_path, content).await {
            Ok(()) => {
                let line_count = content.lines().count();
                Ok(ToolResult {
                    content: format!("Successfully wrote {} lines to {}", line_count, file_path),
                    is_error: false,
                })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Failed to write to '{}': {}", file_path, e),
                is_error: true,
            }),
        }
    }

    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            file_path: input
                .get("file_path")
                .and_then(|v| v.as_str())
                .map(String::from),
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
    async fn test_write_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let ctx = make_ctx(serde_json::json!({
            "file_path": path.to_str().unwrap(),
            "content": "line 1\nline 2\nline 3"
        }));
        let result = FileWriteTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("3 lines"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "line 1\nline 2\nline 3"
        );
    }

    #[tokio::test]
    async fn test_write_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/dir/test.txt");
        let ctx = make_ctx(serde_json::json!({
            "file_path": path.to_str().unwrap(),
            "content": "hello"
        }));
        let result = FileWriteTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_write_missing_params() {
        let ctx = make_ctx(serde_json::json!({"file_path": "/tmp/test"}));
        let result = FileWriteTool.execute(ctx).await;
        assert!(result.is_err());
    }
}
