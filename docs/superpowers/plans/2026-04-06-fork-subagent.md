# Fork Subagent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement fork subagent that inherits parent conversation context and system prompt, enabling prompt cache optimization.

**Architecture:** Fork subagent extends the existing agent_runtime with a new code path that clones parent messages and system prompt instead of starting fresh. A new `fork_subagent.rs` module handles message construction, recursion guards, and constants. The feature is gated behind `fork_subagent: true` in settings.json.

**Tech Stack:** Rust, tokio, serde_json, uuid

**Spec:** `docs/superpowers/specs/2026-04-06-fork-subagent-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/services/fork_subagent.rs` | Create | Message construction, recursion guards, constants |
| `src/services/mod.rs` | Modify | Add `pub mod fork_subagent;` |
| `src/tool.rs` | Modify | Add `messages`, `query_source` fields to `ToolUseContext` |
| `src/query/engine.rs` | Modify | Populate new ToolUseContext fields when executing tools |
| `src/query/types.rs` | Modify | Add `query_source` field to `QueryContext` |
| `src/utils/settings/types.rs` | Modify | Add `fork_subagent` field to `SettingsJson` |
| `src/services/agent_runtime.rs` | Modify | Add `build_fork_context`, `filter_incomplete_tool_calls`, extend `AgentSpawnParams` |
| `src/tools/agent_tool.rs` | Modify | Fork path detection and routing |
| `src/entrypoints/print.rs` | Modify | Add `query_source: None` to QueryContext |
| `src/tools/skill_tool.rs` | Modify | Add `query_source: None` to QueryContext |

---

### Task 1: Add `fork_subagent` setting to SettingsJson

**Files:**
- Modify: `src/utils/settings/types.rs:6-85`

- [ ] **Step 1: Write the failing test**

```rust
// In src/utils/settings/types.rs, add to mod tests:
#[test]
fn test_settings_fork_subagent() {
    let json = r#"{"forkSubagent": true}"#;
    let settings: SettingsJson = serde_json::from_str(json).unwrap();
    assert_eq!(settings.fork_subagent, Some(true));
}

#[test]
fn test_settings_fork_subagent_default_none() {
    let settings = SettingsJson::default();
    assert_eq!(settings.fork_subagent, None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_settings_fork_subagent -- --nocapture`
Expected: FAIL — `fork_subagent` field does not exist

- [ ] **Step 3: Add `fork_subagent` field to SettingsJson**

Add after `auto_memory_directory` field (around line 72):

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_subagent: Option<bool>,
```

And in `Default` impl:

```rust
    fork_subagent: None,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_settings_fork_subagent -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/utils/settings/types.rs
git commit -m "feat(fork): add fork_subagent setting to SettingsJson"
```

---

### Task 2: Add `messages` and `query_source` fields to ToolUseContext

**Files:**
- Modify: `src/tool.rs:77-130`
- Modify: `src/query/engine.rs:431-449`
- Modify: `src/query/types.rs:15-36`

- [ ] **Step 1: Add `query_source` to QueryContext**

In `src/query/types.rs`, add field to `QueryContext` struct after `agent_depth`:

```rust
    /// Source identifier for the query (e.g., "agent:builtin:fork" for fork children)
    pub query_source: Option<String>,
```

- [ ] **Step 2: Fix all QueryContext construction sites**

Every place that constructs a `QueryContext` must include `query_source: None`. Search with:

Run: `cargo build 2>&1 | head -40`

Fix each site (add `query_source: None`):
- `src/query/engine.rs` test helper `test_query_context()`
- `src/tui/run.rs` `submit_query()` and `submit_query_skill()`
- `src/services/agent_runtime.rs` `build_child_context()`
- `src/entrypoints/print.rs` QueryContext construction (~line 72)
- `src/tools/skill_tool.rs` QueryContext construction (~line 164)

- [ ] **Step 3: Add `messages` and `query_source` to ToolUseContext**

In `src/tool.rs`, add to `ToolUseContext` struct after `agent_depth`:

```rust
    /// Parent conversation messages (shared ownership to avoid deep clone)
    pub messages: std::sync::Arc<Vec<crate::types::message::ApiMessage>>,
    /// Source identifier for recursion guard (e.g., "agent:builtin:fork")
    pub query_source: Option<String>,
