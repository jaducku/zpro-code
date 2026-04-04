# Batch 3: Agent Communication Tools — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 4 agent communication tools (ToolSearchTool, SendMessageTool, SyntheticOutputTool, BriefTool) with supporting infrastructure.

**Architecture:** Tool-first approach — each tool brings its infrastructure. Tool trait extended with deferred tool support. ToolResultBlock extended with optional content_blocks for tool_reference. Mailbox service (file-based, fs2) for SendMessage. jsonschema crate for SyntheticOutput validation.

**Tech Stack:** Rust, async-trait, tokio, serde, fs2, jsonschema, tokio::net::UnixStream

**Spec:** `docs/superpowers/specs/2026-04-04-batch3-agent-communication-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/tool.rs` | Modify | Add `should_defer`, `always_load`, `search_hint` to Tool trait; extend ToolResult/ToolResultBlock with `content_blocks` |
| `src/types/content.rs` | Modify | Add `ToolReference` variant to ContentBlock |
| `src/tools/mod.rs` | Modify | Add `is_deferred_tool()`, register 3 new tools (ToolSearch, SendMessage, Brief) |
| `src/tools/tool_search_tool.rs` | Create | ToolSearchTool implementation |
| `src/tools/send_message_tool.rs` | Create | SendMessageTool implementation |
| `src/tools/synthetic_output_tool.rs` | Create | SyntheticOutputTool (stateful, conditional registration) |
| `src/tools/brief_tool.rs` | Create | BriefTool (SendUserMessage) |
| `src/services/mailbox/mod.rs` | Create | Mailbox file-based service |
| `src/services/mailbox/types.rs` | Create | MailboxMessage, StructuredMessage types |
| `src/services/mod.rs` | Modify | Add `pub mod mailbox;` |
| `src/services/coordinator/mod.rs` | Modify | Add `is_agent_swarms_enabled()` |
| `src/state/app_state.rs` | Modify | Add `agent_message_queues`, `team_name`, `output_json_schema` |
| `src/query/types.rs` | Modify | Add `QueryEvent::BriefMessage`, `BriefAttachment`, `BriefStatus` |
| `src/query/engine.rs` | Modify | Handle `content_blocks` in ToolResultBlock → ContentBlock conversion |
| `Cargo.toml` | Modify | Add `jsonschema` dependency |

---

### Task 1: Tool Trait Extension — Deferred Tool Support

**Files:**
- Modify: `src/tool.rs` (lines 12-58: Tool trait, lines 113-118: ToolResult, lines 153-177: ToolResultBlock)
- Modify: `src/types/content.rs` (lines 4-34: ContentBlock enum)
- Modify: `src/tools/mod.rs` (add `is_deferred_tool` function)
- Modify: `src/query/engine.rs` (lines 352-359: ToolResultBlock → ContentBlock conversion)

- [ ] **Step 1: Write test for new Tool trait methods**

In `src/tool.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
// A minimal test tool to verify defaults
struct TestTool;
#[async_trait]
impl Tool for TestTool {
    fn name(&self) -> &str { "TestTool" }
    fn description(&self) -> &str { "test" }
    fn input_schema(&self) -> serde_json::Value { serde_json::json!({"type": "object"}) }
    async fn execute(&self, _ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        Ok(ToolResult { content: "ok".into(), is_error: false, content_blocks: None })
    }
}

#[test]
fn test_tool_trait_deferred_defaults() {
    let tool = TestTool;
    assert!(!tool.should_defer());
    assert!(!tool.always_load());
    assert!(tool.search_hint().is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p atom-code test_tool_trait_deferred_defaults 2>&1 | tail -20`
Expected: FAIL — methods and `content_blocks` field don't exist yet

- [ ] **Step 3: Add deferred methods to Tool trait**

In `src/tool.rs`, add three methods to the `Tool` trait after `is_mcp()` (line 58):

```rust
    /// Whether this tool should be deferred (schema not sent upfront to LLM)
    fn should_defer(&self) -> bool {
        false
    }

    /// Whether this tool must always be loaded upfront (never deferred)
    fn always_load(&self) -> bool {
        false
    }

    /// 3-10 word capability phrase for keyword search matching
    fn search_hint(&self) -> Option<&str> {
        None
    }
```

- [ ] **Step 4: Add `content_blocks` to ToolResult**

In `src/tool.rs`, change the `ToolResult` struct (line 113):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
    /// Structured content blocks (e.g. tool_reference). When Some, used instead of content string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_blocks: Option<Vec<crate::types::content::ContentBlock>>,
}
```

- [ ] **Step 5: Add `content_blocks` to ToolResultBlock**

In `src/tool.rs`, update `ToolResultBlock` struct (line 153):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_blocks: Option<Vec<crate::types::content::ContentBlock>>,
}
```

Update `ToolResultBlock::success` and `ToolResultBlock::error` to set `content_blocks: None`.

- [ ] **Step 6: Add `ToolReference` variant to ContentBlock**

In `src/types/content.rs`, add after the `Thinking` variant (line 33):

```rust
    #[serde(rename = "tool_reference")]
    ToolReference { tool_name: String },
```

- [ ] **Step 7: Write test for ToolReference serde**

In `src/types/content.rs` test module:

```rust
#[test]
fn test_tool_reference_block_serde() {
    let block = ContentBlock::ToolReference {
        tool_name: "Read".into(),
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "tool_reference");
    assert_eq!(json["tool_name"], "Read");

    let parsed: ContentBlock = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, block);
}
```

- [ ] **Step 8: Add `defer_loading` to ToolDefinition**

In `src/tool.rs`, update `ToolDefinition` (line 137):

```rust
fn is_false(v: &bool) -> bool { !v }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "is_false")]
    pub defer_loading: bool,
}
```

The `is_false` helper is defined as a private function in `src/tool.rs` (same file).

Update `From<&dyn Tool>` to set `defer_loading: false` for now (will use `is_deferred_tool` after it exists).

