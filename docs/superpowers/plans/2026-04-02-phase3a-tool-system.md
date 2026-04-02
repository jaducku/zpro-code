# Phase 3a: Tool System — Infrastructure + Core Tools

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement tool registry, tool execution orchestration, and 6 core tools needed for the query engine to function.

**Architecture:** Tool trait (from Phase 1) + tool registry in `src/tools.rs` + individual tool implementations in `src/tools/`. Tool orchestration in `src/services/tools/`.

**Tech Stack:** tokio (process spawning, fs), globset, ignore, grep (via process), serde

**Spec:** `docs/superpowers/specs/2026-04-02-ts-to-rust-migration-design.md` (Section 5)

---

### Task 1: Tool registry

**Files:**
- Create: `src/tools/mod.rs`
- Modify: `src/lib.rs`

The registry collects all available tools and provides lookup by name.

- [ ] **Step 1: Implement tool registry**

```rust
// src/tools/mod.rs
pub mod bash_tool;
pub mod file_read_tool;
pub mod file_edit_tool;
pub mod file_write_tool;
pub mod glob_tool;
pub mod grep_tool;

use crate::tool::Tool;

/// Collect all built-in tools
pub fn get_builtin_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(bash_tool::BashTool),
        Box::new(file_read_tool::FileReadTool),
        Box::new(file_edit_tool::FileEditTool),
        Box::new(file_write_tool::FileWriteTool),
        Box::new(glob_tool::GlobTool),
        Box::new(grep_tool::GrepTool),
    ]
}

/// Find a tool by name from a list
pub fn find_tool_by_name<'a>(tools: &'a [Box<dyn Tool>], name: &str) -> Option<&'a dyn Tool> {
    tools.iter().find(|t| t.name() == name).map(|t| t.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_tools() {
        let tools = get_builtin_tools();
        assert!(tools.len() >= 6);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"Bash"));
        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Edit"));
        assert!(names.contains(&"Write"));
        assert!(names.contains(&"Glob"));
        assert!(names.contains(&"Grep"));
    }

    #[test]
    fn test_find_tool_by_name() {
        let tools = get_builtin_tools();
        assert!(find_tool_by_name(&tools, "Bash").is_some());
        assert!(find_tool_by_name(&tools, "NonExistent").is_none());
    }
}
```

- [ ] **Step 2: Add `pub mod tools;` to `src/lib.rs`**

- [ ] **Step 3: Create placeholder files for each tool** (empty files so mod.rs compiles — we'll fill them in subsequent tasks)

Create these 6 files with minimal placeholder Tool impls that compile:

Each file follows this pattern (using BashTool as example):
```rust
// src/tools/bash_tool.rs
use async_trait::async_trait;
use crate::tool::{Tool, ToolUseContext, ToolResult, ValidationResult};
use crate::types::permissions::PermissionMeta;

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str { "Bash" }
    fn description(&self) -> &str { "Execute a bash command" }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command to execute" }
            },
            "required": ["command"]
        })
    }
    async fn execute(&self, _ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        todo!("Implement in next task")
    }
    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            command: input.get("command").and_then(|v| v.as_str()).map(String::from),
            file_path: None,
            url: None,
        }
    }
}
```

Do the same pattern for FileReadTool ("Read"), FileEditTool ("Edit"), FileWriteTool ("Write"), GlobTool ("Glob"), GrepTool ("Grep") — each with appropriate name, description, input_schema, and `todo!()` execute.

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p atom-code`
Expected: Compiles (tests will fail due to todo! but that's fine — don't run tests yet)

- [ ] **Step 5: Commit**

```bash
git add src/tools/ src/lib.rs
git commit -m "feat: add tool registry with 6 core tool stubs"
```

---

### Task 2: BashTool implementation

**Files:**
- Modify: `src/tools/bash_tool.rs`

- [ ] **Step 1: Implement BashTool**

```rust
// src/tools/bash_tool.rs
use async_trait::async_trait;
use tokio::process::Command;

use crate::tool::{Tool, ToolUseContext, ToolResult, ToolProgress};
use crate::types::permissions::PermissionMeta;

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str { "Bash" }

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
        let command = ctx.input.get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Bash".into(),
                message: "Missing 'command' parameter".into(),
            })?;

        let timeout_ms = ctx.input.get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000)
            .min(600_000);

        let _ = ctx.progress_tx.send(ToolProgress::Output(format!("$ {}\n", command)));

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
            command: input.get("command").and_then(|v| v.as_str()).map(String::from),
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
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p atom-code tools::bash_tool`
Expected: 5 tests pass

- [ ] **Step 3: Commit**

```bash
git add src/tools/bash_tool.rs
git commit -m "feat: implement BashTool with process spawning and timeout"
```

---

### Task 3: FileReadTool implementation

**Files:**
- Modify: `src/tools/file_read_tool.rs`

- [ ] **Step 1: Implement FileReadTool**

```rust
// src/tools/file_read_tool.rs
use async_trait::async_trait;
use tokio::fs;

