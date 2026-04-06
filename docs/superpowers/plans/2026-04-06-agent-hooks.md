# Agent Hooks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `HookCommand::Agent` — a multi-turn LLM agent hook that reuses the `query()` engine to verify conditions and return structured `{ok, reason}` results.

**Architecture:** The `execute_agent_hook()` function in `agent_hook.rs` builds a `QueryContext` with DontAsk permissions and a SyntheticOutputTool, runs `query()`, and extracts the StructuredOutput result from messages. The executor's Agent arm delegates to this function.

**Tech Stack:** Rust, tokio, serde_json, jsonschema (via SyntheticOutputTool)

**Spec:** `docs/superpowers/specs/2026-04-06-agent-hooks-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/services/hooks/agent_hook.rs` | `execute_agent_hook()`, `hook_response_schema()`, `AGENT_HOOK_DISALLOWED_TOOLS`, `build_agent_hook_tools()`, `extract_structured_output()` |
| `src/services/hooks/executor.rs` | Wire `HookCommand::Agent` arm to call `execute_agent_hook()` |

---

### Task 1: Add tool filtering and schema helpers to agent_hook.rs

**Files:**
- Modify: `src/services/hooks/agent_hook.rs`

- [ ] **Step 1: Write tests for hook_response_schema() and build_agent_hook_tools()**

Add to the existing `#[cfg(test)] mod tests` block in `agent_hook.rs`:

```rust
#[test]
fn test_hook_response_schema() {
    let schema = super::hook_response_schema();
    // Must be a valid JSON Schema with "ok" required
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["ok"]["type"] == "boolean");
    assert!(schema["properties"]["reason"]["type"] == "string");
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("ok")));
}

#[test]
fn test_build_agent_hook_tools_filters_disallowed() {
    let tools = super::build_agent_hook_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    // Disallowed tools must not be present
    for disallowed in super::AGENT_HOOK_DISALLOWED_TOOLS {
        assert!(
            !names.contains(disallowed),
            "Disallowed tool '{}' should be filtered out",
            disallowed
        );
    }
    // StructuredOutput must be present (from SyntheticOutputTool)
    assert!(names.contains(&"StructuredOutput"), "StructuredOutput tool must be present");
}

#[test]
fn test_build_agent_hook_tools_has_core_tools() {
    let tools = super::build_agent_hook_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    // Core tools should survive filtering
    assert!(names.contains(&"Bash"));
    assert!(names.contains(&"Read"));
    assert!(names.contains(&"Glob"));
    assert!(names.contains(&"Grep"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p atom-code -- hooks::agent_hook::tests --no-run 2>&1 | tail -20`