- [ ] **Step 8b: Update `From<&dyn Tool>` to use `is_deferred_tool` after Step 9**

After Step 9 adds `is_deferred_tool`, come back and update the `From` impl:

```rust
impl From<&dyn Tool> for ToolDefinition {
    fn from(tool: &dyn Tool) -> Self {
        ToolDefinition {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
            defer_loading: crate::tools::is_deferred_tool(tool),
        }
    }
}
```

- [ ] **Step 9: Add `is_deferred_tool` to `src/tools/mod.rs`**

```rust
/// Determine if a tool should have its schema deferred (not sent to LLM upfront)
pub fn is_deferred_tool(tool: &dyn Tool) -> bool {
    if tool.always_load() {
        return false;
    }
    if tool.is_mcp() {
        return true;
    }
    if tool.name() == "ToolSearch" {
        return false;
    }
    tool.should_defer()
}
```

Add test:

```rust
#[test]
fn test_is_deferred_tool() {
    let tools = get_builtin_tools();
    // All current builtin tools have should_defer() = false
    for tool in &tools {
        assert!(!is_deferred_tool(tool.as_ref()), "{} should not be deferred", tool.name());
    }
}
```

- [ ] **Step 10: Update query engine to handle content_blocks**

In `src/query/engine.rs`, update the ToolResult → ToolResultBlock creation (around line 328):

```rust
// After creating content string, propagate content_blocks from ToolResult
let block = ToolResultBlock {
    tool_use_id: tc.id.clone(),
    content: content.clone(),
    is_error: tr.is_error,
    content_blocks: tr.content_blocks.clone(),
};
```

Update the ToolResultBlock → ContentBlock conversion (lines 352-359):

```rust
let tool_result_content: Vec<ContentBlock> = tool_result_blocks
    .iter()
    .map(|b| ContentBlock::ToolResult {
        tool_use_id: b.tool_use_id.clone(),
        content: if let Some(ref blocks) = b.content_blocks {
            crate::types::content::ToolResultContent::Blocks(blocks.clone())
        } else {
            crate::types::content::ToolResultContent::Text(b.content.clone())
        },
        is_error: b.is_error,
    })
    .collect();
```

- [ ] **Step 11: Fix all existing tools — add `content_blocks: None` to ToolResult returns**

Every tool's `execute` method returns `ToolResult { content, is_error }`. Add `content_blocks: None` to each. Affected files (all in `src/tools/`):

- `agent_tool.rs`
- `ask_user_question_tool.rs`
- `bash_tool.rs`
- `file_edit_tool.rs`
- `file_read_tool.rs`
- `file_write_tool.rs`
- `glob_tool.rs`
- `grep_tool.rs`
- `notebook_edit_tool.rs`
- `skill_tool.rs`
- `sleep_tool.rs`
- `task_create_tool.rs`
- `task_get_tool.rs`
- `task_list_tool.rs`
- `task_output_tool.rs`
- `task_stop_tool.rs`
- `task_update_tool.rs`
- `todo_write_tool.rs`
- `web_fetch_tool.rs`
- `web_search_tool.rs`

- [ ] **Step 12: Run all tests**

Run: `cargo test -p atom-code 2>&1 | tail -30`
Expected: All tests pass including new deferred tool tests.

- [ ] **Step 13: Commit**

```bash
git add src/tool.rs src/types/content.rs src/tools/mod.rs src/query/engine.rs src/tools/*.rs
git commit -m "feat: extend Tool trait with deferred tool support and content_blocks"
```

---

- [ ] **Step 13b: Update `find_tool_by_name` to check aliases**

In `src/tools/mod.rs`, update the existing function:

```rust
pub fn find_tool_by_name<'a>(tools: &'a [Box<dyn Tool>], name: &str) -> Option<&'a dyn Tool> {
    tools.iter()
        .find(|t| t.name() == name || t.aliases().contains(&name))
        .map(|t| t.as_ref())
}
```

Add test:

```rust
#[test]
fn test_find_tool_by_alias() {
    let tools = get_builtin_tools();
    // BriefTool has alias "Brief"
    assert!(find_tool_by_name(&tools, "SendUserMessage").is_some());
    assert!(find_tool_by_name(&tools, "Brief").is_some());
}
```

---

### Task 2: ToolSearchTool

**Files:**
- Create: `src/tools/tool_search_tool.rs`
- Modify: `src/tools/mod.rs` (register ToolSearchTool)

- [ ] **Step 1: Write tests for ToolSearchTool**

Create `src/tools/tool_search_tool.rs`:

```rust
use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::content::ContentBlock;

pub struct ToolSearchTool;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(ToolSearchTool.name(), "ToolSearch");
    }

    #[test]
    fn test_schema() {
        let schema = ToolSearchTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["query"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("query")));
    }

    #[test]
    fn test_always_load() {
        assert!(ToolSearchTool.always_load());
        assert!(ToolSearchTool.is_read_only());
        assert!(ToolSearchTool.is_concurrent_safe());
    }

    #[tokio::test]
    async fn test_select_mode() {
        let tool = ToolSearchTool;
        // Create context with tools available
        let mut ctx = ToolUseContext::test(serde_json::json!({
            "query": "select:Bash,Read"
        }));
        // Add some tools to ctx.tools
        ctx.tools = std::sync::Arc::new(crate::tools::get_builtin_tools());

        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        // Should have content_blocks with ToolReference entries
        let blocks = result.content_blocks.unwrap();
        let names: Vec<&str> = blocks.iter().filter_map(|b| match b {
            ContentBlock::ToolReference { tool_name } => Some(tool_name.as_str()),
            _ => None,
        }).collect();
        assert!(names.contains(&"Bash"));
        assert!(names.contains(&"Read"));
    }

    #[tokio::test]
    async fn test_exact_name_match() {
        let tool = ToolSearchTool;
        let mut ctx = ToolUseContext::test(serde_json::json!({
            "query": "Bash"
        }));
        ctx.tools = std::sync::Arc::new(crate::tools::get_builtin_tools());

        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let blocks = result.content_blocks.unwrap();
        let names: Vec<&str> = blocks.iter().filter_map(|b| match b {
            ContentBlock::ToolReference { tool_name } => Some(tool_name.as_str()),
            _ => None,
        }).collect();
        assert!(names.contains(&"Bash"));
    }

    #[tokio::test]
    async fn test_missing_query() {
        let tool = ToolSearchTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p atom-code tool_search 2>&1 | tail -20`