use crate::tool::{Tool, ToolUseContext, ToolResult};
use crate::types::permissions::PermissionMeta;

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str { "Read" }

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
        let file_path = ctx.input.get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Read".into(),
                message: "Missing 'file_path' parameter".into(),
            })?;

        let offset = ctx.input.get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let limit = ctx.input.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(2000) as usize;

        match fs::read_to_string(file_path).await {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                let total_lines = lines.len();
                let end = (offset + limit).min(total_lines);
                let selected: Vec<String> = lines[offset.min(total_lines)..end]
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

                Ok(ToolResult { content: result, is_error: false })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Error reading file '{}': {}", file_path, e),
                is_error: true,
            }),
        }
    }

    fn is_read_only(&self) -> bool { true }

    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            file_path: input.get("file_path").and_then(|v| v.as_str()).map(String::from),
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
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p atom-code tools::file_read_tool`
Expected: 4 tests pass

- [ ] **Step 3: Commit**

```bash
git add src/tools/file_read_tool.rs
git commit -m "feat: implement FileReadTool with line numbers, offset, limit"
```

---

### Task 4: FileWriteTool implementation

**Files:**
- Modify: `src/tools/file_write_tool.rs`

- [ ] **Step 1: Implement FileWriteTool**

```rust
// src/tools/file_write_tool.rs
use async_trait::async_trait;
use std::path::Path;
use tokio::fs;

use crate::tool::{Tool, ToolUseContext, ToolResult};
use crate::types::permissions::PermissionMeta;

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str { "Write" }

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
        let file_path = ctx.input.get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Write".into(),
                message: "Missing 'file_path' parameter".into(),
            })?;

        let content = ctx.input.get("content")
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
                        content: format!("Failed to create directory '{}': {}", parent.display(), e),
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
            file_path: input.get("file_path").and_then(|v| v.as_str()).map(String::from),
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
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "line 1\nline 2\nline 3");
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
```

- [ ] **Step 2: Run tests, commit**

---

### Task 5: FileEditTool implementation

**Files:**
- Modify: `src/tools/file_edit_tool.rs`

- [ ] **Step 1: Implement FileEditTool**

```rust
// src/tools/file_edit_tool.rs
use async_trait::async_trait;
use tokio::fs;

use crate::tool::{Tool, ToolUseContext, ToolResult};
use crate::types::permissions::PermissionMeta;

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str { "Edit" }

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
        let file_path = ctx.input.get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Edit".into(),
                message: "Missing 'file_path' parameter".into(),
            })?;

        let old_string = ctx.input.get("old_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Edit".into(),
                message: "Missing 'old_string' parameter".into(),
            })?;

        let new_string = ctx.input.get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Edit".into(),
                message: "Missing 'new_string' parameter".into(),
            })?;

        let replace_all = ctx.input.get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Read file
        let content = match fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => return Ok(ToolResult {
                content: format!("Error reading file '{}': {}", file_path, e),
                is_error: true,
            }),
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
            file_path: input.get("file_path").and_then(|v| v.as_str()).map(String::from),
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
```

- [ ] **Step 2: Run tests, commit**

---

### Task 6: GlobTool and GrepTool

**Files:**
- Modify: `src/tools/glob_tool.rs`
- Modify: `src/tools/grep_tool.rs`

- [ ] **Step 1: Implement GlobTool**

```rust
// src/tools/glob_tool.rs
use async_trait::async_trait;
use tokio::process::Command;

use crate::tool::{Tool, ToolUseContext, ToolResult};
use crate::types::permissions::PermissionMeta;

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str { "Glob" }

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
        let pattern = ctx.input.get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Glob".into(),
                message: "Missing 'pattern' parameter".into(),
            })?;

        let path = ctx.input.get("path")
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

    fn is_read_only(&self) -> bool { true }

    fn permission_metadata(&self, _input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta { file_path: None, command: None, url: None }
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
```

- [ ] **Step 2: Implement GrepTool**

```rust
// src/tools/grep_tool.rs
use async_trait::async_trait;
use tokio::process::Command;

use crate::tool::{Tool, ToolUseContext, ToolResult};
use crate::types::permissions::PermissionMeta;

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "Grep" }

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
        let pattern = ctx.input.get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Grep".into(),
                message: "Missing 'pattern' parameter".into(),
            })?;

        let path = ctx.input.get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let output_mode = ctx.input.get("output_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("files_with_matches");

        let glob_pattern = ctx.input.get("glob")
            .and_then(|v| v.as_str());

        // Use ripgrep if available, fall back to grep
        let mut cmd = Command::new("rg");
        cmd.arg(pattern).arg(path);

        match output_mode {
            "files_with_matches" => { cmd.arg("-l"); }
            "count" => { cmd.arg("-c"); }
            _ => { cmd.arg("-n"); } // content mode: show line numbers
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
                    "files_with_matches" => { cmd.arg("-l"); }
                    "count" => { cmd.arg("-c"); }
                    _ => { cmd.arg("-n"); }
                }
                cmd.output().await.map_err(|e| crate::types::AtomError::Tool {
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
            })
        } else {
            Ok(ToolResult {
                content: stdout,
                is_error: false,
            })
        }
    }

    fn is_read_only(&self) -> bool { true }

    fn permission_metadata(&self, _input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta { file_path: None, command: None, url: None }
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
    async fn test_grep_finds_pattern() {
        let tool = GrepTool;
        let ctx = make_ctx(serde_json::json!({"pattern": "atom-code", "path": "Cargo.toml"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("atom-code"));
    }

    #[tokio::test]
    async fn test_grep_no_match() {
        let tool = GrepTool;
        let ctx = make_ctx(serde_json::json!({"pattern": "zzz_nonexistent_zzz", "path": "Cargo.toml"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.content.contains("No matches"));
    }
}
```

- [ ] **Step 3: Run all tool tests, commit**

Run: `cargo test -p atom-code tools`
Expected: All tool tests pass

```bash
git add src/tools/
git commit -m "feat: implement GlobTool and GrepTool"
```

---

### Task 7: Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 3: Commit if needed**

```bash
git add -A
git commit -m "chore: phase 3a complete — tool registry + 6 core tools"
```