Expected: compilation errors (functions don't exist yet)

- [ ] **Step 3: Implement hook_response_schema(), AGENT_HOOK_DISALLOWED_TOOLS, build_agent_hook_tools()**

Add above the existing `parse_agent_output` function in `agent_hook.rs`:

```rust
use crate::tool::Tool;
use crate::tools::get_builtin_tools;
use crate::tools::synthetic_output_tool::SyntheticOutputTool;

/// Tools that agent hooks must not use (prevent recursion, deadlocks, irrelevant actions).
pub const AGENT_HOOK_DISALLOWED_TOOLS: &[&str] = &[
    "Agent",
    "AskUserQuestion",
    "EnterPlanMode",
    "ExitPlanMode",
    "EnterWorktree",
    "ExitWorktree",
    "TaskStop",
    "TaskOutput",
    "TeamCreate",
    "TeamDelete",
];

/// JSON Schema for agent hook structured output: `{ok: boolean, reason?: string}`.
pub fn hook_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "ok": { "type": "boolean" },
            "reason": { "type": "string" }
        },
        "required": ["ok"]
    })
}

/// Build the tool set for an agent hook: all builtins minus disallowed, plus SyntheticOutputTool.
pub fn build_agent_hook_tools() -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = get_builtin_tools()
        .into_iter()
        .filter(|t| {
            !AGENT_HOOK_DISALLOWED_TOOLS.contains(&t.name()) && t.name() != "StructuredOutput"
        })
        .collect();
    // SyntheticOutputTool::new should not fail for our well-known schema
    if let Ok(sot) = SyntheticOutputTool::new(hook_response_schema()) {
        tools.push(Box::new(sot));
    }
    tools
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p atom-code -- hooks::agent_hook::tests -v`
Expected: all 5 tests pass (existing 2 + new 3)

- [ ] **Step 5: Commit**

```bash
git add src/services/hooks/agent_hook.rs
git commit -m "feat(hooks): add agent hook tool filtering and schema helpers"
```

---

### Task 2: Add extract_structured_output() helper

**Files:**
- Modify: `src/services/hooks/agent_hook.rs`

- [ ] **Step 1: Write test for extract_structured_output()**

Add to `agent_hook.rs` tests:

```rust
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};

#[test]
fn test_extract_structured_output_found() {
    let messages = vec![
        ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text { text: "verify condition".into() }],
        },
        ApiMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "tu_1".into(),
                name: "StructuredOutput".into(),
                input: serde_json::json!({"ok": true}),
            }],
        },
    ];
    let result = super::extract_structured_output(&messages);
    assert!(result.is_some());
    assert_eq!(result.unwrap()["ok"], true);
}

#[test]
fn test_extract_structured_output_not_found() {
    let messages = vec![
        ApiMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: "I'm done".into() }],
        },
    ];
    assert!(super::extract_structured_output(&messages).is_none());
}

#[test]
fn test_extract_structured_output_takes_last() {
    let messages = vec![
        ApiMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "tu_1".into(),
                name: "StructuredOutput".into(),
                input: serde_json::json!({"ok": false, "reason": "first"}),
            }],
        },
        ApiMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "tu_2".into(),
                name: "StructuredOutput".into(),
                input: serde_json::json!({"ok": true}),
            }],
        },
    ];
    let result = super::extract_structured_output(&messages).unwrap();
    assert_eq!(result["ok"], true);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p atom-code -- hooks::agent_hook::tests --no-run 2>&1 | tail -10`
Expected: compilation error (`extract_structured_output` not found)

- [ ] **Step 3: Implement extract_structured_output()**

Add to `agent_hook.rs`:

```rust
use crate::types::content::ContentBlock;
use crate::types::message::ApiMessage;

/// Scan messages backwards for the last StructuredOutput tool_use input.
/// Returns the `input` JSON value if found.
pub fn extract_structured_output(messages: &[ApiMessage]) -> Option<serde_json::Value> {
    for msg in messages.iter().rev() {
        for block in msg.content.iter().rev() {
            if let ContentBlock::ToolUse { name, input, .. } = block {
                if name == "StructuredOutput" {
                    return Some(input.clone());
                }
            }
        }
    }
    None
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p atom-code -- hooks::agent_hook::tests -v`
Expected: all 8 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/services/hooks/agent_hook.rs
git commit -m "feat(hooks): add extract_structured_output helper"
```

---

### Task 3: Implement execute_agent_hook()

**Files:**
- Modify: `src/services/hooks/agent_hook.rs`

- [ ] **Step 1: Write test for execute_agent_hook() — provider missing**

Add to `agent_hook.rs` tests:

```rust
use std::collections::HashMap;
use crate::services::hooks::types::{HookContext, HookOutcome};

#[tokio::test]
async fn test_execute_agent_hook_no_provider() {
    let env: HashMap<String, String> = HashMap::new();
    let hook_ctx = HookContext { provider: None, registry: None };
    let result = super::execute_agent_hook(
        "check something",
        None,
        None,
        &env,
        &hook_ctx,
    ).await;
    assert!(
        matches!(result.outcome, HookOutcome::NonBlockingError { .. }),
        "Expected NonBlockingError when no provider, got {:?}",
        result.outcome
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p atom-code -- hooks::agent_hook::tests::test_execute_agent_hook_no_provider --no-run 2>&1 | tail -10`
Expected: compilation error (`execute_agent_hook` not found)

- [ ] **Step 3: Implement execute_agent_hook()**

Add to `agent_hook.rs`:

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::commands::registry::CommandRegistry;
use crate::services::api::types::SystemBlock;
use crate::services::hooks::command_hook::HookEnv;
use crate::services::hooks::prompt_hook::build_prompt_input;
use crate::services::hooks::types::{HookContext, HookExecutionResult, HookOutcome};
use crate::query::engine::query;
use crate::query::types::{QueryContext, QueryEvent};
use crate::types::message::{ApiMessage, Role};
use crate::types::permissions::{PermissionMode, ToolPermissionContext};

const AGENT_HOOK_SYSTEM_PROMPT: &str = "\
You are verifying a condition in a coding assistant hook. \
Your task is to verify the given condition using the available tools.\n\n\
Use as few steps as possible - be efficient and direct.\n\n\
When done, return your result using the StructuredOutput tool with:\n\
- ok: true if the condition is met\n\
- ok: false with reason if the condition is not met";

/// Execute an agent-based hook using a multi-turn LLM query.
pub async fn execute_agent_hook(
    prompt: &str,
    model: Option<&str>,
    timeout: Option<u64>,
    env: &HookEnv,
    hook_ctx: &HookContext,
) -> HookExecutionResult {
    // 1. Resolve provider — if model specified, try registry first
    let (provider, model_id) = match model {
        Some(m) => {
            // Try registry for named model resolution
            if let Some(reg) = &hook_ctx.registry {
                match reg.resolve(m) {
                    Ok(p) => (p.clone(), m.to_string()),
                    Err(_) => {
                        // Fall back to default provider with the model name
                        match &hook_ctx.provider {
                            Some(p) => (p.clone(), m.to_string()),
                            None => return HookExecutionResult {
                                outcome: HookOutcome::NonBlockingError {
                                    message: format!("Agent hook: cannot resolve model '{}' (no provider)", m),
                                },
                                additional_contexts: Vec::new(),
                                watch_paths: Vec::new(),
                                hook_specific: None,
                            },
                        }
                    }
                }
            } else {
                match &hook_ctx.provider {
                    Some(p) => (p.clone(), m.to_string()),
                    None => return HookExecutionResult {
                        outcome: HookOutcome::NonBlockingError {
                            message: "Agent hooks require LLM provider (not available)".into(),
                        },
                        additional_contexts: Vec::new(),
                        watch_paths: Vec::new(),
                        hook_specific: None,
                    },
                }
            }
        }
        None => {
            match &hook_ctx.provider {
                Some(p) => {
                    let id = p.model_info().id.clone();
                    (p.clone(), id)
                }
                None => return HookExecutionResult {
                    outcome: HookOutcome::NonBlockingError {
                        message: "Agent hooks require LLM provider (not available)".into(),
                    },
                    additional_contexts: Vec::new(),
                    watch_paths: Vec::new(),
                    hook_specific: None,
                },
            }
        }
    };

    // 2. Build prompt with $ARGUMENTS substitution
    let arguments_json = env.get("ATOM_TOOL_INPUT").cloned().unwrap_or_default();
    let processed_prompt = build_prompt_input(prompt, &arguments_json);

    // 3. Build tools
    let tools = build_agent_hook_tools();

    // 4. Build messages — single user message
    let messages = vec![ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text { text: processed_prompt }],
    }];

    // 5. Build QueryContext with throwaway channels
    let (event_tx, _event_rx) = mpsc::unbounded_channel::<QueryEvent>();
    let (permission_tx, _perm_rx) = mpsc::unbounded_channel();
    let (question_tx, _q_rx) = mpsc::unbounded_channel();
    let abort_signal = CancellationToken::new();

    let ctx = QueryContext {
        messages,
        tools,
        system_prompt: Some(vec![SystemBlock::Text {
            text: AGENT_HOOK_SYSTEM_PROMPT.to_string(),
        }]),
        model: model_id,
        max_tokens: 4096,
        permission_ctx: ToolPermissionContext {
            mode: PermissionMode::DontAsk,
            ..Default::default()
        },
        permission_tx,
        question_tx,
        hooks: HashMap::new(), // No hooks — prevent recursion
        cwd: PathBuf::from(env.get("ATOM_CWD").cloned().unwrap_or_else(|| "/tmp".into())),
        max_turns: Some(MAX_AGENT_HOOK_TURNS as u32),
        max_budget_usd: None,
        commands: Arc::new(CommandRegistry::empty()),
        provider,
        registry: hook_ctx.registry.clone(),
        app_state: Arc::new(tokio::sync::RwLock::new(crate::state::AppState::default())),
        mcp_manager: None,
        agent_depth: None,
        query_source: Some("hook_agent".into()),
    };

    // 6. Execute with timeout
    let timeout_ms = timeout.unwrap_or(60_000);
    let timeout_duration = std::time::Duration::from_millis(timeout_ms);

    let query_result = tokio::time::timeout(
        timeout_duration,
        query(ctx, event_tx, abort_signal),
    )
    .await;

    // 7. Process result
    match query_result {
        Err(_elapsed) => HookExecutionResult {
            outcome: HookOutcome::NonBlockingError {
                message: "Agent hook timed out".into(),
            },
            additional_contexts: Vec::new(),
            watch_paths: Vec::new(),
            hook_specific: None,
        },
        Ok(Err(e)) => HookExecutionResult {
            outcome: HookOutcome::NonBlockingError {
                message: format!("Agent hook query error: {}", e),
            },
            additional_contexts: Vec::new(),
            watch_paths: Vec::new(),
            hook_specific: None,
        },
        Ok(Ok(result)) => {
            match extract_structured_output(&result.messages) {
                Some(output) => match parse_agent_output(&output) {
                    Ok(()) => HookExecutionResult {
                        outcome: HookOutcome::Success,
                        additional_contexts: Vec::new(),
                        watch_paths: Vec::new(),
                        hook_specific: None,
                    },
                    Err(reason) => HookExecutionResult {
                        outcome: HookOutcome::Blocking { reason },
                        additional_contexts: Vec::new(),
                        watch_paths: Vec::new(),
                        hook_specific: None,
                    },
                },
                None => HookExecutionResult {
                    outcome: HookOutcome::NonBlockingError {
                        message: "Agent hook did not return structured output".into(),
                    },
                    additional_contexts: Vec::new(),
                    watch_paths: Vec::new(),
                    hook_specific: None,
                },
            }
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p atom-code -- hooks::agent_hook::tests -v`
Expected: all 9 tests pass (no-provider test returns NonBlockingError)

- [ ] **Step 5: Commit**

```bash
git add src/services/hooks/agent_hook.rs
git commit -m "feat(hooks): implement execute_agent_hook with query() reuse"
```

---

### Task 4: Wire executor Agent arm

**Files:**
- Modify: `src/services/hooks/executor.rs`

- [ ] **Step 1: Write test for agent hook execution through execute_event_hooks()**

Add to the existing `#[cfg(test)] mod tests` block in `executor.rs`:

```rust
#[tokio::test]
async fn test_agent_hook_no_provider_returns_non_blocking_error() {
    let dir = TempDir::new().unwrap();
    let settings = settings_with_hook(
        "PreToolUse",
        None,
        HookCommand::Agent {
            prompt: "check if tests pass".to_string(),
            model: None,
            timeout: None,
            status_message: None,
            once: None,
            r#if: None,
        },
    );
    let result = execute_event_hooks(
        "PreToolUse",
        "Bash",
        &serde_json::json!({}),
        &settings,
        dir.path(),
        &HookContext { provider: None, registry: None },
    )
    .await;
    // Without a provider, agent hook should return NonBlockingError (not panic)
    assert!(
        matches!(result.outcome, HookOutcome::NonBlockingError { .. }),
        "Expected NonBlockingError, got {:?}",
        result.outcome
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p atom-code -- hooks::tests::test_agent_hook_no_provider --no-run 2>&1 | tail -10`
Expected: compiles but test fails — current Agent arm returns NonBlockingError with "not yet implemented" message (this should actually pass since the stub also returns NonBlockingError; verify the message content if needed)

- [ ] **Step 3: Replace the Agent arm in execute_single_hook()**

In `executor.rs`, replace the `HookCommand::Agent { .. }` arm (around line 258-267):

```rust
HookCommand::Agent {
    prompt,
    model,
    timeout,
    ..
} => {
    crate::services::hooks::agent_hook::execute_agent_hook(
        prompt,
        model.as_deref(),
        *timeout,
        env,
        hook_ctx,
    )
    .await
}
```

- [ ] **Step 4: Run all hook tests to verify they pass**

Run: `cargo test -p atom-code -- hooks::tests -v`
Expected: all tests pass including the new agent hook test

- [ ] **Step 5: Run full test suite**

Run: `cargo test -p atom-code 2>&1 | tail -5`
Expected: all tests pass (571+ tests)

- [ ] **Step 6: Commit**

```bash
git add src/services/hooks/executor.rs
git commit -m "feat(hooks): wire Agent arm to execute_agent_hook in executor"
```

---

### Task 5: Add agent hook filtering for SessionStart/Setup events

**Files:**
- Modify: `src/services/hooks/executor.rs`

- [ ] **Step 1: Write test for agent hooks filtered on SessionStart**

Add to `executor.rs` tests:

```rust
#[tokio::test]
async fn test_agent_hooks_filtered_for_session_start() {
    let dir = TempDir::new().unwrap();
    let settings = settings_with_hook(
        "SessionStart",
        None,
        HookCommand::Agent {
            prompt: "check something".to_string(),
            model: None,
            timeout: None,
            status_message: None,
            once: None,
            r#if: None,
        },
    );
    let result = execute_event_hooks(
        "SessionStart",
        "",
        &serde_json::json!({}),
        &settings,
        dir.path(),
        &HookContext { provider: None, registry: None },
    )
    .await;
    // Agent hook should be filtered out for SessionStart, resulting in Success
    assert!(
        matches!(result.outcome, HookOutcome::Success),
        "Expected Success (Agent hook filtered), got {:?}",
        result.outcome
    );
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p atom-code -- hooks::tests::test_agent_hooks_filtered_for_session_start -v`
Expected: PASS — the existing `ALWAYS_EMITTED` filter in `execute_event_hooks` already retains only `HookCommand::Command` for SessionStart/Setup events. Agent hooks are already filtered out by the `matches!(hook, HookCommand::Command { .. })` check on line 335.

- [ ] **Step 3: Commit**

```bash
git add src/services/hooks/executor.rs
git commit -m "test(hooks): verify agent hooks filtered for SessionStart/Setup events"
```

---

### Task 6: Full integration verification

**Files:**
- None (verification only)

- [ ] **Step 1: Run full test suite**

Run: `cargo test -p atom-code 2>&1 | tail -10`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p atom-code 2>&1 | tail -20`
Expected: no new warnings from modified files

- [ ] **Step 3: Verify HookCommand::Agent deserialization works end-to-end**

This is already covered by existing `test_hook_command_deserialize_*` tests in `types.rs`. Verify with:

Run: `cargo test -p atom-code -- hooks::types::tests -v`
Expected: all type tests pass

---

### Deferred: Mock provider integration tests

The spec lists these tests which require a mock `LlmProvider` that returns StructuredOutput tool calls:
- Happy path: `ok=true` → `Success`
- Blocking path: `ok=false` → `Blocking { reason }`
- Timeout path: slow provider → `NonBlockingError("timed out")`
- Max turns exceeded: provider never calls StructuredOutput → `NonBlockingError`

These need a `MockStreamingProvider` that yields pre-built `StreamEvent` sequences including `ToolUse` content blocks. This is significant test infrastructure and can be added as a follow-up once the core implementation is verified working end-to-end.