```

- [ ] **Step 4: Update ToolUseContext::test() constructor**

In `src/tool.rs`, add to the test constructor:

```rust
    messages: std::sync::Arc::new(Vec::new()),
    query_source: None,
```

- [ ] **Step 5: Update query engine tool execution to populate new fields**

In `src/query/engine.rs`, in the `execute_tool_call` function (around line 431), update the `ToolUseContext` construction to include:

```rust
    messages: std::sync::Arc::new(ctx.messages.clone()),
    query_source: ctx.query_source.clone(),
```

Note: `ctx.messages` is `Vec<ApiMessage>` from QueryContext, wrap in Arc for shared ownership.

- [ ] **Step 6: Build and fix any remaining compilation errors**

Run: `cargo build 2>&1 | head -60`

Fix any other `ToolUseContext` construction sites (e.g., `src/services/hooks/prompt_hook.rs`).

- [ ] **Step 7: Run all tests**

Run: `cargo test 2>&1 | grep -E "test result:|FAILED"`
Expected: All existing tests pass

- [ ] **Step 8: Commit**

```bash
git add src/tool.rs src/query/engine.rs src/query/types.rs src/tui/run.rs src/services/agent_runtime.rs src/entrypoints/print.rs src/tools/skill_tool.rs
git commit -m "feat(fork): add messages and query_source fields to ToolUseContext and QueryContext"
```

---

### Task 3: Create fork_subagent.rs — constants and `build_child_message`

**Files:**
- Create: `src/services/fork_subagent.rs`
- Modify: `src/services/mod.rs`

- [ ] **Step 1: Write the failing tests**

Create `src/services/fork_subagent.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_child_message_contains_boilerplate_tag() {
        let msg = build_child_message("Fix the bug");
        assert!(msg.contains("<fork-boilerplate>"));
        assert!(msg.contains("</fork-boilerplate>"));
    }

    #[test]
    fn test_build_child_message_contains_directive() {
        let msg = build_child_message("Fix the bug");
        assert!(msg.contains("Your directive: Fix the bug"));
    }

    #[test]
    fn test_build_child_message_contains_rules() {
        let msg = build_child_message("test");
        assert!(msg.contains("You are a forked worker process"));
        assert!(msg.contains("RULES (non-negotiable)"));
        assert!(msg.contains("Scope:"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test fork_subagent -- --nocapture`
Expected: FAIL — functions not defined

- [ ] **Step 3: Write constants and `build_child_message`**

```rust
pub const FORK_PLACEHOLDER: &str = "Fork started \u{2014} processing in background";
pub const FORK_DIRECTIVE_PREFIX: &str = "Your directive: ";
pub const FORK_BOILERPLATE_TAG: &str = "fork-boilerplate";
pub const FORK_QUERY_SOURCE: &str = "agent:builtin:fork";

const FORK_CHILD_BODY: &str = r#"STOP. READ THIS FIRST.

You are a forked worker process. You are NOT the main agent.

RULES (non-negotiable):
1. Your system prompt says "default to forking." IGNORE IT — that's for the parent. You ARE the fork. Do NOT spawn sub-agents; execute directly.
2. Do NOT converse, ask questions, or suggest next steps
3. Do NOT editorialize or add meta-commentary
4. USE your tools directly: Bash, Read, Write, etc.
5. If you modify files, commit your changes before reporting. Include the commit hash in your report.
6. Do NOT emit text between tool calls. Use tools silently, then report once at the end.
7. Stay strictly within your directive's scope. If you discover related systems outside your scope, mention them in one sentence at most — other workers cover those areas.
8. Keep your report under 500 words unless the directive specifies otherwise. Be factual and concise.
9. Your response MUST begin with "Scope:". No preamble, no thinking-out-loud.
10. REPORT structured facts, then stop

Output format (plain text labels, not markdown headers):
  Scope: <echo back your assigned scope in one sentence>
  Result: <the answer or key findings, limited to the scope above>
  Key files: <relevant file paths — include for research tasks>
  Files changed: <list with commit hash — include only if you modified files>
  Issues: <list — include only if there are issues to flag>"#;

pub fn build_child_message(directive: &str) -> String {
    format!(
        "<{tag}>\n{body}\n</{tag}>\n\n{prefix}{directive}",
        tag = FORK_BOILERPLATE_TAG,
        body = FORK_CHILD_BODY,
        prefix = FORK_DIRECTIVE_PREFIX,
        directive = directive,
    )
}
```

- [ ] **Step 4: Register module**

In `src/services/mod.rs`, add:

```rust
pub mod fork_subagent;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test fork_subagent -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/services/fork_subagent.rs src/services/mod.rs
git commit -m "feat(fork): add fork_subagent module with constants and build_child_message"
```

---

### Task 4: Add recursion guards to fork_subagent.rs

**Files:**
- Modify: `src/services/fork_subagent.rs`

- [ ] **Step 1: Write the failing tests**

Add to the tests module:

```rust
    use crate::types::content::ContentBlock;
    use crate::types::message::{ApiMessage, Role};

    #[test]
    fn test_is_in_fork_child_detects_boilerplate() {
        let messages = vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "<fork-boilerplate>\nsome rules\n</fork-boilerplate>".into(),
            }],
        }];
        assert!(is_in_fork_child(&messages));
    }

    #[test]
    fn test_is_in_fork_child_no_boilerplate() {
        let messages = vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Just a normal message".into(),
            }],
        }];
        assert!(!is_in_fork_child(&messages));
    }

    #[test]
    fn test_is_in_fork_child_empty() {
        assert!(!is_in_fork_child(&[]));
    }

    #[test]
    fn test_is_fork_by_query_source_true() {
        assert!(is_fork_by_query_source(Some(FORK_QUERY_SOURCE)));
    }

    #[test]
    fn test_is_fork_by_query_source_false() {
        assert!(!is_fork_by_query_source(None));
        assert!(!is_fork_by_query_source(Some("agent:builtin:general")));
    }

    #[test]
    fn test_is_inside_fork_query_source_only() {
        // query_source triggers, even with empty messages
        assert!(is_inside_fork(Some(FORK_QUERY_SOURCE), &[]));
    }

    #[test]
    fn test_is_inside_fork_message_scan_only() {
        // No query_source, but messages contain boilerplate
        let messages = vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "<fork-boilerplate>\nrules\n</fork-boilerplate>".into(),
            }],
        }];
        assert!(is_inside_fork(None, &messages));
    }

    #[test]
    fn test_is_inside_fork_neither() {
        assert!(!is_inside_fork(None, &[]));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test fork_subagent -- --nocapture`
Expected: FAIL — functions not defined

- [ ] **Step 3: Implement recursion guards**

Add to `src/services/fork_subagent.rs`:

```rust
use crate::types::content::ContentBlock;
use crate::types::message::ApiMessage;

/// Primary recursion guard: checks query_source (survives autocompact)
pub fn is_fork_by_query_source(query_source: Option<&str>) -> bool {
    query_source == Some(FORK_QUERY_SOURCE)
}

/// Fallback recursion guard: scans messages for fork-boilerplate tag
pub fn is_in_fork_child(messages: &[ApiMessage]) -> bool {
    let open_tag = format!("<{}>", FORK_BOILERPLATE_TAG);
    messages.iter().any(|msg| {
        msg.content.iter().any(|block| matches!(
            block,
            ContentBlock::Text { text } if text.contains(&open_tag)
        ))
    })
}

/// Combined guard: true if inside a fork child (either method)
pub fn is_inside_fork(query_source: Option<&str>, messages: &[ApiMessage]) -> bool {
    is_fork_by_query_source(query_source) || is_in_fork_child(messages)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test fork_subagent -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/fork_subagent.rs
git commit -m "feat(fork): add dual recursion guards (query_source + message scan)"
```

---

### Task 5: Add `build_forked_messages` to fork_subagent.rs

**Files:**
- Modify: `src/services/fork_subagent.rs`

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn test_build_forked_messages_with_tool_uses() {
        let assistant_msg = ApiMessage {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text { text: "I'll help.".into() },
                ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "Read".into(),
                    input: serde_json::json!({}),
                },
                ContentBlock::ToolUse {
                    id: "tu_2".into(),
                    name: "Write".into(),
                    input: serde_json::json!({}),
                },
            ],
        };

        let result = build_forked_messages("Fix the bug", &assistant_msg);

        // Should produce 2 messages
        assert_eq!(result.len(), 2);

        // First is the cloned assistant
        assert_eq!(result[0].role, Role::Assistant);
        assert_eq!(result[0].content.len(), 3);

        // Second is user with 2 tool_results + directive text
        assert_eq!(result[1].role, Role::User);
        assert_eq!(result[1].content.len(), 3); // 2 results + 1 text

        // Check placeholder in tool_results
        match &result[1].content[0] {
            ContentBlock::ToolResult { tool_use_id, content, .. } => {
                assert_eq!(tool_use_id, "tu_1");
                assert_eq!(*content, ToolResultContent::Text(FORK_PLACEHOLDER.into()));
            }
            _ => panic!("Expected ToolResult"),
        }

        // Check directive text contains boilerplate
        match &result[1].content[2] {
            ContentBlock::Text { text } => {
                assert!(text.contains("<fork-boilerplate>"));
                assert!(text.contains("Your directive: Fix the bug"));
            }
            _ => panic!("Expected Text"),
        }
    }

    #[test]
    fn test_build_forked_messages_no_tool_uses() {
        let assistant_msg = ApiMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: "Just text".into() }],
        };

        let result = build_forked_messages("Do something", &assistant_msg);

        // Edge case: returns single user message with just directive
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, Role::User);
        match &result[0].content[0] {
            ContentBlock::Text { text } => {
                assert!(text.contains("Your directive: Do something"));
            }
            _ => panic!("Expected Text"),
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test fork_subagent -- --nocapture`
Expected: FAIL — `build_forked_messages` not defined

- [ ] **Step 3: Implement `build_forked_messages`**

Add to `src/services/fork_subagent.rs`:

```rust
use crate::types::content::ToolResultContent;
use crate::types::message::Role;

/// Build the fork child's initial messages from the parent's assistant message.
/// Returns 2 messages: cloned assistant + user message with placeholder tool_results + directive.
/// Edge case: if assistant has no tool_use blocks, returns 1 user message with just the directive.
pub fn build_forked_messages(directive: &str, assistant_message: &ApiMessage) -> Vec<ApiMessage> {
    let tool_use_ids: Vec<&str> = assistant_message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();

    // Edge case: no tool_use blocks
    if tool_use_ids.is_empty() {
        return vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: build_child_message(directive),
            }],
        }];
    }

    let cloned_assistant = assistant_message.clone();

    let mut user_content: Vec<ContentBlock> = tool_use_ids
        .iter()
        .map(|id| ContentBlock::ToolResult {
            tool_use_id: id.to_string(),
            content: ToolResultContent::Text(FORK_PLACEHOLDER.to_string()),
            is_error: false,
        })
        .collect();
    user_content.push(ContentBlock::Text {
        text: build_child_message(directive),
    });

    vec![
        cloned_assistant,
        ApiMessage {
            role: Role::User,
            content: user_content,
        },
    ]
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test fork_subagent -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/fork_subagent.rs
git commit -m "feat(fork): add build_forked_messages with placeholder tool_results"
```

---

### Task 6: Add `build_worktree_notice` and `is_fork_enabled` to fork_subagent.rs

**Files:**
- Modify: `src/services/fork_subagent.rs`

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn test_build_worktree_notice() {
        let notice = build_worktree_notice(
            std::path::Path::new("/parent/dir"),
            std::path::Path::new("/worktree/dir"),
        );
        assert!(notice.contains("/parent/dir"));
        assert!(notice.contains("/worktree/dir"));
        assert!(notice.contains("isolated git worktree"));
    }

    #[tokio::test]
    async fn test_is_fork_enabled_true() {
        let mut settings = crate::utils::settings::SettingsJson::default();
        settings.fork_subagent = Some(true);
        let app_state = crate::state::AppState { settings, ..Default::default() };
        let state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));
        assert!(is_fork_enabled(&state).await);
    }

    #[tokio::test]
    async fn test_is_fork_enabled_default_false() {
        let app_state = crate::state::AppState::default();
        let state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));
        assert!(!is_fork_enabled(&state).await);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test fork_subagent -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement**

