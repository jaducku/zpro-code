# Batch 1: Core Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 5 essential tools (WebFetch, WebSearch, AskUserQuestion, Sleep, NotebookEdit) so the coding assistant can browse the web, ask users questions, pause, and edit notebooks.

**Architecture:** Each tool is a standalone Rust file implementing the `Tool` trait. SleepTool and NotebookEditTool are self-contained. WebFetchTool uses reqwest+readability+htmd. WebSearchTool calls Tavily API via reqwest. AskUserQuestionTool adds a new channel type (like PermissionAsk) plumbed through QueryContext → ToolUseContext → TUI event loop.

**Tech Stack:** Rust, reqwest, readability (0.3), htmd (0.5), serde_json, tokio, ratatui

**Spec:** `docs/superpowers/specs/2026-04-04-batch1-core-tools-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/tools/sleep_tool.rs` | **New** — SleepTool |
| `src/tools/notebook_edit_tool.rs` | **New** — NotebookEditTool |
| `src/tools/web_fetch_tool.rs` | **New** — WebFetchTool |
| `src/tools/web_search_tool.rs` | **New** — WebSearchTool |
| `src/tools/ask_user_question_tool.rs` | **New** — AskUserQuestionTool |
| `src/tools/mod.rs` | Register all 5 new tools |
| `src/tool.rs` | Add `tool_use_id`, `question_tx` to ToolUseContext |
| `src/query/types.rs` | Add `UserQuestionAsk`, `QuestionOption`, `UserQuestionReply`; add `question_tx` to QueryContext |
| `src/query/engine.rs` | Pass `tool_use_id`, `question_tx` in execute_tool() |
| `src/tui/app.rs` | Add `UserQuestionDialog`, `question_dialog` field |
| `src/tui/run.rs` | Create question channel, handle in event loop |
| `src/tui/ui.rs` | Render question dialog |
| `src/services/config/mod.rs` | Add `tavily_api_key` to AtomConfig |
| `Cargo.toml` | Add `readability`, `htmd` dependencies |

---

### Task 1: Add dependencies to Cargo.toml

**Files:**
- Modify: `Cargo.toml:23-61`

- [ ] **Step 1: Add readability and htmd to [dependencies]**

Add after the `regex = "1"` line:
```toml
readability = "0.3"
htmd = "0.5"
```

- [ ] **Step 2: Verify dependencies resolve**

Run: `cargo check 2>&1 | tail -5`
Expected: No new errors (existing warnings OK).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add readability and htmd for WebFetchTool"
```

---

### Task 2: Implement SleepTool

**Files:**
- Create: `src/tools/sleep_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create `src/tools/sleep_tool.rs`**

```rust
use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct SleepTool;

#[async_trait]
impl Tool for SleepTool {
    fn name(&self) -> &str {
        "Sleep"
    }

    fn description(&self) -> &str {
        "Pause execution for a specified duration. The user can interrupt at any time."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "duration_ms": {
                    "type": "integer",
                    "description": "Duration to sleep in milliseconds (max 60000)"
                }
            },
            "required": ["duration_ms"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let duration_ms = ctx.input.get("duration_ms")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Sleep".into(),
                message: "Missing required 'duration_ms' parameter".into(),
            })?;

        let duration_ms = duration_ms.min(60_000);
        let duration = std::time::Duration::from_millis(duration_ms);

        tokio::select! {
            _ = tokio::time::sleep(duration) => {
                Ok(ToolResult {
                    content: format!("Slept for {}ms", duration_ms),
                    is_error: false,
                })
            }
            _ = ctx.abort_signal.cancelled() => {
                Ok(ToolResult {
                    content: "Sleep interrupted".into(),
                    is_error: false,
                })
            }
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sleep_tool_name() {
        assert_eq!(SleepTool.name(), "Sleep");
    }

    #[test]
    fn test_sleep_tool_schema() {
        let schema = SleepTool.input_schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("duration_ms")));
    }

    #[tokio::test]
    async fn test_sleep_short_duration() {
        let ctx = ToolUseContext::test(serde_json::json!({ "duration_ms": 10 }));
        let result = SleepTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Slept for 10ms"));
    }

    #[tokio::test]
    async fn test_sleep_clamps_max() {
        let ctx = ToolUseContext::test(serde_json::json!({ "duration_ms": 999999 }));
        let result = SleepTool.execute(ctx).await.unwrap();
        assert!(result.content.contains("60000ms"));
    }

    #[tokio::test]
    async fn test_sleep_missing_param() {
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = SleepTool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Register in `src/tools/mod.rs`**

Add `pub mod sleep_tool;` and `Box::new(sleep_tool::SleepTool),` to `get_builtin_tools()`.

- [ ] **Step 3: Run tests**

Run: `cargo test sleep_tool 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/tools/sleep_tool.rs src/tools/mod.rs
git commit -m "feat: implement SleepTool"
```

---

### Task 3: Implement NotebookEditTool

**Files:**
- Create: `src/tools/notebook_edit_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create `src/tools/notebook_edit_tool.rs`**

