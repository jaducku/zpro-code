# Prompt Hooks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire prompt hooks to actually call the LLM via a HookContext struct, replacing the current stub.

**Architecture:** Add `HookContext { provider }` to types.rs, thread it through execute_event_hooks → execute_single_hook. Implement `query_without_streaming` to collect a MessageStream into text. Strengthen `parse_prompt_response` to extract JSON from LLM preamble text.

**Tech Stack:** Rust, tokio, futures::StreamExt, async_trait

**Spec:** `docs/superpowers/specs/2026-04-06-prompt-hooks-design.md`

---

### Task 1: Add HookContext and update signatures

**Files:**
- Modify: `src/services/hooks/types.rs`
- Modify: `src/services/hooks/executor.rs:65-69` (execute_single_hook signature)
- Modify: `src/services/hooks/executor.rs:238-244` (execute_event_hooks signature)
- Modify: `src/services/hooks/executor.rs:289-291` (join_all call)
- Modify: `src/services/hooks/executor.rs:325-573` (7 tests)

- [ ] **Step 1: Add HookContext to types.rs**

At the end of `src/services/hooks/types.rs`, add:

```rust
use std::sync::Arc;

/// Runtime context for hook execution.
/// Provides LLM provider access for Prompt/Agent hooks.
pub struct HookContext {
    pub provider: Option<Arc<dyn crate::services::api::provider::LlmProvider>>,
}
```

- [ ] **Step 2: Update execute_single_hook signature**

In `src/services/hooks/executor.rs:65`, change:

```rust
async fn execute_single_hook(
    hook: &HookCommand,
    env: &HookEnv,
    cwd: &Path,
    hook_ctx: &HookContext,
) -> HookExecutionResult {
```

Add `HookContext` to the import at line 6-8:

```rust
use crate::services::hooks::types::{
    HookCommand, HookContext, HookExecutionResult, HookOutcome, HooksSettings, SyncHookOutput,
};
```

- [ ] **Step 3: Update execute_event_hooks signature**

In `src/services/hooks/executor.rs:238`, change:

```rust
pub async fn execute_event_hooks(
    event: &str,
    match_query: &str,
    input: &serde_json::Value,
    settings: &HooksSettings,
    cwd: &Path,
    hook_ctx: &HookContext,
) -> HookExecutionResult {
```

- [ ] **Step 4: Pass hook_ctx through to execute_single_hook**

In `src/services/hooks/executor.rs:289-291`, update the map closure:

```rust
    let futures: Vec<_> = matching_hooks
        .iter()
        .map(|hook| execute_single_hook(hook, &env, cwd, hook_ctx))
        .collect();
```

- [ ] **Step 5: Expand ALWAYS_EMITTED filter to Prompt/Agent hooks**

In `src/services/hooks/executor.rs:271-274`, replace:

```rust
    if ALWAYS_EMITTED.contains(&event) {
        matching_hooks.retain(|hook| matches!(hook, HookCommand::Command { .. }));
    }
```

- [ ] **Step 6: Update all 5 async test functions**

Each test that calls `execute_event_hooks()` needs `&HookContext { provider: None }` as the last argument. The 5 tests are:
- `test_execute_hooks_no_matching` (line 349)
- `test_execute_hooks_matching_allows` (line 377)
- `test_execute_hooks_blocking_result` (line 405)
- `test_execute_hooks_matcher_filters` (line 438)
- `test_execute_hooks_json_block` (line 470)
- `test_http_hooks_filtered_for_session_start` (line 558)

Add to each: `&HookContext { provider: None },` after `dir.path(),`

Also add the import in the test module:

```rust
use crate::services::hooks::types::HookContext;
```

- [ ] **Step 7: Update mod.rs re-export if needed**

Check `src/services/hooks/mod.rs` — `pub use types::*` already covers HookContext. No change needed.

- [ ] **Step 8: Verify compilation and tests**

Run: `cargo test services::hooks -- --nocapture 2>&1 | tail -20`
Expected: All existing tests pass

- [ ] **Step 9: Commit**

```bash
git add src/services/hooks/types.rs src/services/hooks/executor.rs
git commit -m "feat(hooks): add HookContext struct and thread through hook execution pipeline"
```

---

### Task 2: Update engine.rs call sites

**Files:**
- Modify: `src/query/engine.rs:288-295` (PreToolUse hook call)
- Modify: `src/query/engine.rs:322-328` (PostToolUse hook call)

- [ ] **Step 1: Create HookContext before the tool loop**

In `src/query/engine.rs`, inside the `query()` function, before the tool execution loop (find `for tc in &tool_calls {`), add:

```rust
        let hook_ctx = crate::services::hooks::HookContext {
            provider: Some(ctx.provider.clone()),
        };
```