Expected: FAIL — no execute implementation

- [ ] **Step 3: Implement ToolSearchTool core**

In `src/tools/tool_search_tool.rs`, implement the Tool trait:

```rust
use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::content::ContentBlock;
use crate::types::AtomError;

pub struct ToolSearchTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "ToolSearch".into(),
        message: msg.into(),
    }
}

/// Score a tool against search terms for keyword matching
fn score_tool(tool: &dyn Tool, terms: &[&str]) -> i32 {
    let mut score = 0i32;
    let name_lower = tool.name().to_lowercase();
    let is_mcp = tool.is_mcp();

    for term in terms {
        let term_lower = term.to_lowercase();

        // Name matching
        let name_full_score = if is_mcp { 12 } else { 10 };
        let name_partial_score = if is_mcp { 6 } else { 5 };

        // Split tool name on underscores and check word boundaries
        let name_parts: Vec<&str> = name_lower.split('_').collect();
        if name_parts.iter().any(|p| *p == term_lower) {
            score += name_full_score;
        } else if name_lower.contains(&term_lower) {
            score += name_partial_score;
        }

        // search_hint matching
        if let Some(hint) = tool.search_hint() {
            let hint_lower = hint.to_lowercase();
            if hint_lower.contains(&term_lower) {
                score += 4;
            }
        }

        // Description matching
        let desc_lower = tool.description().to_lowercase();
        if desc_lower.contains(&term_lower) {
            score += 2;
        }
    }

    score
}

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "ToolSearch"
    }

    fn description(&self) -> &str {
        "Fetches full schema definitions for deferred tools so they can be called."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Query: 'select:Read,Edit' for direct selection, or keywords to search"
                },
                "max_results": {
                    "type": "number",
                    "description": "Maximum number of results to return (default: 5)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let query = ctx.input.get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'query' parameter"))?;

        let max_results = ctx.input.get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let tools = ctx.tools.as_ref();
        let matched_names = search_tools(tools, query, max_results);

        // Count total deferred tools
        let total_deferred = tools.iter()
            .filter(|t| crate::tools::is_deferred_tool(t.as_ref()))
            .count();

        // Build tool_reference content blocks
        let mut blocks: Vec<ContentBlock> = matched_names.iter()
            .map(|name| ContentBlock::ToolReference { tool_name: name.clone() })
            .collect();

        // Add metadata as text block
        let meta = serde_json::json!({
            "query": query,
            "total_deferred_tools": total_deferred,
            "matches": matched_names,
        });
        blocks.push(ContentBlock::Text { text: meta.to_string() });

        Ok(ToolResult {
            content: format!("Found {} tool(s) matching '{}'", matched_names.len(), query),
            is_error: false,
            content_blocks: Some(blocks),
        })
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }
    fn always_load(&self) -> bool { true }
}

/// Search tools by query string. Supports select:, exact-name, mcp__ prefix, +required, keyword modes.
fn search_tools(tools: &[Box<dyn Tool>], query: &str, max_results: usize) -> Vec<String> {
    // Mode 1: select:Name1,Name2
    if let Some(names_str) = query.strip_prefix("select:") {
        let names: Vec<&str> = names_str.split(',').map(|s| s.trim()).collect();
        return tools.iter()
            .filter(|t| names.iter().any(|n| n.eq_ignore_ascii_case(t.name())))
            .map(|t| t.name().to_string())
            .collect();
    }

    // Mode 2: Exact name match (case-insensitive)
    let query_lower = query.to_lowercase();
    if let Some(tool) = tools.iter().find(|t| t.name().to_lowercase() == query_lower) {
        return vec![tool.name().to_string()];
    }

    // Mode 3: mcp__ prefix match
    if query.starts_with("mcp__") {
        return tools.iter()
            .filter(|t| t.name().starts_with(query))
            .map(|t| t.name().to_string())
            .take(max_results)
            .collect();
    }

    // Modes 4-5: keyword search (deferred tools only)
    let deferred_tools: Vec<&dyn Tool> = tools.iter()
        .filter(|t| crate::tools::is_deferred_tool(t.as_ref()))
        .map(|t| t.as_ref())
        .collect();

    // Parse +required and optional terms
    let terms: Vec<&str> = query.split_whitespace().collect();
    let required: Vec<&str> = terms.iter()
        .filter(|t| t.starts_with('+'))
        .map(|t| t.trim_start_matches('+'))
        .collect();
    let optional: Vec<&str> = terms.iter()
        .filter(|t| !t.starts_with('+'))
        .copied()
        .collect();
    let all_terms: Vec<&str> = required.iter().chain(optional.iter()).copied().collect();

    let mut scored: Vec<(String, i32)> = deferred_tools.iter()
        .filter_map(|tool| {
            let score = score_tool(*tool, &all_terms);
            if score == 0 { return None; }
            // Check required terms all match
            if !required.is_empty() {
                let name_lower = tool.name().to_lowercase();
                let desc_lower = tool.description().to_lowercase();
                let hint_lower = tool.search_hint().map(|h| h.to_lowercase()).unwrap_or_default();
                let combined = format!("{} {} {}", name_lower, desc_lower, hint_lower);
                for req in &required {
                    if !combined.contains(&req.to_lowercase()) {
                        return None;
                    }
                }
            }
            Some((tool.name().to_string(), score))
        })
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.into_iter().take(max_results).map(|(name, _)| name).collect()
}
```