```rust
use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

pub struct NotebookEditTool;

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
                "notebook_path": { "type": "string", "description": "Path to the .ipynb file" },
                "command": { "type": "string", "enum": ["insert", "replace", "delete"], "description": "Operation to perform" },
                "cell_index": { "type": "integer", "description": "0-based cell index" },
                "cell_type": { "type": "string", "enum": ["code", "markdown"], "description": "Cell type (for insert/replace)" },
                "source": { "type": "string", "description": "Cell content (for insert/replace)" }
            },
            "required": ["notebook_path", "command", "cell_index"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let path = ctx.input.get("notebook_path").and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing 'notebook_path'"))?;
        let command = ctx.input.get("command").and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing 'command'"))?;
        let cell_index = ctx.input.get("cell_index").and_then(|v| v.as_u64())
            .ok_or_else(|| tool_err("Missing 'cell_index'"))? as usize;

        let content = std::fs::read_to_string(path).map_err(|e| tool_err(&format!("Cannot read {}: {}", path, e)))?;
        let mut notebook: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| tool_err(&format!("Invalid JSON in {}: {}", path, e)))?;

        let cells = notebook.get_mut("cells")
            .and_then(|v| v.as_array_mut())
            .ok_or_else(|| tool_err("Notebook has no 'cells' array"))?;

        let summary = match command {
            "insert" => {
                let source = ctx.input.get("source").and_then(|v| v.as_str())
                    .ok_or_else(|| tool_err("'source' required for insert"))?;
                let cell_type = ctx.input.get("cell_type").and_then(|v| v.as_str()).unwrap_or("code");
                let cell = make_cell(cell_type, source);
                let idx = cell_index.min(cells.len());
                cells.insert(idx, cell);
                format!("Inserted {} cell at index {}", cell_type, idx)
            }
            "replace" => {
                if cell_index >= cells.len() {
                    return Ok(ToolResult { content: format!("cell_index {} out of bounds (notebook has {} cells)", cell_index, cells.len()), is_error: true });
                }
                let source = ctx.input.get("source").and_then(|v| v.as_str())
                    .ok_or_else(|| tool_err("'source' required for replace"))?;
                let cell_type = ctx.input.get("cell_type").and_then(|v| v.as_str()).unwrap_or("code");
                let source_lines = source_to_lines(source);
                cells[cell_index]["source"] = source_lines;
                cells[cell_index]["cell_type"] = serde_json::Value::String(cell_type.to_string());
                format!("Replaced cell at index {} with {} cell", cell_index, cell_type)
            }
            "delete" => {
                if cell_index >= cells.len() {
                    return Ok(ToolResult { content: format!("cell_index {} out of bounds (notebook has {} cells)", cell_index, cells.len()), is_error: true });
                }
                cells.remove(cell_index);
                format!("Deleted cell at index {}", cell_index)
            }
            _ => {
                return Ok(ToolResult { content: format!("Unknown command '{}'. Use insert, replace, or delete.", command), is_error: true });
            }
        };

        let output = serde_json::to_string_pretty(&notebook)
            .map_err(|e| tool_err(&format!("Failed to serialize: {}", e)))?;
        std::fs::write(path, output)
            .map_err(|e| tool_err(&format!("Failed to write {}: {}", path, e)))?;

        Ok(ToolResult { content: summary, is_error: false })
    }

    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            file_path: input.get("notebook_path").and_then(|v| v.as_str()).map(String::from),
            command: None,
            url: None,
        }
    }
}

fn tool_err(msg: &str) -> crate::types::AtomError {
    crate::types::AtomError::Tool { tool_name: "NotebookEdit".into(), message: msg.into() }
}

fn source_to_lines(source: &str) -> serde_json::Value {
    let lines: Vec<serde_json::Value> = source.split('\n')
        .enumerate()
        .map(|(i, line)| {
            let mut s = line.to_string();
            // Jupyter format: each line ends with \n except possibly the last
            if i < source.split('\n').count() - 1 || source.ends_with('\n') {
                s.push('\n');
            }
            serde_json::Value::String(s)
        })
        .collect();
    serde_json::Value::Array(lines)
}

fn make_cell(cell_type: &str, source: &str) -> serde_json::Value {
    let mut cell = serde_json::json!({
        "cell_type": cell_type,
        "source": source_to_lines(source),
        "metadata": {},
    });
    if cell_type == "code" {
        cell["outputs"] = serde_json::json!([]);
        cell["execution_count"] = serde_json::Value::Null;
    }
    cell
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(NotebookEditTool.name(), "NotebookEdit");
    }

    #[test]
    fn test_source_to_lines() {
        let lines = source_to_lines("print('hello')\nprint('world')");
        let arr = lines.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], "print('hello')\n");
        assert_eq!(arr[1], "print('world')");
    }

    #[test]
    fn test_make_code_cell() {
        let cell = make_cell("code", "x = 1");
        assert_eq!(cell["cell_type"], "code");
        assert!(cell["outputs"].is_array());
        assert!(cell["execution_count"].is_null());
    }

    #[test]
    fn test_make_markdown_cell() {
        let cell = make_cell("markdown", "# Title");
        assert_eq!(cell["cell_type"], "markdown");
        assert!(cell.get("outputs").is_none());
    }

    #[tokio::test]
    async fn test_insert_cell() {
        let dir = tempfile::TempDir::new().unwrap();
        let nb_path = dir.path().join("test.ipynb");
        let notebook = serde_json::json!({
            "cells": [],
            "metadata": {},
            "nbformat": 4,
            "nbformat_minor": 5
        });
        std::fs::write(&nb_path, serde_json::to_string(&notebook).unwrap()).unwrap();

        let ctx = ToolUseContext::test(serde_json::json!({
            "notebook_path": nb_path.to_str().unwrap(),
            "command": "insert",
            "cell_index": 0,
            "cell_type": "code",
            "source": "print('hello')"
        }));
        let result = NotebookEditTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Inserted"));

        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&nb_path).unwrap()).unwrap();
        assert_eq!(saved["cells"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_delete_out_of_bounds() {
        let dir = tempfile::TempDir::new().unwrap();
        let nb_path = dir.path().join("test.ipynb");
        std::fs::write(&nb_path, r#"{"cells":[],"metadata":{},"nbformat":4,"nbformat_minor":5}"#).unwrap();

        let ctx = ToolUseContext::test(serde_json::json!({
            "notebook_path": nb_path.to_str().unwrap(),
            "command": "delete",
            "cell_index": 5
        }));
        let result = NotebookEditTool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("out of bounds"));
    }
}
```