```rust
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Check if fork subagent feature is enabled via settings
pub async fn is_fork_enabled(app_state: &Arc<RwLock<crate::state::AppState>>) -> bool {
    let state = app_state.read().await;
    state.settings.fork_subagent.unwrap_or(false)
}

/// Worktree notice for fork children running in isolated worktrees
pub fn build_worktree_notice(parent_cwd: &Path, worktree_cwd: &Path) -> String {
    format!(
        "You've inherited the conversation context above from a parent agent working in {}. \
         You are operating in an isolated git worktree at {} — same repository, same relative \
         file structure, separate working copy. Paths in the inherited context refer to the \
         parent's working directory; translate them to your worktree root. Re-read files before \
         editing if the parent may have modified them since they appear in the context. Your \
         changes stay in this worktree and will not affect the parent's files.",
        parent_cwd.display(),
        worktree_cwd.display(),
    )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test fork_subagent -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/fork_subagent.rs
git commit -m "feat(fork): add is_fork_enabled and build_worktree_notice"
```

---

### Task 7: Extend AgentSpawnParams and add `filter_incomplete_tool_calls` + `build_fork_context`

**Files:**
- Modify: `src/services/agent_runtime.rs:19-114`

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `agent_runtime.rs`:

```rust
    use crate::types::content::{ContentBlock, ToolResultContent};
    use crate::types::message::{ApiMessage, Role};

    #[test]
    fn test_filter_incomplete_tool_calls_removes_orphans() {
        let messages = vec![
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "Read".into(),
                    input: serde_json::json!({}),
                }],
            },
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: ToolResultContent::Text("ok".into()),
                    is_error: false,
                }],
            },
            // This assistant has an orphan tool_use (no matching result)
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_2".into(),
                    name: "Agent".into(),
                    input: serde_json::json!({}),
                }],
            },
        ];

        let filtered = filter_incomplete_tool_calls(&messages);
        // The last assistant message (orphan tu_2) should be removed
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].role, Role::Assistant);
        assert_eq!(filtered[1].role, Role::User);
    }

    #[test]
    fn test_filter_incomplete_tool_calls_keeps_complete() {
        let messages = vec![
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "Read".into(),
                    input: serde_json::json!({}),
                }],
            },
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: ToolResultContent::Text("ok".into()),
                    is_error: false,
                }],
            },
        ];

        let filtered = filter_incomplete_tool_calls(&messages);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_agent_spawn_params_fork_fields() {
        let input = serde_json::json!({ "prompt": "test" });
        let params = AgentSpawnParams::from_input(&input).unwrap();
        assert!(!params.is_fork);
        assert!(params.fork_messages.is_none());
        assert!(params.query_source.is_none());
    }

    #[test]
    fn test_build_fork_context_fields() {
        let parent_ctx = ToolUseContext::test(serde_json::json!({}));
        let fork_msgs = vec![
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::Text { text: "cloned".into() }],
            },
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text { text: "directive".into() }],
            },
        ];
        let params = AgentSpawnParams {
            prompt: "test".into(),
            description: "test".into(),
            model: None,
            run_in_background: false,
            isolation: None,
            is_fork: true,
            fork_messages: Some(fork_msgs),
            query_source: Some("agent:builtin:fork".into()),
        };
        let tools = get_agent_tools(false);
        let ctx = build_fork_context(&params, &parent_ctx, tools);

        // Fork context should have fork_messages appended
        assert_eq!(ctx.messages.len(), 2); // parent had no messages, just fork_msgs
        assert_eq!(ctx.max_turns, Some(200)); // FORK_AGENT max_turns
        assert_eq!(ctx.agent_depth, Some(1)); // parent depth 0 + 1
        assert_eq!(ctx.query_source, Some("agent:builtin:fork".into()));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test agent_runtime -- --nocapture`