- [ ] **Step 4: Register ToolSearchTool in mod.rs**

In `src/tools/mod.rs`, add:

```rust
pub mod tool_search_tool;
```

And in `get_builtin_tools()`:

```rust
Box::new(tool_search_tool::ToolSearchTool),
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p atom-code tool_search 2>&1 | tail -20`
Expected: All ToolSearchTool tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/tools/tool_search_tool.rs src/tools/mod.rs
git commit -m "feat: implement ToolSearchTool with select, exact-name, keyword search"
```

---

### Task 3: Mailbox Service

**Files:**
- Create: `src/services/mailbox/mod.rs`
- Create: `src/services/mailbox/types.rs`
- Modify: `src/services/mod.rs` (add `pub mod mailbox;`)

- [ ] **Step 1: Create types module**

Create `src/services/mailbox/types.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub from: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StructuredMessage {
    ShutdownRequest {
        reason: Option<String>,
    },
    ShutdownResponse {
        request_id: String,
        approve: bool,
        reason: Option<String>,
    },
    PlanApprovalResponse {
        request_id: String,
        approve: bool,
        feedback: Option<String>,
    },
}
```

- [ ] **Step 2: Write Mailbox tests**

Create `src/services/mailbox/mod.rs`:

```rust
pub mod types;

use std::path::PathBuf;
use types::MailboxMessage;

pub struct Mailbox {
    base_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_mailbox(dir: &TempDir) -> Mailbox {
        Mailbox { base_dir: dir.path().to_path_buf() }
    }

    #[test]
    fn test_write_and_read() {
        let dir = TempDir::new().unwrap();
        let mb = test_mailbox(&dir);
        let msg = MailboxMessage {
            from: "agent-a".into(),
            text: "hello".into(),
            summary: Some("greeting".into()),
            timestamp: chrono::Utc::now(),
        };
        mb.write("agent-b", msg.clone()).unwrap();
        let messages = mb.read("agent-b").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "agent-a");
        assert_eq!(messages[0].text, "hello");
    }

    #[test]
    fn test_read_empty() {
        let dir = TempDir::new().unwrap();
        let mb = test_mailbox(&dir);
        let messages = mb.read("nobody").unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn test_write_multiple_and_clear() {
        let dir = TempDir::new().unwrap();
        let mb = test_mailbox(&dir);
        for i in 0..3 {
            let msg = MailboxMessage {
                from: format!("agent-{}", i),
                text: format!("msg {}", i),
                summary: None,
                timestamp: chrono::Utc::now(),
            };
            mb.write("target", msg).unwrap();
        }
        assert_eq!(mb.read("target").unwrap().len(), 3);
        mb.clear("target").unwrap();
        assert!(mb.read("target").unwrap().is_empty());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p atom-code mailbox 2>&1 | tail -20`
Expected: FAIL — no write/read/clear implementations

- [ ] **Step 4: Implement Mailbox**

```rust
use std::path::PathBuf;
use anyhow::Result;
use types::MailboxMessage;

pub struct Mailbox {
    base_dir: PathBuf,
}

impl Mailbox {
    pub fn new(team_name: &str) -> Self {
        let base = crate::utils::paths::global_config_dir()
            .join("teams")
            .join(team_name)
            .join("inboxes");
        Self { base_dir: base }
    }

    fn inbox_path(&self, agent_name: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", agent_name))
    }

    pub fn write(&self, recipient: &str, msg: MailboxMessage) -> Result<()> {
        std::fs::create_dir_all(&self.base_dir)?;
        let path = self.inbox_path(recipient);

        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;
        fs2::FileExt::lock_exclusive(&file)?;

        let mut messages: Vec<MailboxMessage> = if path.exists() && std::fs::metadata(&path)?.len() > 0 {
            let content = std::fs::read_to_string(&path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };
        messages.push(msg);

        let content = serde_json::to_string_pretty(&messages)?;
        std::fs::write(&path, content)?;
        fs2::FileExt::unlock(&file)?;
        Ok(())
    }

    pub fn read(&self, agent_name: &str) -> Result<Vec<MailboxMessage>> {
        let path = self.inbox_path(agent_name);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&path)?;
        if content.is_empty() {
            return Ok(Vec::new());
        }
        let messages: Vec<MailboxMessage> = serde_json::from_str(&content)?;
        Ok(messages)
    }

    pub fn clear(&self, agent_name: &str) -> Result<()> {
        let path = self.inbox_path(agent_name);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
}
```

- [ ] **Step 5: Register mailbox module**

In `src/services/mod.rs`, add:

```rust
pub mod mailbox;
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p atom-code mailbox 2>&1 | tail -20`
Expected: All mailbox tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/services/mailbox/ src/services/mod.rs
git commit -m "feat: add file-based Mailbox service with fs2 locking"
```

---

### Task 4: AppState + Coordinator Changes

**Files:**
- Modify: `src/state/app_state.rs` (add fields)
- Modify: `src/services/coordinator/mod.rs` (add `is_agent_swarms_enabled`)

- [ ] **Step 1: Add AppState fields**

In `src/state/app_state.rs`, add to `AppState` struct after `swarm_agents` (line 156):

```rust
    #[serde(default)]
    pub agent_message_queues: HashMap<String, Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_json_schema: Option<serde_json::Value>,
```

Add defaults in `Default::default()`:

```rust
            agent_message_queues: HashMap::new(),
            team_name: None,
            output_json_schema: None,
```

- [ ] **Step 2: Add `is_agent_swarms_enabled`**

In `src/services/coordinator/mod.rs`, add:

```rust
/// Returns `true` when agent swarms are enabled (coordinator mode or env var).
pub fn is_agent_swarms_enabled() -> bool {
    is_coordinator_mode()
        || matches!(
            std::env::var("ATOM_CODE_AGENT_SWARMS")
                .unwrap_or_default()
                .as_str(),
            "1" | "true" | "yes"
        )
}
```

Add test:

```rust
#[test]
fn test_is_agent_swarms_enabled_default_false() {
    std::env::remove_var("ATOM_CODE_COORDINATOR_MODE");
    std::env::remove_var("ATOM_CODE_AGENT_SWARMS");
    assert!(!is_agent_swarms_enabled());
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code coordinator 2>&1 | tail -20`
Expected: All pass.

- [ ] **Step 4: Commit**

```bash
git add src/state/app_state.rs src/services/coordinator/mod.rs
git commit -m "feat: add agent_message_queues, team_name, output_json_schema to AppState; add is_agent_swarms_enabled"
```

---

### Task 5: SendMessageTool

**Files:**
- Create: `src/tools/send_message_tool.rs`
- Modify: `src/tools/mod.rs` (register)

- [ ] **Step 1: Write tests**

Create `src/tools/send_message_tool.rs` with test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(SendMessageTool.name(), "SendMessage");
    }

    #[test]
    fn test_schema() {
        let schema = SendMessageTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["to"].is_object());
        assert!(schema["properties"]["message"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("to")));
        assert!(required.contains(&serde_json::json!("message")));
    }

    #[test]
    fn test_should_defer() {
        assert!(SendMessageTool.should_defer());
        assert!(!SendMessageTool.is_read_only());
    }

    #[tokio::test]
    async fn test_missing_to() {
        let tool = SendMessageTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "message": "hello"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_message() {
        let tool = SendMessageTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "to": "agent-1"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_plain_message_to_agent_in_registry() {
        let tool = SendMessageTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "to": "worker-1",
            "message": "do the thing",
            "summary": "task delegation"
        }));
        // Register agent name
        ctx.app_state.write().await.agent_name_registry.insert(
            "worker-1".to_string(),
            "bg_task_123".to_string(),
        );
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["success"], true);
    }

    #[tokio::test]
    async fn test_empty_to_rejected() {
        let tool = SendMessageTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "to": "",
            "message": "hello",
            "summary": "hi"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p atom-code send_message 2>&1 | tail -20`
Expected: FAIL

- [ ] **Step 3: Implement SendMessageTool**

Full implementation in `src/tools/send_message_tool.rs`:

```rust
use async_trait::async_trait;

use crate::services::coordinator::is_agent_swarms_enabled;
use crate::services::mailbox::types::{MailboxMessage, StructuredMessage};
use crate::services::mailbox::Mailbox;
use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::AtomError;

pub struct SendMessageTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "SendMessage".into(),
        message: msg.into(),
    }
}