- [ ] **Step 2: Register in `src/tools/mod.rs`**

Add `pub mod notebook_edit_tool;` and `Box::new(notebook_edit_tool::NotebookEditTool),`.

- [ ] **Step 3: Run tests**

Run: `cargo test notebook_edit_tool 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/tools/notebook_edit_tool.rs src/tools/mod.rs
git commit -m "feat: implement NotebookEditTool"
```

---

### Task 4: Implement WebFetchTool

**Files:**
- Create: `src/tools/web_fetch_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create `src/tools/web_fetch_tool.rs`**

```rust
use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

const MAX_BODY_BYTES: usize = 5 * 1024 * 1024; // 5MB
const MAX_OUTPUT_CHARS: usize = 20_000;

pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "WebFetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return its content as markdown. Useful for reading documentation, articles, and web pages."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "The URL to fetch (http/https)" },
                "prompt": { "type": "string", "description": "Optional instruction for what to extract from the page" }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let url = ctx.input.get("url").and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'url' parameter"))?;
        let prompt = ctx.input.get("prompt").and_then(|v| v.as_str());

        // Validate URL scheme
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(ToolResult { content: "URL must start with http:// or https://".into(), is_error: true });
        }

        // Fetch with timeout
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("atom-code/0.1")
            .build()
            .map_err(|e| tool_err(&format!("Failed to create HTTP client: {}", e)))?;

        let response = tokio::select! {
            result = client.get(url).send() => {
                result.map_err(|e| tool_err(&format!("Fetch failed: {}", e)))?
            }
            _ = ctx.abort_signal.cancelled() => {
                return Err(crate::types::AtomError::Cancelled);
            }
        };

        let status = response.status();
        if !status.is_success() {
            return Ok(ToolResult { content: format!("HTTP {} for {}", status, url), is_error: true });
        }

        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // Read body with size limit
        let bytes = response.bytes().await
            .map_err(|e| tool_err(&format!("Failed to read response: {}", e)))?;
        if bytes.len() > MAX_BODY_BYTES {
            return Ok(ToolResult { content: format!("Response too large ({} bytes, max {})", bytes.len(), MAX_BODY_BYTES), is_error: true });
        }

        let body = String::from_utf8_lossy(&bytes).to_string();

        // Convert based on content type
        let markdown = if content_type.contains("text/html") || content_type.contains("application/xhtml") {
            html_to_markdown(&body, url)
        } else {
            // Plain text or other — return as-is
            body
        };

        // Truncate
        let markdown = if markdown.len() > MAX_OUTPUT_CHARS {
            format!("{}...\n\n[Truncated: {} chars total]", &markdown[..MAX_OUTPUT_CHARS], markdown.len())
        } else {
            markdown
        };

        // Prepend prompt if provided
        let content = match prompt {
            Some(p) => format!("Instructions: {}\n\n---\n\n{}", p, markdown),
            None => markdown,
        };

        Ok(ToolResult { content, is_error: false })
    }

    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            url: input.get("url").and_then(|v| v.as_str()).map(String::from),
            file_path: None,
            command: None,
        }
    }
}