- [ ] **Step 2: Add hook_ctx to PreToolUse call**

Find the `execute_event_hooks("PreToolUse", ...` call and add `&hook_ctx` as last argument:

```rust
                let hook_result = execute_event_hooks(
                    "PreToolUse",
                    &tc.name,
                    &tc.input,
                    &ctx.hooks,
                    &ctx.cwd,
                    &hook_ctx,
                ).await;
```

- [ ] **Step 3: Add hook_ctx to PostToolUse call**

Same pattern for `execute_event_hooks("PostToolUse", ...`:

```rust
                let hook_result = execute_event_hooks(
                    "PostToolUse",
                    &tc.name,
                    &tc.input,
                    &ctx.hooks,
                    &ctx.cwd,
                    &hook_ctx,
                ).await;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 5: Commit**

```bash
git add src/query/engine.rs
git commit -m "feat(hooks): pass HookContext with provider to hook execution in query engine"
```

---

### Task 3: Implement query_without_streaming

**Files:**
- Modify: `src/services/hooks/prompt_hook.rs`

- [ ] **Step 1: Add query_without_streaming function**

Add to `src/services/hooks/prompt_hook.rs`:

```rust
use futures::StreamExt;

use crate::services::api::provider::LlmProvider;
use crate::services::api::types::{ContentDelta, CreateMessageRequest, StreamEvent};
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};

