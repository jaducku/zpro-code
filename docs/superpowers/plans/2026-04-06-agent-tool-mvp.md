# AgentTool MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the AgentTool stub with real foreground/background subagent spawning using the existing query engine.

**Architecture:** AgentTool.execute() dispatches to `agent_runtime` module. Foreground calls query() synchronously and returns result. Background spawns a tokio task, registers as BackgroundTask in AppState, and notifies via QueryEvent::AgentCompleted. Tools are fresh-created per agent (Box<dyn Tool> not Clone).

**Tech Stack:** Rust, tokio, async_trait, serde_json

**Spec:** `docs/superpowers/specs/2026-04-06-agent-tool-mvp-design.md`

---

### Task 1: Add `agent_depth` to ToolUseContext

**Files:**
- Modify: `src/tool.rs:77-96` (ToolUseContext struct)
- Modify: `src/tool.rs:99-127` (ToolUseContext::test())
- Modify: `src/query/engine.rs:427-444` (execute_tool function)

- [ ] **Step 1: Add `agent_depth` field to ToolUseContext**

In `src/tool.rs`, add field after `mcp_manager`:

```rust
pub agent_depth: u32,
```

- [ ] **Step 2: Update ToolUseContext::test() default**

In `src/tool.rs`, add to the test constructor:

```rust
agent_depth: 0,
```

- [ ] **Step 3: Pass agent_depth in execute_tool()**

In `src/query/engine.rs:427-444`, add to the ToolUseContext construction:

```rust
agent_depth: ctx.agent_depth.unwrap_or(0),
```

- [ ] **Step 4: Add `agent_depth` field to QueryContext**

In `src/query/types.rs`, add to QueryContext struct:

```rust
/// Current agent nesting depth (0 = top-level)
pub agent_depth: Option<u32>,
```

- [ ] **Step 5: Set agent_depth in submit_query()**

In `src/tui/run.rs:623-645`, add to the QueryContext construction:

```rust
agent_depth: None,
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 7: Commit**

```bash
git add src/tool.rs src/query/engine.rs src/query/types.rs src/tui/run.rs
git commit -m "feat(agent): add agent_depth to ToolUseContext for recursion tracking"
```

---

### Task 2: Add `pending_agent_results` to AppState

**Files:**
- Modify: `src/state/app_state.rs:120-180` (AppState struct)

> Note: `QueryEvent::AgentCompleted` is added later in Task 7 together with its handler, to avoid non-exhaustive match errors between tasks.

- [ ] **Step 1: Add PendingAgentResult struct**

In `src/state/app_state.rs`, add before AppState:

```rust
/// A completed background agent result pending injection
#[derive(Debug, Clone)]
pub struct PendingAgentResult {
    pub agent_id: String,
    pub description: String,
    pub result: String,
    pub is_error: bool,
}
```

- [ ] **Step 2: Add pending_agent_results field to AppState**

In `src/state/app_state.rs`, add after `background_tasks` field (line ~133):

```rust
/// Completed background agent results waiting to be injected into next query
#[serde(skip)]
#[serde(default)]
pub pending_agent_results: Vec<PendingAgentResult>,
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 4: Commit**

```bash
git add src/state/app_state.rs
git commit -m "feat(agent): add PendingAgentResult and pending_agent_results to AppState"
```

---

### Task 3: Create `agent_runtime` module — tool filtering + context building

**Files:**
- Create: `src/services/agent_runtime.rs`
- Modify: `src/services/mod.rs`

- [ ] **Step 1: Write tests for tool filtering**

Create `src/services/agent_runtime.rs` with:

```rust
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::query::types::{QueryContext, QueryEvent};
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::tools::get_builtin_tools;

/// Tools allowed for background agents (read-only + search)
const BACKGROUND_ALLOWED_TOOLS: &[&str] = &[
    "Read", "Glob", "Grep", "WebFetch", "WebSearch",
    "ToolSearch", "TodoWrite",
    "ListMcpResources", "ReadMcpResource",
];

/// Parameters for spawning an agent
pub struct AgentSpawnParams {
    pub prompt: String,
    pub description: String,
    pub model: Option<String>,
    pub run_in_background: bool,
}

impl AgentSpawnParams {
    pub fn from_input(input: &serde_json::Value) -> Result<Self, crate::types::AtomError> {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Agent".into(),
                message: "Missing required 'prompt' parameter".into(),
            })?
            .to_string();

        let description = input
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("agent task")
            .to_string();

        let model = input.get("model").and_then(|v| v.as_str()).map(String::from);

        let run_in_background = input
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(Self {
            prompt,
            description,
            model,
            run_in_background,
        })
    }
}

/// Result of a completed agent run
pub struct AgentResult {
    pub agent_id: String,
    pub output: String,
    pub is_error: bool,
}

/// Create a fresh set of tools for an agent.
/// Box<dyn Tool> is not Clone, so we create new instances.
pub fn get_agent_tools(is_background: bool) -> Vec<Box<dyn Tool>> {
    let all = get_builtin_tools();
    if !is_background {
        return all;
    }
    all.into_iter()
        .filter(|t| BACKGROUND_ALLOWED_TOOLS.contains(&t.name()))
        .collect()
}

/// Build a child QueryContext from the parent's ToolUseContext.
/// event_tx and abort_signal are passed separately to query(), not stored in QueryContext.
pub fn build_child_context(
    params: &AgentSpawnParams,
    parent_ctx: &ToolUseContext,
    tools: Vec<Box<dyn Tool>>,
) -> QueryContext {
    use crate::types::content::ContentBlock;
    use crate::types::message::{ApiMessage, Role};

    let model = params.model.clone().unwrap_or_else(|| parent_ctx.model.clone());
    let max_tokens = parent_ctx.provider.model_info().max_output_tokens;

    let messages = vec![ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: params.prompt.clone(),
        }],
    }];

    QueryContext {
        messages,
        tools,
        system_prompt: Some(parent_ctx.system_prompt.clone()),
        model,
        max_tokens,
        permission_ctx: parent_ctx.permission_ctx.clone(),
        permission_tx: parent_ctx.permission_tx.clone(),
        question_tx: parent_ctx.question_tx.clone(),
        hooks: crate::services::hooks::load_all_hooks(&parent_ctx.cwd),
        cwd: parent_ctx.cwd.clone(),
        max_turns: Some(50),
        max_budget_usd: None,
        commands: parent_ctx.commands.clone(),
        provider: parent_ctx.provider.clone(),
        app_state: parent_ctx.app_state.clone(),
        mcp_manager: parent_ctx.mcp_manager.clone(),
        agent_depth: Some(parent_ctx.agent_depth + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_agent_tools_foreground_includes_all() {
        let tools = get_agent_tools(false);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"Agent"), "foreground should include Agent");
        assert!(names.contains(&"Write"), "foreground should include Write");
        assert!(names.contains(&"Read"), "foreground should include Read");
    }

    #[test]
    fn test_get_agent_tools_background_is_restricted() {
        let tools = get_agent_tools(true);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"Read"), "background should include Read");
        assert!(names.contains(&"Grep"), "background should include Grep");
        assert!(names.contains(&"Glob"), "background should include Glob");
        assert!(!names.contains(&"Agent"), "background should exclude Agent");
        assert!(!names.contains(&"Write"), "background should exclude Write");
        assert!(!names.contains(&"Edit"), "background should exclude Edit");
    }

    #[test]
    fn test_agent_spawn_params_from_input() {
        let input = serde_json::json!({
            "prompt": "Fix the bug",
            "description": "fix bug",
            "model": "gpt-4o",
            "run_in_background": true
        });
        let params = AgentSpawnParams::from_input(&input).unwrap();
        assert_eq!(params.prompt, "Fix the bug");
        assert_eq!(params.description, "fix bug");
        assert_eq!(params.model, Some("gpt-4o".into()));
        assert!(params.run_in_background);
    }

    #[test]
    fn test_agent_spawn_params_defaults() {
        let input = serde_json::json!({ "prompt": "Do something" });
        let params = AgentSpawnParams::from_input(&input).unwrap();
        assert_eq!(params.description, "agent task");
        assert_eq!(params.model, None);
        assert!(!params.run_in_background);
    }

    #[test]
    fn test_agent_spawn_params_missing_prompt() {
        let input = serde_json::json!({});
        assert!(AgentSpawnParams::from_input(&input).is_err());
    }

    #[test]
    fn test_build_child_context() {
        let parent_ctx = ToolUseContext::test(serde_json::json!({}));
        let params = AgentSpawnParams {
            prompt: "test prompt".into(),
            description: "test".into(),
            model: Some("custom-model".into()),
            run_in_background: false,
        };
        let tools = get_agent_tools(false);

        let child = build_child_context(&params, &parent_ctx, tools);

        assert_eq!(child.model, "custom-model");
        assert_eq!(child.messages.len(), 1);
        assert_eq!(child.max_turns, Some(50));
        assert_eq!(child.agent_depth, Some(1));
    }

    #[test]
    fn test_build_child_context_inherits_parent_model() {
        let parent_ctx = ToolUseContext::test(serde_json::json!({}));
        let params = AgentSpawnParams {
            prompt: "test".into(),
            description: "test".into(),
            model: None,
            run_in_background: false,
        };
        let tools = get_agent_tools(false);

        let child = build_child_context(&params, &parent_ctx, tools);

        assert_eq!(child.model, "test"); // inherited from parent_ctx.model
    }
}
```