fn html_to_markdown(html: &str, url: &str) -> String {
    // Try readability extraction first
    let mut cursor = std::io::Cursor::new(html.as_bytes());
    match readability::extractor::extract(&mut cursor, &url.parse().unwrap_or_else(|_| "http://example.com".parse().unwrap())) {
        Ok(product) => {
            // product.content is cleaned HTML — convert to markdown
            htmd::convert(&product.content).unwrap_or_else(|_| product.content)
        }
        Err(_) => {
            // Fallback: convert raw HTML to markdown
            htmd::convert(html).unwrap_or_else(|_| html.to_string())
        }
    }
}

fn tool_err(msg: &str) -> crate::types::AtomError {
    crate::types::AtomError::Tool { tool_name: "WebFetch".into(), message: msg.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(WebFetchTool.name(), "WebFetch");
    }

    #[test]
    fn test_schema() {
        let schema = WebFetchTool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("url")));
    }

    #[tokio::test]
    async fn test_invalid_url_scheme() {
        let ctx = ToolUseContext::test(serde_json::json!({ "url": "ftp://example.com" }));
        let result = WebFetchTool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("http"));
    }

    #[test]
    fn test_html_to_markdown_basic() {
        let md = html_to_markdown("<html><body><h1>Hello</h1><p>World</p></body></html>", "http://example.com");
        assert!(md.contains("Hello"));
        assert!(md.contains("World"));
    }

    #[test]
    fn test_permission_metadata() {
        let input = serde_json::json!({ "url": "https://docs.rs" });
        let meta = WebFetchTool.permission_metadata(&input);
        assert_eq!(meta.url, Some("https://docs.rs".into()));
    }
}
```

- [ ] **Step 2: Register in `src/tools/mod.rs`**

Add `pub mod web_fetch_tool;` and `Box::new(web_fetch_tool::WebFetchTool),`.

- [ ] **Step 3: Run tests**

Run: `cargo test web_fetch_tool 2>&1 | tail -10`
Expected: All tests pass.

Note: `readability::extractor::extract` may need `url::Url` type — check the actual API. If the crate uses `url::Url`, add `url = "2"` to Cargo.toml. If it uses a custom URL type, adapt accordingly. The implementer should read the readability crate docs and adjust the `html_to_markdown` function.

- [ ] **Step 4: Commit**

```bash
git add src/tools/web_fetch_tool.rs src/tools/mod.rs Cargo.toml Cargo.lock
git commit -m "feat: implement WebFetchTool with readability + htmd"
```

---

### Task 5: Add tavily_api_key to AtomConfig + implement WebSearchTool

**Files:**
- Modify: `src/services/config/mod.rs:6-30`
- Create: `src/tools/web_search_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Add `tavily_api_key` to `AtomConfig`**

