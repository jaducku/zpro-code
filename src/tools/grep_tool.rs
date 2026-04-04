use async_trait::async_trait;
use tokio::process::Command;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "Search file contents using regular expressions."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (defaults to cwd)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files"
                },
                "output_mode": {
                    "type": "string",
                    "description": "Output mode: content, files_with_matches, or count"
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
                tool_name: "Grep".into(),
                message: "Missing 'pattern' parameter".into(),
            })?;

        let path = ctx
            .input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let output_mode = ctx
            .input
            .get("output_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("files_with_matches");

        let glob_pattern = ctx.input.get("glob").and_then(|v| v.as_str());

        // Use ripgrep if available, fall back to grep
        let mut cmd = Command::new("rg");
        cmd.arg(pattern).arg(path);

        match output_mode {
            "files_with_matches" => {
                cmd.arg("-l");
            }
            "count" => {
                cmd.arg("-c");
            }
            _ => {
                cmd.arg("-n");
            } // content mode: show line numbers
        }

        if let Some(glob) = glob_pattern {
            cmd.arg("--glob").arg(glob);
        }

        cmd.arg("--max-count").arg("250");

        let output = cmd.output().await;

        // If rg not found, try grep
        let output = match output {
            Ok(o) => o,
            Err(_) => {
                let mut cmd = Command::new("grep");
                cmd.arg("-r").arg(pattern).arg(path);
                match output_mode {
                    "files_with_matches" => {
                        cmd.arg("-l");
                    }
                    "count" => {
                        cmd.arg("-c");
                    }
                    _ => {
                        cmd.arg("-n");
                    }
                }
                cmd.output()
                    .await
                    .map_err(|e| crate::types::AtomError::Tool {
                        tool_name: "Grep".into(),
                        message: format!("Failed to execute grep: {}", e),
                    })?
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        if stdout.trim().is_empty() {
            Ok(ToolResult {
                content: format!("No matches found for pattern '{}'", pattern),
                is_error: false,
                content_blocks: None,
            })
        } else {
            Ok(ToolResult {
                content: stdout,
                is_error: false,
                content_blocks: None,
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
    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        ToolUseContext::test(input)
    }

    #[tokio::test]
    async fn test_grep_finds_pattern() {
        let tool = GrepTool;
        let ctx = make_ctx(
            serde_json::json!({"pattern": "atom-code", "path": "Cargo.toml", "output_mode": "content"}),
        );
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("atom-code"));
    }

    #[tokio::test]
    async fn test_grep_no_match() {
        let tool = GrepTool;
        let ctx =
            make_ctx(serde_json::json!({"pattern": "zzz_nonexistent_zzz", "path": "Cargo.toml"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.content.contains("No matches"));
    }
}