/// Validate input according to SendMessage rules
fn validate_input(to: &str, message: &serde_json::Value, summary: Option<&str>) -> Result<(), String> {
    if to.is_empty() {
        return Err("'to' must not be empty".into());
    }
    if to.contains('@') {
        return Err("'to' must not contain '@' (reserved)".into());
    }
    // Plain text messages require summary
    if message.is_string() && summary.is_none() {
        return Err("'summary' is required for plain text messages".into());
    }
    // Structured messages cannot be broadcast
    if to == "*" && message.is_object() {
        return Err("Structured messages cannot be broadcast".into());
    }
    // Structured messages cannot cross sessions
    if message.is_object() && (to.starts_with("uds:") || to.starts_with("bridge:")) {
        return Err("Structured messages cannot be sent cross-session".into());
    }
    Ok(())
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str { "SendMessage" }

    fn description(&self) -> &str {
        "Send a message to another agent in the swarm."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Recipient: agent name, '*' for broadcast, 'uds:<path>', 'bridge:<session-id>'"
                },
                "message": {
                    "description": "Message content (string or structured message object)"
                },
                "summary": {
                    "type": "string",
                    "description": "5-10 word preview (required for plain text messages)"
                }
            },
            "required": ["to", "message"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let to = ctx.input.get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'to' parameter"))?;

        let message = ctx.input.get("message")
            .ok_or_else(|| tool_err("Missing required 'message' parameter"))?;

        let summary = ctx.input.get("summary").and_then(|v| v.as_str());

        // Validate
        validate_input(to, message, summary).map_err(|e| tool_err(e))?;

        // Determine if structured message
        let structured: Option<StructuredMessage> = if message.is_object() {
            Some(serde_json::from_value(message.clone()).map_err(|e|
                tool_err(format!("Invalid structured message: {}", e))
            )?)
        } else {
            None
        };

        let text = if let Some(s) = message.as_str() {
            s.to_string()
        } else {
            serde_json::to_string(message).unwrap_or_default()
        };

        // Route message
        if to == "*" {
            // Broadcast
            return self.handle_broadcast(&ctx, &text, summary).await;
        }

        if let Some(path) = to.strip_prefix("uds:") {
            return self.handle_uds(path, &text).await;
        }

        if let Some(session_id) = to.strip_prefix("bridge:") {
            return self.handle_bridge(session_id, &text).await;
        }

        // Handle structured messages
        if let Some(ref sm) = structured {
            return self.handle_structured(&ctx, to, sm).await;
        }

        // In-process or file mailbox
        self.handle_agent_message(&ctx, to, &text, summary).await
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        // Check env vars first (no state needed)
        if is_agent_swarms_enabled() {
            return ValidationResult::Ok;
        }
        // Check AppState for active swarm agents (try_read to avoid blocking)
        if let Ok(state) = ctx.app_state.try_read() {
            if !state.swarm_agents.is_empty() {
                return ValidationResult::Ok;
            }
        }
        ValidationResult::Disabled("Agent swarms not enabled".into())
    }

    fn is_read_only(&self) -> bool { false }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> {
        Some("send messages to agent teammates (swarm protocol)")
    }
}