- [ ] **Step 2: Add module to services/mod.rs**

In `src/services/mod.rs`, add:

```rust
pub mod agent_runtime;
```

- [ ] **Step 3: Run tests**

Run: `cargo test services::agent_runtime -- --nocapture 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add src/services/agent_runtime.rs src/services/mod.rs
git commit -m "feat(agent): add agent_runtime module with tool filtering and context building"
```

---

### Task 4: Implement foreground agent execution

**Files:**
- Modify: `src/services/agent_runtime.rs`

- [ ] **Step 1: Add extract_agent_output helper and run_foreground function**

Add to `src/services/agent_runtime.rs` after `build_child_context`:

```rust
/// Extract the last assistant message text from a completed query's messages.
fn extract_agent_output(messages: &[crate::types::message::ApiMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|m| m.role == crate::types::message::Role::Assistant)
        .map(|m| {
            m.content
                .iter()
                .filter_map(|c| match c {
                    crate::types::content::ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "Agent completed with no text output.".into())
}
```

Then add:

```rust
/// Run an agent in foreground (blocking) mode.
/// Calls query() and waits for completion, returning the assistant's response.
pub async fn run_foreground(
    params: AgentSpawnParams,
    parent_ctx: &ToolUseContext,
) -> Result<AgentResult, crate::types::AtomError> {
    let agent_id = uuid::Uuid::new_v4().to_string();
    let tools = get_agent_tools(false);
    let child_abort = parent_ctx.abort_signal.child_token();

    let child_ctx = build_child_context(&params, parent_ctx, tools);

    let result = crate::query::engine::query(child_ctx, parent_ctx.event_tx.clone(), child_abort).await;

    match result {
        Ok(query_result) => {
            let output = extract_agent_output(&query_result.messages);
            Ok(AgentResult {
                agent_id,
                output,
                is_error: false,
            })
        }
        Err(e) => Ok(AgentResult {
            agent_id,
            output: format!("Agent failed: {}", e),
            is_error: true,
        }),
    }
}
```

- [ ] **Step 2: Add uuid dependency if needed**

Run: `cargo check 2>&1 | grep uuid`

If uuid is not available, add to Cargo.toml:
```toml
uuid = { version = "1", features = ["v4"] }
```

If uuid is already a dependency, skip this step.

- [ ] **Step 3: Verify compilation**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 4: Commit**

```bash
git add src/services/agent_runtime.rs Cargo.toml Cargo.lock
git commit -m "feat(agent): implement foreground agent execution via query engine"
```

---

### Task 5: Implement background agent execution

**Files:**
- Modify: `src/services/agent_runtime.rs`

- [ ] **Step 1: Add run_background function**

Add to `src/services/agent_runtime.rs`:

```rust
/// Run an agent in background (async) mode.
/// Spawns a tokio task, registers as BackgroundTask, returns immediately.
pub async fn run_background(
    params: AgentSpawnParams,
    parent_ctx: &ToolUseContext,
) -> Result<ToolResult, crate::types::AtomError> {
    let agent_id = uuid::Uuid::new_v4().to_string();
    let description = params.description.clone();
    let tools = get_agent_tools(true);
    let cancel_token = CancellationToken::new();

    // Register as BackgroundTask in AppState
    {
        let mut state = parent_ctx.app_state.write().await;
        state.background_tasks.insert(
            agent_id.clone(),
            crate::state::BackgroundTask {
                id: agent_id.clone(),
                task_type: "agent".into(),
                command: Some(format!("Agent: {}", &description)),
                status: crate::state::BackgroundTaskStatus::Running,
                cancel_token: cancel_token.clone(),
                output: Arc::new(std::sync::Mutex::new(String::new())),
                created_at: chrono::Utc::now(),
            },
        );
    }

    let child_ctx = build_child_context(&params, parent_ctx, tools);

    let event_tx = parent_ctx.event_tx.clone();
    let app_state = parent_ctx.app_state.clone();
    let agent_id_clone = agent_id.clone();
    let description_clone = description.clone();

    tokio::spawn(async move {
        let result = crate::query::engine::query(child_ctx, event_tx.clone(), cancel_token).await;

        let (result_text, is_error) = match result {
            Ok(query_result) => {
                let output = extract_agent_output(&query_result.messages);
                (output, false)
            }
            Err(e) => (format!("Agent failed: {}", e), true),
        };

        // Update BackgroundTask status
        {
            let mut state = app_state.write().await;
            if let Some(task) = state.background_tasks.get_mut(&agent_id_clone) {
                task.status = if is_error {
                    crate::state::BackgroundTaskStatus::Failed
                } else {
                    crate::state::BackgroundTaskStatus::Completed
                };
                *task.output.lock().unwrap() = result_text.clone();
            }

            // Store as pending result for injection into next query
            state.pending_agent_results.push(
                crate::state::PendingAgentResult {
                    agent_id: agent_id_clone.clone(),
                    description: description_clone.clone(),
                    result: result_text.clone(),
                    is_error,
                },
            );
        }

        // Notify TUI
        let _ = event_tx.send(QueryEvent::AgentCompleted {
            agent_id: agent_id_clone,
            description: description_clone,
            result: Some(result_text),
            is_error,
        });
    });

    // Return immediately
    let response = serde_json::json!({
        "status": "background_started",
        "agent_id": agent_id,
        "description": description,
        "note": "Agent is running in the background. You will be notified when it completes."
    });

    Ok(ToolResult {
        content: serde_json::to_string_pretty(&response).unwrap_or_default(),
        is_error: false,
        content_blocks: None,
    })
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 3: Commit**

```bash
git add src/services/agent_runtime.rs
git commit -m "feat(agent): implement background agent execution with BackgroundTask registration"
```

---

### Task 6: Wire AgentTool.execute() to agent_runtime

**Files:**
- Modify: `src/tools/agent_tool.rs`

- [ ] **Step 1: Replace stub execute() with real implementation**

Replace the entire `execute` method in `src/tools/agent_tool.rs`:

```rust
    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        // Enforce max nesting depth
        if ctx.agent_depth >= 3 {
            return Ok(ToolResult {
                content: "Maximum agent nesting depth (3) reached. Cannot spawn further sub-agents.".into(),
                is_error: true,
                content_blocks: None,
            });
        }

        let params = crate::services::agent_runtime::AgentSpawnParams::from_input(&ctx.input)?;

        if params.run_in_background {
            crate::services::agent_runtime::run_background(params, &ctx).await
        } else {
            let result = crate::services::agent_runtime::run_foreground(params, &ctx).await?;
            Ok(ToolResult {
                content: result.output,
                is_error: result.is_error,
                content_blocks: None,
            })
        }
    }