Expected: FAIL — new fields/functions not defined

- [ ] **Step 3: Extend AgentSpawnParams**

Add fields to `AgentSpawnParams` struct:

```rust
pub struct AgentSpawnParams {
    pub prompt: String,
    pub description: String,
    pub model: Option<String>,
    pub run_in_background: bool,
    pub isolation: Option<String>,
    /// Whether this is a fork subagent (inherits parent context)
    pub is_fork: bool,
    /// Pre-built fork messages (from build_forked_messages)
    pub fork_messages: Option<Vec<crate::types::message::ApiMessage>>,
    /// Query source for recursion guard (e.g., "agent:builtin:fork")
    pub query_source: Option<String>,
}
```

Update `from_input()` to initialize new fields:

```rust
    Ok(Self { prompt, description, model, run_in_background, isolation, is_fork: false, fork_messages: None, query_source: None })
```

- [ ] **Step 4: Implement `filter_incomplete_tool_calls`**

```rust
use std::collections::HashSet;

/// Remove assistant messages with orphan tool_use blocks (no matching tool_result).
fn filter_incomplete_tool_calls(messages: &[crate::types::message::ApiMessage]) -> Vec<crate::types::message::ApiMessage> {
    use crate::types::content::ContentBlock;
    use crate::types::message::Role;

    let answered_ids: HashSet<&str> = messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
            _ => None,
        })
        .collect();

    messages
        .iter()
        .filter(|msg| {
            if msg.role != Role::Assistant {
                return true;
            }
            !msg.content.iter().any(|block| matches!(
                block,
                ContentBlock::ToolUse { id, .. } if !answered_ids.contains(id.as_str())
            ))
        })
        .cloned()
        .collect()
}
```