/// Send a single prompt to the LLM and collect the full response text.
/// Consumes the entire MessageStream, collecting TextDelta events.
pub async fn query_without_streaming(
    provider: &dyn LlmProvider,
    prompt: &str,
    model: Option<&str>,
    timeout_ms: u64,
) -> Result<String, String> {
    let model_id = model
        .map(|m| m.to_string())
        .unwrap_or_else(|| provider.model_info().id.clone());

    let request = CreateMessageRequest {
        model: model_id,
        messages: vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: prompt.to_string(),
            }],
        }],
        max_tokens: 4096,
        system: None,
        tools: None,
        temperature: None,
        stream: true,
    };

    let stream_result = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        async {
            let mut stream = provider
                .create_message(request)
                .await
                .map_err(|e| format!("LLM call failed: {e}"))?;

            let mut text = String::new();
            while let Some(event) = stream.next().await {
                match event {
                    Ok(StreamEvent::ContentBlockDelta {
                        delta: ContentDelta::TextDelta { text: t },
                        ..
                    }) => {
                        text.push_str(&t);
                    }
                    Ok(_) => {} // ignore other events
                    Err(e) => return Err(format!("Stream error: {e}")),
                }
            }
            Ok(text)
        },
    )
    .await;

    match stream_result {
        Ok(inner) => inner,
        Err(_) => Err("Prompt hook timed out".to_string()),
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 3: Commit**

```bash
git add src/services/hooks/prompt_hook.rs
git commit -m "feat(hooks): add query_without_streaming helper for prompt hooks"
```

---

### Task 4: Strengthen parse_prompt_response with JSON extraction

**Files:**
- Modify: `src/services/hooks/prompt_hook.rs`

- [ ] **Step 1: Replace parse_prompt_response and add check_ok_field**

Replace the existing `parse_prompt_response` function and add helper:

```rust
/// Check the "ok" field in a parsed JSON value.
fn check_ok_field(v: &serde_json::Value) -> Result<(), String> {
    match v.get("ok").and_then(|v| v.as_bool()) {
        Some(true) => Ok(()),
        _ => {
            let reason = v
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("Blocked by prompt hook");
            Err(reason.to_string())
        }
    }
}

/// Parse a prompt hook response JSON. Expects `{"ok": true/false, "reason": "..."}`.
///
/// Handles LLM responses that include preamble text before the JSON:
/// 1. Tries parsing the full response as JSON
/// 2. Falls back to scanning each line for a JSON object starting with `{`
///
/// Returns:
/// - `Ok(())` if `ok=true`
/// - `Err(reason)` if `ok=false` (policy block) or JSON not found (parse error)
pub fn parse_prompt_response(response: &str) -> Result<(), String> {
    // 1st: try parsing the entire response
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
        return check_ok_field(&v);
    }

    // 2nd: scan lines for JSON object
    for line in response.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return check_ok_field(&v);
            }
        }
    }

    Err(format!(
        "Could not parse JSON from response: {}",
        &response[..response.len().min(200)]
    ))
}
```

- [ ] **Step 2: Update existing tests and add new ones**

Replace the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_hook_input() {
        let template = "Review this tool call: $ARGUMENTS";
        let args = r#"{"tool":"Bash","command":"rm -rf /"}"#;
        let result = build_prompt_input(template, args);
        assert_eq!(
            result,
            r#"Review this tool call: {"tool":"Bash","command":"rm -rf /"}"#
        );
    }

    #[test]
    fn test_parse_prompt_hook_response_ok() {
        let response = r#"{"ok": true}"#;
        assert!(parse_prompt_response(response).is_ok());
    }

    #[test]
    fn test_parse_prompt_hook_response_blocked() {
        let response = r#"{"ok": false, "reason": "dangerous"}"#;
        let err = parse_prompt_response(response).unwrap_err();
        assert_eq!(err, "dangerous");
    }

    #[test]
    fn test_parse_prompt_response_with_preamble() {
        let response = "Here is my analysis:\n\n{\"ok\": false, \"reason\": \"unsafe command\"}\n";
        let err = parse_prompt_response(response).unwrap_err();
        assert_eq!(err, "unsafe command");
    }

    #[test]
    fn test_parse_prompt_response_preamble_ok() {
        let response = "I've reviewed the tool call.\n{\"ok\": true}\nDone.";
        assert!(parse_prompt_response(response).is_ok());
    }

    #[test]
    fn test_parse_prompt_response_no_json() {
        let response = "I'm not sure what to do here.";
        let err = parse_prompt_response(response).unwrap_err();
        assert!(err.starts_with("Could not parse JSON"));
    }

    #[test]
    fn test_parse_prompt_response_default_reason() {
        let response = r#"{"ok": false}"#;
        let err = parse_prompt_response(response).unwrap_err();
        assert_eq!(err, "Blocked by prompt hook");
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test services::hooks::prompt_hook -- --nocapture 2>&1 | tail -15`
Expected: All 7 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/services/hooks/prompt_hook.rs
git commit -m "feat(hooks): strengthen parse_prompt_response with JSON extraction from LLM preamble"
```

---

### Task 5: Implement Prompt hook execution in executor

**Files:**
- Modify: `src/services/hooks/executor.rs:189-196` (HookCommand::Prompt match arm)

- [ ] **Step 1: Replace the Prompt stub**

In `src/services/hooks/executor.rs`, replace the `HookCommand::Prompt { .. }` arm (lines 189-196) with:

```rust
        HookCommand::Prompt {
            prompt,
            model,
            timeout,
            ..
        } => {
            let Some(provider) = &hook_ctx.provider else {
                return HookExecutionResult {
                    outcome: HookOutcome::NonBlockingError {
                        message: "Prompt hooks require LLM provider (not available)".into(),
                    },
                    additional_contexts: Vec::new(),
                    watch_paths: Vec::new(),
                    hook_specific: None,
                };
            };

            let arguments_json = env.get("ATOM_TOOL_INPUT").cloned().unwrap_or_default();
            let filled_prompt =
                crate::services::hooks::prompt_hook::build_prompt_input(prompt, &arguments_json);

            let timeout_ms = timeout.unwrap_or(30_000);
            let response = crate::services::hooks::prompt_hook::query_without_streaming(
                provider.as_ref(),
                &filled_prompt,
                model.as_deref(),
                timeout_ms,
            )
            .await;

            match response {
                Ok(text) => {
                    match crate::services::hooks::prompt_hook::parse_prompt_response(&text) {
                        Ok(()) => HookExecutionResult {
                            outcome: HookOutcome::Success,
                            additional_contexts: Vec::new(),
                            watch_paths: Vec::new(),
                            hook_specific: None,
                        },
                        Err(reason) => {
                            if reason.starts_with("Could not parse JSON") {
                                HookExecutionResult {
                                    outcome: HookOutcome::NonBlockingError { message: reason },
                                    additional_contexts: Vec::new(),
                                    watch_paths: Vec::new(),
                                    hook_specific: None,
                                }
                            } else {
                                HookExecutionResult {
                                    outcome: HookOutcome::Blocking { reason },
                                    additional_contexts: Vec::new(),
                                    watch_paths: Vec::new(),
                                    hook_specific: None,
                                }
                            }
                        }
                    }
                }
                Err(err) => HookExecutionResult {
                    outcome: HookOutcome::NonBlockingError { message: err },
                    additional_contexts: Vec::new(),
                    watch_paths: Vec::new(),
                    hook_specific: None,
                },
            }
        }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 3: Run all hook tests**

Run: `cargo test services::hooks -- --nocapture 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add src/services/hooks/executor.rs
git commit -m "feat(hooks): implement Prompt hook execution with LLM call"
```

---

### Task 6: Full verification

**Files:**
- No new files

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass (except pre-existing team_create_tool failure)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | tail -20`
Expected: No new errors

- [ ] **Step 3: Verify build**

Run: `cargo build 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 4: Final commit if fixups needed**

```bash
git add -A
git commit -m "fix: address clippy and test issues in prompt hooks"
```