```

- [ ] **Step 2: Update tests**

Replace the test module in `src/tools/agent_tool.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        ToolUseContext::test(input)
    }

    #[test]
    fn test_agent_tool_name() {
        let tool = AgentTool;
        assert_eq!(tool.name(), "Agent");
    }

    #[test]
    fn test_agent_tool_schema() {
        let tool = AgentTool;
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");

        let props = &schema["properties"];
        assert!(props["prompt"].is_object());
        assert!(props["description"].is_object());
        assert!(props["subagent_type"].is_object());
        assert!(props["model"].is_object());
        assert!(props["run_in_background"].is_object());
        assert!(props["isolation"].is_object());

        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "prompt");
    }

    #[test]
    fn test_agent_tool_is_not_read_only() {
        let tool = AgentTool;
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_agent_tool_not_concurrent_safe() {
        let tool = AgentTool;
        assert!(!tool.is_concurrent_safe());
    }

    #[tokio::test]
    async fn test_agent_tool_missing_prompt() {
        let tool = AgentTool;
        let ctx = make_ctx(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_tool_max_depth_rejected() {
        let tool = AgentTool;
        let mut ctx = make_ctx(serde_json::json!({"prompt": "test"}));
        ctx.agent_depth = 3;
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("nesting depth"));
    }

    #[tokio::test]
    async fn test_agent_tool_depth_2_allowed() {
        let tool = AgentTool;
        let mut ctx = make_ctx(serde_json::json!({"prompt": "test"}));
        ctx.agent_depth = 2;
        // This will attempt to run a real query which will fail with NoopProvider,
        // but it should NOT be rejected for depth
        let result = tool.execute(ctx).await;
        // Either Ok (with error from NoopProvider) or Err — but NOT a depth error
        match result {
            Ok(r) => assert!(!r.content.contains("nesting depth")),
            Err(_) => {} // query failure is expected with NoopProvider
        }
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test tools::agent_tool -- --nocapture 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add src/tools/agent_tool.rs
git commit -m "feat(agent): wire AgentTool.execute() to real agent_runtime"
```

---

### Task 7: Add QueryEvent::AgentCompleted and handle in TUI

**Files:**
- Modify: `src/query/types.rs:80-100` (QueryEvent enum)
- Modify: `src/tui/app.rs` (handle_query_event method)
- Modify: `src/tui/run.rs:560-659` (submit_query — inject pending results)

- [ ] **Step 1: Add AgentCompleted variant to QueryEvent**

In `src/query/types.rs`, add to QueryEvent enum:

```rust
/// Background agent completed or failed
AgentCompleted {
    agent_id: String,
    description: String,
    result: Option<String>,
    is_error: bool,
},
```

- [ ] **Step 2: Handle AgentCompleted in app.handle_query_event()**

Find the `handle_query_event` method in `src/tui/app.rs` and add a match arm for `AgentCompleted`:

```rust
QueryEvent::AgentCompleted { agent_id: _, description, result, is_error } => {
    let status = if is_error { "failed" } else { "completed" };
    let summary = result
        .as_deref()
        .map(|r| if r.len() > 200 { &r[..200] } else { r })
        .unwrap_or("no output");
    self.push_message(DisplayMessage {
        role: DisplayRole::System,
        content: format!("Background agent '{}' {}: {}", description, status, summary),
        tool_name: None,
    });
}
```

- [ ] **Step 3: Inject pending agent results in submit_query()**

In `src/tui/run.rs`, inside `submit_query()`, right after adding the user message to api_messages (after line 595) and before setting `is_loading`:

```rust
    // Inject pending background agent results
    {
        let mut state = app_state.write().await;
        let pending = std::mem::take(&mut state.pending_agent_results);
        for result in pending {
            let status = if result.is_error { "failed" } else { "completed" };
            app.api_messages.push(ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: format!(
                        "[Background agent '{}' {}: {}]",
                        result.description, status, result.result
                    ),
                }],
            });
        }
    }
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 5: Commit**

```bash
git add src/query/types.rs src/tui/app.rs src/tui/run.rs
git commit -m "feat(agent): add AgentCompleted event, handle in TUI, inject pending results"
```

---

### Task 8: Full integration verification

**Files:**
- No new files

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass, no regressions

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | tail -20`
Expected: No errors (warnings OK)

- [ ] **Step 3: Verify the build**

Run: `cargo build 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 4: Final commit if any fixups needed**

```bash
git add -A
git commit -m "fix: address clippy and test issues in AgentTool MVP"
```