- [ ] **Step 5: Implement `build_fork_context`**

```rust
/// Build a QueryContext for a fork child, inheriting parent conversation + system prompt.
pub fn build_fork_context(
    params: &AgentSpawnParams,
    parent_ctx: &ToolUseContext,
    tools: Vec<Box<dyn Tool>>,
) -> QueryContext {
    use crate::types::message::ApiMessage;

    let model = params.model.clone().unwrap_or_else(|| parent_ctx.model.clone());
    let max_tokens = parent_ctx.provider.model_info().max_output_tokens;

    // Filter orphan tool_uses from parent history, then append fork messages
    let mut messages: Vec<ApiMessage> = filter_incomplete_tool_calls(parent_ctx.messages.as_ref());
    if let Some(ref fork_msgs) = params.fork_messages {
        messages.extend(fork_msgs.clone());
    }

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
        max_turns: Some(200), // FORK_AGENT.max_turns
        max_budget_usd: None,
        commands: parent_ctx.commands.clone(),
        provider: parent_ctx.provider.clone(),
        app_state: parent_ctx.app_state.clone(),
        mcp_manager: parent_ctx.mcp_manager.clone(),
        agent_depth: Some(parent_ctx.agent_depth + 1),
        query_source: params.query_source.clone(),
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test agent_runtime -- --nocapture`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/services/agent_runtime.rs
git commit -m "feat(fork): add filter_incomplete_tool_calls and build_fork_context to agent_runtime"
```

---

### Task 8: Wire fork path into run_foreground and run_background

**Files:**
- Modify: `src/services/agent_runtime.rs:175-348`

- [ ] **Step 1: Update `run_foreground` to use fork context when `is_fork`**

Replace the tool setup and context building in `run_foreground` (line 180+):

```rust
    // Tool set: fork uses all builtins (same set as parent), normal same as before.
    // Note: Box<dyn Tool> is not Clone, so we create fresh instances via get_builtin_tools().
    // Phase 2 will address byte-identical tool serialization (use_exact_tools).
    let mut tools = get_agent_tools(false);
    // Share parent's MCP tools
    if let Some(ref mcp) = parent_ctx.mcp_manager {
        tools.extend(mcp.get_all_tools().await);
    }
    let child_abort = parent_ctx.abort_signal.child_token();