In `src/services/config/mod.rs`, add after `permission_mode`:

```rust
    /// Tavily API key for web search
    #[serde(default)]
    pub tavily_api_key: Option<String>,
```

- [ ] **Step 2: Create `src/tools/web_search_tool.rs`**

```rust
use async_trait::async_trait;
use serde::Deserialize;

use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct WebSearchTool;

#[derive(Deserialize)]
struct TavilyResponse {
    results: Vec<TavilyResult>,
}

#[derive(Deserialize)]
struct TavilyResult {
    title: String,
    url: String,
    content: String,
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        "Search the web using Tavily and return relevant results with content."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search query" },
                "num_results": { "type": "integer", "description": "Number of results (default 5, max 10)" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let query = ctx.input.get("query").and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'query' parameter"))?;
        let num_results = ctx.input.get("num_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(10) as usize;

        // Load API key from config
        let config = crate::services::config::AtomConfig::load();
        let api_key = match config.tavily_api_key {
            Some(key) if !key.is_empty() => key,
            _ => {
                return Ok(ToolResult {
                    content: "Tavily API key not configured. Add \"tavily_api_key\": \"tvly-...\" to ~/.atom/config.json".into(),
                    is_error: true,
                });
            }
        };

        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "query": query,
            "max_results": num_results,
            "include_answer": false,
            "include_raw_content": false,
        });

        let response = tokio::select! {
            result = client
                .post("https://api.tavily.com/search")
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&body)
                .send() => {
                result.map_err(|e| tool_err(&format!("Search request failed: {}", e)))?
            }
            _ = ctx.abort_signal.cancelled() => {
                return Err(crate::types::AtomError::Cancelled);
            }
        };

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Ok(ToolResult {
                content: format!("Tavily API error (HTTP {}): {}", status, error_body),
                is_error: true,
            });
        }

        let tavily: TavilyResponse = response.json().await
            .map_err(|e| tool_err(&format!("Failed to parse Tavily response: {}", e)))?;

        if tavily.results.is_empty() {
            return Ok(ToolResult { content: format!("No results found for '{}'", query), is_error: false });
        }

        let formatted: String = tavily.results.iter()
            .map(|r| format!("### [{}]({})\n\n{}", r.title, r.url, r.content))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        Ok(ToolResult { content: formatted, is_error: false })
    }
}

fn tool_err(msg: &str) -> crate::types::AtomError {
    crate::types::AtomError::Tool { tool_name: "WebSearch".into(), message: msg.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(WebSearchTool.name(), "WebSearch");
    }

    #[test]
    fn test_schema() {
        let schema = WebSearchTool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("query")));
    }

    #[tokio::test]
    async fn test_missing_api_key() {
        let ctx = ToolUseContext::test(serde_json::json!({ "query": "rust async" }));
        let result = WebSearchTool.execute(ctx).await.unwrap();
        // In test env, no config.json → should get the "not configured" error
        assert!(result.is_error);
        assert!(result.content.contains("not configured") || result.content.contains("API key"));
    }

    #[tokio::test]
    async fn test_missing_query_param() {
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = WebSearchTool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Register in `src/tools/mod.rs`**

Add `pub mod web_search_tool;` and `Box::new(web_search_tool::WebSearchTool),`.

- [ ] **Step 4: Run tests**

Run: `cargo test web_search_tool 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/services/config/mod.rs src/tools/web_search_tool.rs src/tools/mod.rs
git commit -m "feat: implement WebSearchTool with Tavily API"
```

---

### Task 6: Add question channel plumbing (tool_use_id + question_tx)

**Files:**
- Modify: `src/tool.rs`
- Modify: `src/query/types.rs`
- Modify: `src/query/engine.rs`
- Modify: `src/tui/run.rs`
- Modify: `src/tui/app.rs`
- Modify: `src/tui/ui.rs`
- Modify: `src/entrypoints/print.rs`

This task adds the infrastructure for AskUserQuestionTool — the channel types, plumbing through QueryContext/ToolUseContext, and TUI rendering.

- [ ] **Step 1: Add types to `src/query/types.rs`**

Add after `PermissionReply`:

```rust
/// Sent to the TUI when a tool wants to ask the user a question
pub struct UserQuestionAsk {
    pub tool_use_id: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub reply_tx: tokio::sync::oneshot::Sender<UserQuestionReply>,
}

pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

pub enum UserQuestionReply {
    Answer(String),
    Dismissed,
}
```

Add `question_tx` to `QueryContext`:
```rust
    pub question_tx: mpsc::UnboundedSender<UserQuestionAsk>,
```

- [ ] **Step 2: Add `tool_use_id` and `question_tx` to `ToolUseContext` in `src/tool.rs`**

Add two fields:
```rust
    pub tool_use_id: String,
    pub question_tx: tokio::sync::mpsc::UnboundedSender<crate::query::types::UserQuestionAsk>,
```

Update `ToolUseContext::test()`:
```rust
    let (question_tx, _) = tokio::sync::mpsc::unbounded_channel();
    // ... in Self { ... }
    tool_use_id: String::new(),
    question_tx,
```

- [ ] **Step 3: Update `execute_tool()` in `src/query/engine.rs`**

Pass `tc.id` as `tool_use_id` and `ctx.question_tx` as `question_tx` when constructing `ToolUseContext`:
```rust
    tool_use_id: tc.id.clone(),
    question_tx: ctx.question_tx.clone(),
```

- [ ] **Step 4: Update `submit_query()` and `submit_query_skill()` in `src/tui/run.rs`**

Create question channel alongside permission channel:
```rust
    let (question_tx, question_rx) = mpsc::unbounded_channel();
    *user_question_rx = Some(question_rx);
```

Add `question_tx` to QueryContext:
```rust
    question_tx,
```

Add new `tokio::select!` branch in the TUI event loop for question_rx (similar to permission_rx branch at line ~220):
```rust
    ask = async {
        match user_question_rx.as_mut() {
            Some(rx) => rx.recv().await,
            None => std::future::pending().await,
        }
    } => {
        app.question_dialog = Some(UserQuestionDialog {
            question: ask.question,
            options: ask.options.into_iter().map(|o| (o.label, o.description)).collect(),
            selected: 0,
            text_input: String::new(),
            reply_tx: Some(ask.reply_tx),
        });
    }
