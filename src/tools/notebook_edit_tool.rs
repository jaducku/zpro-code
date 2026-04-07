use async_trait::async_trait;
use tokio::fs;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

pub struct NotebookEditTool;

/// Convert a source string into Jupyter's line-array format.
/// Each line except the last gets a trailing \n appended.
pub fn source_to_lines(source: &str) -> serde_json::Value {
    if source.is_empty() {
        return serde_json::Value::Array(vec![]);
    }
    let lines: Vec<&str> = source.split('\n').collect();
    let n = lines.len();
    let values: Vec<serde_json::Value> = lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            if i < n - 1 {
                serde_json::Value::String(format!("{}\n", line))
            } else {
                serde_json::Value::String(line.to_string())
            }
        })
        .collect();
    serde_json::Value::Array(values)
}

/// Create a full Jupyter cell object.
/// Code cells include `outputs` and `execution_count`; markdown cells do not.
pub fn make_cell(cell_type: &str, source: &str) -> serde_json::Value {
    let source_lines = source_to_lines(source);
    if cell_type == "code" {
        serde_json::json!({
            "cell_type": "code",
            "metadata": {},
            "source": source_lines,
            "outputs": [],
            "execution_count": null
        })
    } else {
        serde_json::json!({
            "cell_type": cell_type,
            "metadata": {},
            "source": source_lines
        })
    }
}

#[async_trait]
impl Tool for NotebookEditTool {
    fn name(&self) -> &str {
        "NotebookEdit"
    }