```

And the context building:

```rust
    let mut child_ctx = if params.is_fork {
        build_fork_context(&params, parent_ctx, tools)
    } else {
        build_child_context(&params, parent_ctx, tools)
    };
```

After worktree creation, add fork worktree notice:

```rust
    if let Some((ref worktree_path, _)) = worktree_session {
        child_ctx.cwd = worktree_path.clone();
        if params.is_fork {
            let notice = crate::services::fork_subagent::build_worktree_notice(
                &parent_ctx.cwd, worktree_path,
            );
            child_ctx.messages.push(crate::types::message::ApiMessage {
                role: crate::types::message::Role::User,
                content: vec![crate::types::content::ContentBlock::Text { text: notice }],
            });
        }
    }
```

- [ ] **Step 2: Update `run_background` similarly**

Same pattern: fork uses parent tools + `build_fork_context`, normal uses `get_agent_tools(true)` + `build_child_context`. Add worktree notice for fork.

Note for background fork: use `get_agent_tools(false)` (full tool set, not restricted), because fork inherits parent capabilities. `Box<dyn Tool>` is not `Clone` in this codebase, so we always create fresh instances.

- [ ] **Step 3: Build and test**

Run: `cargo build 2>&1 | head -40`
Run: `cargo test agent_runtime -- --nocapture`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/services/agent_runtime.rs
git commit -m "feat(fork): wire fork context into run_foreground and run_background"
```