```

- [ ] **Step 5: Add `UserQuestionDialog` to `src/tui/app.rs`**

```rust
pub struct UserQuestionDialog {
    pub question: String,
    pub options: Vec<(String, Option<String>)>,  // (label, description)
    pub selected: usize,
    pub text_input: String,
    pub reply_tx: Option<tokio::sync::oneshot::Sender<crate::query::types::UserQuestionReply>>,
}
```

Add field to `App`:
```rust
    pub question_dialog: Option<UserQuestionDialog>,
```

Initialize as `None` in `App::new()`.

Add key handler (similar to `handle_permission_key`):
```rust
    pub fn handle_question_key(&mut self, key: KeyEvent) -> bool {
        if let Some(dialog) = &mut self.question_dialog {
            if dialog.options.is_empty() {
                // Free text mode
                match key.code {
                    KeyCode::Enter => return true,
                    KeyCode::Char(c) => dialog.text_input.push(c),
                    KeyCode::Backspace => { dialog.text_input.pop(); },
                    KeyCode::Esc => return true, // dismiss
                    _ => {}
                }
            } else {
                // Option selection mode
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => dialog.selected = dialog.selected.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => {
                        dialog.selected = (dialog.selected + 1).min(dialog.options.len() - 1);
                    }
                    KeyCode::Enter => return true,
                    KeyCode::Esc => return true, // dismiss
                    _ => {}
                }
            }
        }
        false
    }
```

Add resolution handling in the TUI event loop key handler (after permission dialog):
```rust
    if app.question_dialog.is_some() {
        let resolved = app.handle_question_key(key);
        if resolved {
            let dialog = app.question_dialog.take().unwrap();
            if let Some(reply_tx) = dialog.reply_tx {
                let reply = if key.code == KeyCode::Esc {
                    UserQuestionReply::Dismissed
                } else if dialog.options.is_empty() {
                    UserQuestionReply::Answer(dialog.text_input)
                } else {
                    UserQuestionReply::Answer(dialog.options[dialog.selected].0.clone())
                };
                let _ = reply_tx.send(reply);
            }
        }
        continue;
    }
```

- [ ] **Step 6: Add question dialog rendering to `src/tui/ui.rs`**

After the permission dialog render (line ~34):
```rust
    if let Some(dialog) = &app.question_dialog {
        render_question_dialog(frame, dialog);
    }
```

Add the render function (similar to `render_permission_dialog`):
```rust
fn render_question_dialog(frame: &mut Frame, dialog: &super::app::UserQuestionDialog) {
    let area = centered_rect(60, 40, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Question ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![
        Line::from(Span::styled(&dialog.question, Style::default().fg(Color::White).bold())),
        Line::from(""),
    ];

    if dialog.options.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("> {}_", dialog.text_input),
            Style::default().fg(Color::Cyan),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Enter to submit, Esc to dismiss", Style::default().fg(Color::DarkGray))));
    } else {
        for (i, (label, desc)) in dialog.options.iter().enumerate() {
            let is_selected = i == dialog.selected;
            let prefix = if is_selected { "▸ " } else { "  " };
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(format!("{}{}", prefix, label), style)));
            if let Some(d) = desc {
                let desc_style = if is_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                lines.push(Line::from(Span::styled(format!("    {}", d), desc_style)));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("↑↓ to select, Enter to confirm, Esc to dismiss", Style::default().fg(Color::DarkGray))));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
```

- [ ] **Step 7: Update `src/entrypoints/print.rs`**

Add `question_tx` to QueryContext:
```rust
    let (question_tx, _question_rx) = mpsc::unbounded_channel();
    // in QueryContext:
    question_tx,