    fn description(&self) -> &str {
        "Edit Jupyter notebook (.ipynb) cells. Supports insert, replace, and delete operations."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the .ipynb notebook file"
                },
                "command": {
                    "type": "string",
                    "enum": ["insert", "replace", "delete"],
                    "description": "The operation to perform on the notebook cells"
                },
                "cell_index": {
                    "type": "integer",
                    "description": "Zero-based index of the cell to operate on"
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown"],
                    "description": "The type of cell (required for insert; used in replace)"
                },
                "source": {
                    "type": "string",
                    "description": "The source content for the cell (required for insert and replace)"
                }
            },
            "required": ["notebook_path", "command", "cell_index"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let notebook_path = match ctx.input.get("notebook_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return Err(crate::types::AtomError::Tool {
                    tool_name: "NotebookEdit".into(),
                    message: "Missing 'notebook_path' parameter".into(),
                })
            }
        };

        let command = match ctx.input.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => {
                return Err(crate::types::AtomError::Tool {
                    tool_name: "NotebookEdit".into(),
                    message: "Missing 'command' parameter".into(),
                })
            }
        };

        let cell_index = match ctx.input.get("cell_index").and_then(|v| v.as_i64()) {
            Some(i) => i as usize,
            None => {
                return Err(crate::types::AtomError::Tool {
                    tool_name: "NotebookEdit".into(),
                    message: "Missing or invalid 'cell_index' parameter".into(),
                })
            }
        };

        // Read the notebook file
        let raw = match fs::read_to_string(notebook_path).await {
            Ok(s) => s,
            Err(e) => {
                return Ok(ToolResult {
                    content: format!("Failed to read notebook '{}': {}", notebook_path, e),
                    is_error: true,
                    content_blocks: None,
                })
            }
        };

        // Parse JSON
        let mut notebook: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                return Ok(ToolResult {
                    content: format!("Invalid JSON in notebook '{}': {}", notebook_path, e),
                    is_error: true,
                    content_blocks: None,
                })
            }
        };

        // Validate cells array exists
        let cells = match notebook.get_mut("cells").and_then(|v| v.as_array_mut()) {
            Some(c) => c,
            None => {
                return Ok(ToolResult {
                    content: format!(
                        "Notebook '{}' does not have a valid 'cells' array",
                        notebook_path
                    ),
                    is_error: true,
                    content_blocks: None,
                })
            }
        };

        match command {
            "insert" => {
                let cell_type = ctx
                    .input
                    .get("cell_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("code");
                let source = match ctx.input.get("source").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => {
                        return Ok(ToolResult {
                            content: "Missing 'source' parameter for insert command".into(),
                            is_error: true,
                            content_blocks: None,
                        })
                    }
                };
                let new_cell = make_cell(cell_type, source);
                let insert_at = cell_index.min(cells.len());
                cells.insert(insert_at, new_cell);
            }
            "replace" => {
                if cell_index >= cells.len() {
                    return Ok(ToolResult {
                        content: format!(
                            "Cell index {} is out of bounds (notebook has {} cells)",
                            cell_index,
                            cells.len()
                        ),
                        is_error: true,
                        content_blocks: None,
                    });
                }
                let source = match ctx.input.get("source").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => {
                        return Ok(ToolResult {
                            content: "Missing 'source' parameter for replace command".into(),
                            is_error: true,
                            content_blocks: None,
                        })
                    }
                };
                let cell = &mut cells[cell_index];
                // Update source
                cell["source"] = source_to_lines(source);
                // Update cell_type if provided
                if let Some(ct) = ctx.input.get("cell_type").and_then(|v| v.as_str()) {
                    cell["cell_type"] = serde_json::Value::String(ct.to_string());
                }
                // Preserve existing metadata, outputs, execution_count — do not overwrite
            }
            "delete" => {
                if cell_index >= cells.len() {
                    return Ok(ToolResult {
                        content: format!(
                            "Cell index {} is out of bounds (notebook has {} cells)",
                            cell_index,
                            cells.len()
                        ),
                        is_error: true,
                        content_blocks: None,
                    });
                }
                cells.remove(cell_index);
            }
            other => {
                return Ok(ToolResult {
                    content: format!(
                        "Unknown command '{}'. Valid commands: insert, replace, delete",
                        other
                    ),
                    is_error: true,
                    content_blocks: None,
                })
            }
        }

        // Write back
        let output = match serde_json::to_string_pretty(&notebook) {
            Ok(s) => s,
            Err(e) => {
                return Ok(ToolResult {
                    content: format!("Failed to serialize notebook: {}", e),
                    is_error: true,
                    content_blocks: None,
                })
            }
        };

        match fs::write(notebook_path, &output).await {
            Ok(()) => Ok(ToolResult {
                content: format!(
                    "Successfully applied '{}' to notebook '{}'",
                    command, notebook_path
                ),
                is_error: false,
                content_blocks: None,
            }),
            Err(e) => Ok(ToolResult {
                content: format!("Failed to write notebook '{}': {}", notebook_path, e),
                is_error: true,
                content_blocks: None,
            }),
        }
    }

    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            file_path: input
                .get("notebook_path")
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

    /// Minimal valid notebook JSON for tests
    fn empty_notebook() -> serde_json::Value {
        serde_json::json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": []
        })
    }

    fn write_notebook(path: &std::path::Path, nb: &serde_json::Value) {
        std::fs::write(path, serde_json::to_string_pretty(nb).unwrap()).unwrap();
    }

    fn read_notebook(path: &std::path::Path) -> serde_json::Value {
        let s = std::fs::read_to_string(path).unwrap();
        serde_json::from_str(&s).unwrap()
    }

    #[test]
    fn test_name() {
        assert_eq!(NotebookEditTool.name(), "NotebookEdit");
    }

    #[test]
    fn test_source_to_lines() {
        // Single line — no trailing \n
        let v = source_to_lines("hello");
        assert_eq!(v, serde_json::json!(["hello"]));

        // Two lines — first gets \n, second does not
        let v = source_to_lines("line1\nline2");
        assert_eq!(v, serde_json::json!(["line1\n", "line2"]));

        // Empty string
        let v = source_to_lines("");
        assert_eq!(v, serde_json::json!([]));
    }

    #[test]
    fn test_make_code_cell() {
        let cell = make_cell("code", "x = 1");
        assert_eq!(cell["cell_type"], "code");
        assert!(cell.get("outputs").is_some());
        assert!(cell.get("execution_count").is_some());
        assert_eq!(cell["outputs"], serde_json::json!([]));
        assert_eq!(cell["execution_count"], serde_json::Value::Null);
    }

    #[test]
    fn test_make_markdown_cell() {
        let cell = make_cell("markdown", "# Title");
        assert_eq!(cell["cell_type"], "markdown");
        assert!(cell.get("outputs").is_none());
        assert!(cell.get("execution_count").is_none());
    }

    #[tokio::test]
    async fn test_insert_into_empty_notebook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ipynb");
        write_notebook(&path, &empty_notebook());

        let ctx = make_ctx(serde_json::json!({
            "notebook_path": path.to_str().unwrap(),
            "command": "insert",
            "cell_index": 0,
            "cell_type": "code",
            "source": "print('hello')"
        }));
        let result = NotebookEditTool.execute(ctx).await.unwrap();
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            result.content
        );

        let nb = read_notebook(&path);
        let cells = nb["cells"].as_array().unwrap();
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0]["cell_type"], "code");
        assert_eq!(cells[0]["source"], serde_json::json!(["print('hello')"]));
        assert_eq!(cells[0]["outputs"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_delete_out_of_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ipynb");
        write_notebook(&path, &empty_notebook());

        let ctx = make_ctx(serde_json::json!({
            "notebook_path": path.to_str().unwrap(),
            "command": "delete",
            "cell_index": 5
        }));
        let result = NotebookEditTool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(
            result.content.contains("out of bounds"),
            "Expected out of bounds error, got: {}",
            result.content
        );
    }
}
