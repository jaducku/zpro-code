use async_trait::async_trait;
use tokio::fs;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        "Perform exact string replacements in files."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default false)"
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let file_path = ctx
            .input
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Edit".into(),
                message: "Missing 'file_path' parameter".into(),
            })?;

        let old_string = ctx
            .input
            .get("old_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Edit".into(),
                message: "Missing 'old_string' parameter".into(),
            })?;

        let new_string = ctx
            .input
            .get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Edit".into(),
                message: "Missing 'new_string' parameter".into(),
            })?;

        let replace_all = ctx
            .input
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Read file
        let content = match fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult {
                    content: format!("Error reading file '{}': {}", file_path, e),
                    is_error: true,
                })
            }
        };

        // Check old_string exists
        if !content.contains(old_string) {
            return Ok(ToolResult {
                content: format!(
                    "Error: old_string not found in {}. Make sure it matches exactly.",
                    file_path
                ),
                is_error: true,
            });
        }

        // Check uniqueness (unless replace_all)
        if !replace_all {
            let count = content.matches(old_string).count();
            if count > 1 {
                return Ok(ToolResult {
                    content: format!(
                        "Error: old_string appears {} times in {}. Use replace_all or provide more context to make it unique.",
                        count, file_path
                    ),
                    is_error: true,
                });
            }
        }

        // Perform replacement
        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        // Write back
        match fs::write(file_path, &new_content).await {
            Ok(()) => Ok(ToolResult {
                content: format!("Successfully edited {}", file_path),
                is_error: false,
            }),
            Err(e) => Ok(ToolResult {
                content: format!("Error writing file '{}': {}", file_path, e),
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
    async fn test_edit_single_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "hello world").unwrap();

        let ctx = make_ctx(serde_json::json!({
            "file_path": path.to_str().unwrap(),
            "old_string": "hello",
            "new_string": "goodbye"
        }));
        let result = FileEditTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "goodbye world");
    }

    #[tokio::test]
    async fn test_edit_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "hello world").unwrap();

        let ctx = make_ctx(serde_json::json!({
            "file_path": path.to_str().unwrap(),
            "old_string": "xyz",
            "new_string": "abc"
        }));
        let result = FileEditTool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_edit_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "aaa bbb aaa").unwrap();

        let ctx = make_ctx(serde_json::json!({
            "file_path": path.to_str().unwrap(),
            "old_string": "aaa",
            "new_string": "ccc"
        }));
        let result = FileEditTool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("2 times"));
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "aaa bbb aaa").unwrap();

        let ctx = make_ctx(serde_json::json!({
            "file_path": path.to_str().unwrap(),
            "old_string": "aaa",
            "new_string": "ccc",
            "replace_all": true
        }));
        let result = FileEditTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "ccc bbb ccc");
    }
}