```

- [ ] **Step 8: Run `cargo check`**

Run: `cargo check 2>&1 | tail -10`
Expected: Clean compilation.

- [ ] **Step 9: Commit**

```bash
git add src/tool.rs src/query/types.rs src/query/engine.rs src/tui/run.rs src/tui/app.rs src/tui/ui.rs src/entrypoints/print.rs
git commit -m "feat: add UserQuestion channel plumbing for AskUserQuestionTool"
```

---

### Task 7: Implement AskUserQuestionTool

**Files:**
- Create: `src/tools/ask_user_question_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create `src/tools/ask_user_question_tool.rs`**

```rust
use async_trait::async_trait;

use crate::query::types::{QuestionOption, UserQuestionAsk, UserQuestionReply};
use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct AskUserQuestionTool;

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &str {
        "AskUserQuestion"
    }

    fn description(&self) -> &str {
        "Ask the user a question. Can present multiple-choice options or accept free-text input."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "The question to ask" },
                "options": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": { "type": "string" },
                            "description": { "type": "string" }
                        },
                        "required": ["label"]
                    },
                    "description": "Optional multiple-choice options"
                }
            },
            "required": ["question"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let question = ctx.input.get("question").and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "AskUserQuestion".into(),
                message: "Missing required 'question' parameter".into(),
            })?;

        let options: Vec<QuestionOption> = ctx.input.get("options")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().filter_map(|item| {
                    let label = item.get("label")?.as_str()?.to_string();
                    let description = item.get("description").and_then(|d| d.as_str()).map(String::from);
                    Some(QuestionOption { label, description })
                }).collect()
            })
            .unwrap_or_default();

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        let ask = UserQuestionAsk {
            tool_use_id: ctx.tool_use_id.clone(),
            question: question.to_string(),
            options,
            reply_tx,
        };

        ctx.question_tx.send(ask).map_err(|_| crate::types::AtomError::Tool {
            tool_name: "AskUserQuestion".into(),
            message: "Failed to send question to UI (channel closed)".into(),
        })?;

        // Wait for user response
        match reply_rx.await {
            Ok(UserQuestionReply::Answer(answer)) => {
                Ok(ToolResult { content: answer, is_error: false })
            }
            Ok(UserQuestionReply::Dismissed) => {
                Ok(ToolResult { content: "User dismissed the question".into(), is_error: true })
            }
            Err(_) => {
                Ok(ToolResult { content: "Question channel closed before user responded".into(), is_error: true })
            }
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(AskUserQuestionTool.name(), "AskUserQuestion");
    }

    #[test]
    fn test_schema() {
        let schema = AskUserQuestionTool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("question")));
    }

    #[tokio::test]
    async fn test_missing_question() {
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = AskUserQuestionTool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_channel_closed() {
        // ToolUseContext::test() drops the receiver, so send will fail
        let ctx = ToolUseContext::test(serde_json::json!({ "question": "Pick one" }));
        let result = AskUserQuestionTool.execute(ctx).await;
        // The send should fail since the receiver is dropped in test()
        assert!(result.is_err() || result.unwrap().is_error);
    }
}
```

- [ ] **Step 2: Register in `src/tools/mod.rs`**

Add `pub mod ask_user_question_tool;` and `Box::new(ask_user_question_tool::AskUserQuestionTool),`.

- [ ] **Step 3: Run tests**

Run: `cargo test ask_user_question 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/tools/ask_user_question_tool.rs src/tools/mod.rs
git commit -m "feat: implement AskUserQuestionTool with TUI dialog"
```

---

### Task 8: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All new tests pass. Pre-existing failures (services::skills) still present.

- [ ] **Step 2: Run clippy on changed files**

Run: `cargo clippy -- -D warnings 2>&1 | grep -E "sleep_tool|notebook_edit|web_fetch|web_search|ask_user|tool\.rs|engine\.rs" | head -20`
Expected: No warnings in our files.

- [ ] **Step 3: Verify all 14 tools are registered**

Run: `cargo test test_get_builtin_tools 2>&1 | tail -5`
Expected: Passes (check assertion count matches 14: 8 existing + Sleep + NotebookEdit + WebFetch + WebSearch + AskUserQuestion = 13... update assertion accordingly).

- [ ] **Step 4: Commit any fixes**