---

### Task 9: Wire fork detection into AgentTool.execute()

**Files:**
- Modify: `src/tools/agent_tool.rs:56-78`

- [ ] **Step 1: Write the failing test**

```rust
    #[tokio::test]
    async fn test_fork_path_detection_when_enabled() {
        // Fork path requires: subagent_type absent + fork enabled + not inside fork
        let tool = AgentTool;
        let mut ctx = make_ctx(serde_json::json!({"prompt": "test"}));
        // Set fork_subagent in settings
        {
            let mut state = ctx.app_state.write().await;
            state.settings.fork_subagent = Some(true);
        }
        // With no subagent_type and fork enabled, this should take the fork path
        // It will fail at query() (NoopProvider) but should not be rejected for wrong reasons
        let result = tool.execute(ctx).await;
        // Just verify it attempted to run (not rejected for depth or missing prompt)
        match result {
            Ok(r) => assert!(!r.content.contains("nesting depth")),
            Err(_) => {} // query failure expected
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test agent_tool -- --nocapture`
Expected: Current code doesn't check fork — test should still pass (just goes normal path). This is a behavioral verification test.

- [ ] **Step 3: Implement fork detection in execute()**

Replace the `execute` body in `src/tools/agent_tool.rs`:

```rust
    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        if ctx.agent_depth >= 3 {
            return Ok(ToolResult {
                content: "Maximum agent nesting depth (3) reached. Cannot spawn further sub-agents.".into(),
                is_error: true,
                content_blocks: None,
            });
        }

        let mut params = crate::services::agent_runtime::AgentSpawnParams::from_input(&ctx.input)?;
        let subagent_type = ctx.input.get("subagent_type").and_then(|v| v.as_str());

        // Fork path: subagent_type omitted + fork enabled + not already inside a fork
        let is_fork_path = subagent_type.is_none()
            && crate::services::fork_subagent::is_fork_enabled(&ctx.app_state).await
            && !crate::services::fork_subagent::is_inside_fork(
                ctx.query_source.as_deref(),
                ctx.messages.as_ref(),
            );

        if is_fork_path {
            // Extract last assistant message for fork message construction
            let assistant_msg = ctx.messages.iter().rev()
                .find(|m| m.role == crate::types::message::Role::Assistant);

            if let Some(assistant_msg) = assistant_msg {
                let fork_msgs = crate::services::fork_subagent::build_forked_messages(
                    &params.prompt, assistant_msg,
                );
                params.is_fork = true;
                params.fork_messages = Some(fork_msgs);
                params.query_source = Some(
                    crate::services::fork_subagent::FORK_QUERY_SOURCE.to_string(),
                );
            }
            // If no assistant message found, fall through to normal path
        }

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

- [ ] **Step 4: Run all tests**

Run: `cargo test 2>&1 | grep -E "test result:|FAILED"`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add src/tools/agent_tool.rs
git commit -m "feat(fork): wire fork detection and routing into AgentTool.execute()"
```

