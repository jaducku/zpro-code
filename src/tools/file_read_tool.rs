use async_trait::async_trait;
use tokio::fs;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        "Read a file from the filesystem. Returns the file content with line numbers."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "number",
                    "description": "Line number to start reading from (0-indexed)"
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of lines to read"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let file_path = ctx
            .input
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Read".into(),
                message: "Missing 'file_path' parameter".into(),
            })?;

        let offset = ctx
            .input
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let limit = ctx
            .input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(2000) as usize;

        match fs::read_to_string(file_path).await {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                let total_lines = lines.len();
                let start = offset.min(total_lines);
                let end = (offset + limit).min(total_lines);
                let selected: Vec<String> = lines[start..end]
                    .iter()
                    .enumerate()
                    .map(|(i, line)| format!("{}\t{}", offset + i + 1, line))
                    .collect();

                let mut result = selected.join("\n");
                if end < total_lines {
                    result.push_str(&format!(
                        "\n\n... ({} more lines, {} total)",
                        total_lines - end,
                        total_lines
                    ));
                }

                Ok(ToolResult {
                    content: result,
                    is_error: false,
                    content_blocks: None,
                })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Error reading file '{}': {}", file_path, e),
                is_error: true,
                content_blocks: None,
            }),
        }
    }

    fn is_read_only(&self) -> bool {
        true
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
    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        ToolUseContext::test(input)
    }

    #[tokio::test]
    async fn test_read_existing_file() {
        let tool = FileReadTool;
        let ctx = make_ctx(serde_json::json!({"file_path": "Cargo.toml"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("atom-code"));
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let tool = FileReadTool;
        let ctx = make_ctx(serde_json::json!({"file_path": "/nonexistent/file.txt"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_read_with_offset_limit() {
        let tool = FileReadTool;
        let ctx = make_ctx(serde_json::json!({"file_path": "Cargo.toml", "offset": 0, "limit": 3}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let lines: Vec<&str> = result.content.lines().collect();
        // 3 content lines + possibly a "... more lines" line
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_read_only() {
        assert!(FileReadTool.is_read_only());
    }
}