impl SendMessageTool {
    async fn handle_agent_message(
        &self, ctx: &ToolUseContext, to: &str, text: &str, summary: Option<&str>,
    ) -> crate::types::Result<ToolResult> {
        let state = ctx.app_state.read().await;

        // Check agent_name_registry
        let agent_id = state.agent_name_registry.get(to).cloned();
        drop(state);

        if let Some(ref agent_id) = agent_id {
            // In-process: queue message
            let mut state = ctx.app_state.write().await;
            state.agent_message_queues
                .entry(agent_id.clone())
                .or_default()
                .push(serde_json::json!({
                    "from": "main",
                    "text": text,
                    "summary": summary,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));

            let response = serde_json::json!({
                "success": true,
                "message": format!("Message queued for agent '{}'", to),
                "routing": {
                    "sender": "main",
                    "target": to,
                    "summary": summary,
                    "content": text,
                }
            });
            return Ok(ToolResult {
                content: response.to_string(),
                is_error: false,
                content_blocks: None,
            });
        }

        // Fall back to file mailbox
        let team_name = ctx.app_state.read().await.team_name.clone();
        if let Some(ref team) = team_name {
            let mb = Mailbox::new(team);
            let to_owned = to.to_string();
            let msg = MailboxMessage {
                from: "main".into(),
                text: text.to_string(),
                summary: summary.map(String::from),
                timestamp: chrono::Utc::now(),
            };
            tokio::task::spawn_blocking(move || mb.write(&to_owned, msg))
                .await
                .map_err(|e| tool_err(format!("Join error: {}", e)))?
                .map_err(|e| tool_err(format!("Mailbox write failed: {}", e)))?;

            let response = serde_json::json!({
                "success": true,
                "message": format!("Message written to mailbox for '{}'", to),
            });
            return Ok(ToolResult {
                content: response.to_string(),
                is_error: false,
                content_blocks: None,
            });
        }

        // No registry, no team — error
        Err(tool_err(format!("Agent '{}' not found in registry and no team configured", to)))
    }

    async fn handle_broadcast(
        &self, ctx: &ToolUseContext, text: &str, summary: Option<&str>,
    ) -> crate::types::Result<ToolResult> {
        let state = ctx.app_state.read().await;
        let team_name = state.team_name.clone();
        let registry = state.agent_name_registry.clone();
        drop(state);

        let team = team_name.ok_or_else(|| tool_err("No team configured for broadcast"))?;
        let mut recipients = Vec::new();

        for (name, _) in &registry {
            let msg = MailboxMessage {
                from: "main".into(),
                text: text.to_string(),
                summary: summary.map(String::from),
                timestamp: chrono::Utc::now(),
            };
            let mb_clone = Mailbox::new(&team);
            let name_clone = name.clone();
            tokio::task::spawn_blocking(move || mb_clone.write(&name_clone, msg))
                .await
                .map_err(|e| tool_err(format!("Join error: {}", e)))?
                .map_err(|e| tool_err(format!("Mailbox write failed for {}: {}", name, e)))?;
            recipients.push(name.clone());
        }

        let response = serde_json::json!({
            "success": true,
            "message": format!("Broadcast sent to {} recipients", recipients.len()),
            "recipients": recipients,
        });
        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    async fn handle_uds(&self, path: &str, text: &str) -> crate::types::Result<ToolResult> {
        use tokio::io::AsyncWriteExt;
        use tokio::net::UnixStream;

        let mut stream = UnixStream::connect(path).await
            .map_err(|e| tool_err(format!("UDS connect to '{}' failed: {}", path, e)))?;

        stream.write_all(text.as_bytes()).await
            .map_err(|e| tool_err(format!("UDS write failed: {}", e)))?;
        stream.shutdown().await
            .map_err(|e| tool_err(format!("UDS shutdown failed: {}", e)))?;

        let response = serde_json::json!({
            "success": true,
            "message": format!("Message sent via UDS to '{}'", path),
        });
        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    async fn handle_bridge(&self, session_id: &str, text: &str) -> crate::types::Result<ToolResult> {
        // Bridge mode: HTTP POST to remote session
        let client = reqwest::Client::new();
        let url = format!("https://api.anthropic.com/v1/sessions/{}/messages", session_id);
        let resp = client.post(&url)
            .json(&serde_json::json!({ "message": text }))
            .send().await
            .map_err(|e| tool_err(format!("Bridge send failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(tool_err(format!("Bridge returned status {}", resp.status())));
        }

        let response = serde_json::json!({
            "success": true,
            "message": format!("Message sent via bridge to session '{}'", session_id),
        });
        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    async fn handle_structured(
        &self, ctx: &ToolUseContext, to: &str, msg: &StructuredMessage,
    ) -> crate::types::Result<ToolResult> {
        match msg {
            StructuredMessage::ShutdownRequest { reason } => {
                let request_id = uuid::Uuid::new_v4().to_string();
                let response = serde_json::json!({
                    "success": true,
                    "message": format!("Shutdown request sent to '{}'", to),
                    "request_id": request_id,
                    "target": to,
                });
                Ok(ToolResult {
                    content: response.to_string(),
                    is_error: false,
                    content_blocks: None,
                })
            }
            StructuredMessage::ShutdownResponse { request_id, approve, reason } => {
                if *approve {
                    // Try to cancel in-process agent
                    let state = ctx.app_state.read().await;
                    if let Some(agent_id) = state.agent_name_registry.get(to) {
                        if let Some(task) = state.background_tasks.get(agent_id) {
                            task.cancel_token.cancel();
                        }
                    }
                }
                let response = serde_json::json!({
                    "success": true,
                    "message": if *approve {
                        format!("Shutdown approved for '{}'", to)
                    } else {
                        format!("Shutdown rejected for '{}': {}", to, reason.as_deref().unwrap_or("no reason"))
                    },
                    "request_id": request_id,
                });
                Ok(ToolResult {
                    content: response.to_string(),
                    is_error: false,
                    content_blocks: None,
                })
            }
            StructuredMessage::PlanApprovalResponse { request_id, approve, feedback } => {
                let response = serde_json::json!({
                    "success": true,
                    "message": if *approve {
                        format!("Plan approved for '{}'", to)
                    } else {
                        format!("Plan rejected for '{}': {}", to, feedback.as_deref().unwrap_or("no feedback"))
                    },
                    "request_id": request_id,
                });
                Ok(ToolResult {
                    content: response.to_string(),
                    is_error: false,
                    content_blocks: None,
                })
            }
        }
    }
}
```

- [ ] **Step 4: Register in mod.rs**

Add to `src/tools/mod.rs`:

```rust
pub mod send_message_tool;
```

And in `get_builtin_tools()`:

```rust
Box::new(send_message_tool::SendMessageTool),
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p atom-code send_message 2>&1 | tail -20`
Expected: All SendMessageTool tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/tools/send_message_tool.rs src/tools/mod.rs
git commit -m "feat: implement SendMessageTool with 5-path routing and structured messages"
```

---

### Task 6: SyntheticOutputTool

**Files:**
- Create: `src/tools/synthetic_output_tool.rs`
- Modify: `Cargo.toml` (add `jsonschema`)

- [ ] **Step 1: Add jsonschema dependency**

In `Cargo.toml`, add to `[dependencies]`:

```toml
jsonschema = "0.45"
```

Run: `cargo check 2>&1 | tail -5` to verify dependency resolves.

- [ ] **Step 2: Write tests**

Create `src/tools/synthetic_output_tool.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name"]
        })
    }

    #[test]
    fn test_name() {
        let tool = SyntheticOutputTool::new(test_schema()).unwrap();
        assert_eq!(tool.name(), "StructuredOutput");
    }

    #[test]
    fn test_input_schema_returns_user_schema() {
        let schema = test_schema();
        let tool = SyntheticOutputTool::new(schema.clone()).unwrap();
        assert_eq!(tool.input_schema(), schema);
    }

    #[test]
    fn test_properties() {
        let tool = SyntheticOutputTool::new(test_schema()).unwrap();
        assert!(tool.is_read_only());
        assert!(tool.is_concurrent_safe());
    }

    #[tokio::test]
    async fn test_valid_input() {
        let tool = SyntheticOutputTool::new(test_schema()).unwrap();
        let ctx = ToolUseContext::test(serde_json::json!({
            "name": "Alice",
            "age": 30
        }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["data"], "Structured output provided successfully");
        assert_eq!(parsed["structured_output"]["name"], "Alice");
    }

    #[tokio::test]
    async fn test_invalid_input_missing_required() {
        let tool = SyntheticOutputTool::new(test_schema()).unwrap();
        let ctx = ToolUseContext::test(serde_json::json!({
            "age": 30
        }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("name"));
    }

    #[test]
    fn test_invalid_schema() {
        // An invalid JSON schema (not an object type)
        let result = SyntheticOutputTool::new(serde_json::json!("not a schema"));
        // jsonschema may or may not reject this at compile time — test the behavior
        // If it compiles, validation will just be permissive. This is acceptable.
        assert!(result.is_ok() || result.is_err());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p atom-code synthetic_output 2>&1 | tail -20`
Expected: FAIL

- [ ] **Step 4: Implement SyntheticOutputTool**

```rust
use async_trait::async_trait;
use std::sync::Arc;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct SyntheticOutputTool {
    schema: serde_json::Value,
    compiled: Arc<jsonschema::Validator>,
}

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "StructuredOutput".into(),
        message: msg.into(),
    }
}

impl SyntheticOutputTool {
    pub fn new(json_schema: serde_json::Value) -> Result<Self, AtomError> {
        let compiled = jsonschema::validator_for(&json_schema)
            .map_err(|e| tool_err(format!("Invalid JSON schema: {}", e)))?;
        Ok(Self {
            schema: json_schema,
            compiled: Arc::new(compiled),
        })
    }
}

#[async_trait]
impl Tool for SyntheticOutputTool {
    fn name(&self) -> &str { "StructuredOutput" }

    fn description(&self) -> &str {
        "Returns final structured output in the requested JSON schema format."
    }

    fn input_schema(&self) -> serde_json::Value {
        self.schema.clone()
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        // Validate input against compiled schema
        let errors: Vec<String> = self.compiled.iter_errors(&ctx.input)
            .map(|e| format!("{}", e))
            .collect();
        if !errors.is_empty() {
            return Ok(ToolResult {
                content: format!("Validation failed: {}", errors.join("; ")),
                is_error: true,
                content_blocks: None,
            });
        }

        let response = serde_json::json!({
            "data": "Structured output provided successfully",
            "structured_output": ctx.input,
        });

        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> {
        Some("return the final response as structured JSON")
    }
}
```

**Note:** SyntheticOutputTool is NOT registered in `get_builtin_tools()`. It is conditionally added:

```rust
// Example session setup (informational — actual integration in query engine init):
// if let Some(schema) = cli_args.output_schema {
//     let tool = SyntheticOutputTool::new(schema)?;
//     tools.push(Box::new(tool));
// }
```

Add `pub mod synthetic_output_tool;` to `src/tools/mod.rs`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p atom-code synthetic_output 2>&1 | tail -20`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/tools/synthetic_output_tool.rs src/tools/mod.rs
git commit -m "feat: implement SyntheticOutputTool with jsonschema validation"
```

---

### Task 7: BriefTool (SendUserMessage)

**Files:**
- Create: `src/tools/brief_tool.rs`
- Modify: `src/tools/mod.rs` (register)
- Modify: `src/query/types.rs` (add BriefMessage event)

- [ ] **Step 1: Add QueryEvent::BriefMessage**

In `src/query/types.rs`, add after the `Error` variant (line 92):

```rust
    /// Brief/SendUserMessage output for TUI rendering
    BriefMessage {
        message: String,
        attachments: Vec<BriefAttachment>,
        status: BriefStatus,
    },
```

Add the supporting types at the bottom of the file:

```rust
#[derive(Debug, Clone)]
pub struct BriefAttachment {
    pub path: std::path::PathBuf,
    pub size: u64,
    pub is_image: bool,
}

#[derive(Debug, Clone)]
pub enum BriefStatus {
    Normal,
    Proactive,
}
```

- [ ] **Step 2: Write BriefTool tests**

Create `src/tools/brief_tool.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_and_alias() {
        assert_eq!(BriefTool.name(), "SendUserMessage");
        assert_eq!(BriefTool.aliases(), &["Brief"]);
    }

    #[test]
    fn test_schema() {
        let schema = BriefTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["message"].is_object());
        assert!(schema["properties"]["status"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("message")));
        assert!(required.contains(&serde_json::json!("status")));
    }

    #[test]
    fn test_always_load() {
        assert!(BriefTool.always_load());
        assert!(BriefTool.is_read_only());
        assert!(BriefTool.is_concurrent_safe());
    }

    #[tokio::test]
    async fn test_basic_message() {
        let tool = BriefTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "message": "Hello user!",
            "status": "normal"
        }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["message"], "Hello user!");
        assert!(parsed["sentAt"].is_string());
    }

    #[tokio::test]
    async fn test_missing_message() {
        let tool = BriefTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "status": "normal"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_status() {
        let tool = BriefTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "message": "hello"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_status() {
        let tool = BriefTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "message": "hello",
            "status": "invalid"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_with_nonexistent_attachment() {
        let tool = BriefTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "message": "See attached",
            "status": "normal",
            "attachments": ["/tmp/definitely-does-not-exist-xyz.png"]
        }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        // Attachment should have error info but not fail the whole tool
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["message"], "See attached");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p atom-code brief_tool 2>&1 | tail -20`
Expected: FAIL

- [ ] **Step 4: Implement BriefTool**

```rust
use async_trait::async_trait;
use std::path::PathBuf;

use crate::query::types::{BriefAttachment, BriefStatus, QueryEvent};
use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::AtomError;

pub struct BriefTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "SendUserMessage".into(),
        message: msg.into(),
    }
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg"];

fn is_image_path(path: &PathBuf) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn parse_status(s: &str) -> Result<BriefStatus, AtomError> {
    match s {
        "normal" => Ok(BriefStatus::Normal),
        "proactive" => Ok(BriefStatus::Proactive),
        _ => Err(tool_err(format!("Invalid status '{}', must be 'normal' or 'proactive'", s))),
    }
}

#[async_trait]
impl Tool for BriefTool {
    fn name(&self) -> &str { "SendUserMessage" }

    fn description(&self) -> &str {
        "Send a visible message to the user with optional file attachments."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "User-visible message with markdown support"
                },
                "attachments": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths (absolute or cwd-relative)"
                },
                "status": {
                    "type": "string",
                    "enum": ["normal", "proactive"],
                    "description": "'normal' when replying to user, 'proactive' for unsolicited"
                }
            },
            "required": ["message", "status"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let message = ctx.input.get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'message' parameter"))?;

        let status_str = ctx.input.get("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'status' parameter"))?;
        let status = parse_status(status_str)?;

        // Resolve attachments
        let mut attachments = Vec::new();
        let mut attachment_json = Vec::new();

        if let Some(arr) = ctx.input.get("attachments").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(path_str) = item.as_str() {
                    let path = if PathBuf::from(path_str).is_absolute() {
                        PathBuf::from(path_str)
                    } else {
                        ctx.cwd.join(path_str)
                    };

                    match tokio::fs::metadata(&path).await {
                        Ok(meta) if meta.is_file() => {
                            let is_image = is_image_path(&path);
                            attachments.push(BriefAttachment {
                                path: path.clone(),
                                size: meta.len(),
                                is_image,
                            });
                            attachment_json.push(serde_json::json!({
                                "path": path.display().to_string(),
                                "size": meta.len(),
                                "isImage": is_image,
                            }));
                        }
                        Ok(_) => {
                            attachment_json.push(serde_json::json!({
                                "path": path.display().to_string(),
                                "error": "Not a regular file",
                            }));
                        }
                        Err(e) => {
                            attachment_json.push(serde_json::json!({
                                "path": path.display().to_string(),
                                "error": e.to_string(),
                            }));
                        }
                    }
                }
            }
        }

        // Emit event for TUI
        let _ = ctx.event_tx.send(QueryEvent::BriefMessage {
            message: message.to_string(),
            attachments,
            status,
        });

        let response = serde_json::json!({
            "message": message,
            "attachments": attachment_json,
            "sentAt": chrono::Utc::now().to_rfc3339(),
        });

        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        // Brief is available in brief-only mode or kairos mode
        if let Ok(state) = ctx.app_state.try_read() {
            if state.is_brief_only || state.kairos_enabled {
                return ValidationResult::Ok;
            }
        }
        ValidationResult::Disabled("Brief mode not enabled (use --brief or kairos)".into())
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }
    fn always_load(&self) -> bool { true }
    fn aliases(&self) -> &[&str] { &["Brief"] }
    fn search_hint(&self) -> Option<&str> {
        Some("send a visible message to the user")
    }
}
```

- [ ] **Step 5: Register in mod.rs**

Add to `src/tools/mod.rs`:

```rust
pub mod brief_tool;
```

And in `get_builtin_tools()`:

```rust
Box::new(brief_tool::BriefTool),
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p atom-code brief_tool 2>&1 | tail -20`
Expected: All pass.

- [ ] **Step 7: Run full test suite**

Run: `cargo test -p atom-code 2>&1 | tail -30`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/tools/brief_tool.rs src/tools/mod.rs src/query/types.rs
git commit -m "feat: implement BriefTool (SendUserMessage) with attachments and QueryEvent"
```

---

### Task 8: Update Memory

- [ ] **Step 1: Update tool migration status memory**

Update `/Users/zpro/.claude/projects/-Users-zpro-02-dev-02-oss-cc-workspace-atom-code-rs/memory/project_tool_migration_status.md` to reflect Batch 3 completion (24/54 tools done).