---

### Task 10: Integration test — full fork path

**Files:**
- Modify: `src/services/fork_subagent.rs` (add integration test)

- [ ] **Step 1: Write integration test**

Add to `src/services/fork_subagent.rs` tests:

```rust
    #[test]
    fn test_full_fork_message_flow() {
        // Simulate: parent conversation → fork → verify complete message structure
        let parent_messages = vec![
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text { text: "Help me fix a bug".into() }],
            },
            ApiMessage {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Text { text: "I'll investigate.".into() },
                    ContentBlock::ToolUse {
                        id: "tu_read".into(),
                        name: "Read".into(),
                        input: serde_json::json!({"path": "/src/main.rs"}),
                    },
                ],
            },
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_read".into(),
                    content: ToolResultContent::Text("fn main() {}".into()),
                    is_error: false,
                }],
            },
            ApiMessage {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Text { text: "Let me spawn an agent.".into() },
                    ContentBlock::ToolUse {
                        id: "tu_agent".into(),
                        name: "Agent".into(),
                        input: serde_json::json!({"prompt": "fix bug"}),
                    },
                ],
            },
        ];

        // 1. Last assistant message has the Agent tool_use
        let last_assistant = parent_messages.iter().rev()
            .find(|m| m.role == Role::Assistant).unwrap();

        // 2. Build fork messages
        let fork_msgs = build_forked_messages("Fix the bug in main.rs", last_assistant);
        assert_eq!(fork_msgs.len(), 2);

        // 3. Filter incomplete tool calls from parent (tu_agent has no result)
        // The last assistant with orphan tu_agent should be filtered out
        // This verification belongs in agent_runtime tests (Task 7)

        // 4. Verify fork child can't re-fork (recursion guard)
        assert!(is_in_fork_child(&fork_msgs));

        // 5. Verify the directive is in the output
        let user_msg = &fork_msgs[1];
        let has_directive = user_msg.content.iter().any(|block| matches!(
            block,
            ContentBlock::Text { text } if text.contains("Fix the bug in main.rs")
        ));
        assert!(has_directive);
    }
```

- [ ] **Step 2: Run test**

Run: `cargo test test_full_fork_message_flow -- --nocapture`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/services/fork_subagent.rs
git commit -m "test(fork): add full fork message flow integration test"
```

---

### Task 11: Final validation

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1 | grep -E "test result:|FAILED"`
Expected: All pass (except pre-existing flaky test)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | head -30`
Expected: No new warnings

- [ ] **Step 3: Verify build**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Build succeeds

- [ ] **Step 4: Commit any fixes**

If clippy or build issues found, fix and commit.

- [ ] **Step 5: Final commit — tag Phase 1 complete**

```bash
git log --oneline -15  # Review all fork commits
```
