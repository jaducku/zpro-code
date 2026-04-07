# Cancellation System & Overlay Positioning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add full cancellation support (Escape/Ctrl+C/Ctrl+D) matching the original TypeScript CLI, and fix inline overlay positioning.

**Architecture:** Wrap `CancellationToken` with `AbortSignal` (reason-aware, lock-free `OnceLock`). Migrate all `CancellationToken` references. Wire TUI key events to abort signals. Add `InterruptBehavior` to Tool trait. Fix overlay to use content-aware height.

**Tech Stack:** Rust, tokio, tokio_util::CancellationToken, ratatui, crossterm

**Spec:** `docs/superpowers/specs/2026-04-07-cancellation-and-overlay-design.md`

---

## Phase 1: Cancellation System

### Task 1: AbortSignal Core Module

**Files:**
- Create: `src/utils/abort_signal.rs`
- Modify: `src/utils/mod.rs:1-3`

- [ ] **Step 1: Write tests for AbortSignal**

```rust
// src/utils/abort_signal.rs — tests at bottom of file

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_signal_not_cancelled() {
        let signal = AbortSignal::new();
        assert!(!signal.is_cancelled());
        assert!(signal.reason().is_none());
    }

    #[test]
    fn test_cancel_sets_reason() {
        let signal = AbortSignal::new();
        signal.cancel(AbortReason::UserCancel);
        assert!(signal.is_cancelled());
        assert_eq!(signal.reason(), Some(&AbortReason::UserCancel));
    }

    #[test]
    fn test_first_cancel_wins() {
        let signal = AbortSignal::new();
        signal.cancel(AbortReason::UserCancel);
        signal.cancel(AbortReason::Interrupt);
        assert_eq!(signal.reason(), Some(&AbortReason::UserCancel));
    }

    #[test]
    fn test_child_cancelled_by_parent() {
        let parent = AbortSignal::new();
        let child = parent.child_signal();
        parent.cancel(AbortReason::UserCancel);
        assert!(child.is_cancelled());
        assert_eq!(child.reason(), Some(&AbortReason::UserCancel));
    }

    #[test]
    fn test_child_cancel_does_not_affect_parent() {
        let parent = AbortSignal::new();
        let child = parent.child_signal();
        child.cancel(AbortReason::SiblingError);
        // Child's token is cancelled but parent's token is NOT
        assert!(child.is_cancelled());
        assert!(!parent.is_cancelled());
    }

    #[tokio::test]
    async fn test_cancelled_future_resolves() {
        let signal = AbortSignal::new();
        let signal2 = signal.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            signal2.cancel(AbortReason::UserCancel);
        });
        signal.cancelled().await;
        assert!(signal.is_cancelled());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib utils::abort_signal -- --nocapture`
Expected: Compilation error (module doesn't exist yet)

- [ ] **Step 3: Implement AbortSignal**

```rust
// src/utils/abort_signal.rs

use std::sync::{Arc, OnceLock};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbortReason {
    /// Escape key — cancel current query, all tools immediately cancelled
    UserCancel,
    /// New message queued while tools running — tools check interruptBehavior
    Interrupt,
    /// Foreground task moved to background
    Background,
    /// Bash tool error propagated to sibling parallel tools
    SiblingError,
}

#[derive(Clone)]
pub struct AbortSignal {
    token: CancellationToken,
    reason: Arc<OnceLock<AbortReason>>,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            reason: Arc::new(OnceLock::new()),
        }
    }

    pub fn cancel(&self, reason: AbortReason) {
        self.reason.set(reason).ok();
        self.token.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub async fn cancelled(&self) {
        self.token.cancelled().await
    }

    pub fn reason(&self) -> Option<&AbortReason> {
        self.reason.get()
    }

    pub fn child_signal(&self) -> AbortSignal {
        AbortSignal {
            token: self.token.child_token(),
            reason: self.reason.clone(),
        }
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Add module to `src/utils/mod.rs`**

Add `pub mod abort_signal;` after line 3.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib utils::abort_signal -- --nocapture`
Expected: All 6 tests pass

- [ ] **Step 6: Commit**

```bash
git add src/utils/abort_signal.rs src/utils/mod.rs
git commit -m "feat: add AbortSignal core with reason-aware cancellation"
```

---

### Task 2: InterruptBehavior on Tool Trait

**Files:**
- Modify: `src/tool.rs:10-73`

- [ ] **Step 1: Add InterruptBehavior enum and trait method**

In `src/tool.rs`, add the enum before the `Tool` trait (before line 12), and add the method to the trait:

```rust
// Add before line 12 (before `pub trait Tool`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterruptBehavior {
    /// Abort on Interrupt reason
    Cancel,
    /// Continue to completion on Interrupt reason
    Block,
}
```

Add to the `Tool` trait (after `search_hint()` at line 73):

```rust
    fn interrupt_behavior(&self) -> InterruptBehavior {
        InterruptBehavior::Block
    }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check 2>&1 | head -20`
Expected: Compiles without errors (default implementation means no tool needs updating yet)

- [ ] **Step 3: Add interrupt_behavior to tools with Cancel behavior**

Override `interrupt_behavior()` in these tool files to return `InterruptBehavior::Cancel`:
- `src/tools/file_read_tool.rs`
- `src/tools/glob_tool.rs`
- `src/tools/grep_tool.rs`
- `src/tools/web_fetch_tool.rs`
- `src/tools/web_search_tool.rs`
- `src/tools/sleep_tool.rs`
- `src/tools/task_create_tool.rs`
- `src/tools/task_update_tool.rs`

In each file, add inside the `impl Tool for XxxTool` block:

```rust
    fn interrupt_behavior(&self) -> crate::tool::InterruptBehavior {
        crate::tool::InterruptBehavior::Cancel
    }
```

All other tools keep the default `Block`.

- [ ] **Step 4: Verify compilation**

Run: `cargo check 2>&1 | head -20`
Expected: Compiles without errors

- [ ] **Step 5: Commit**

```bash
git add src/tool.rs src/tools/file_read_tool.rs src/tools/glob_tool.rs src/tools/grep_tool.rs src/tools/web_fetch_tool.rs src/tools/web_search_tool.rs src/tools/sleep_tool.rs src/tools/task_create_tool.rs src/tools/task_update_tool.rs
git commit -m "feat: add InterruptBehavior to Tool trait with per-tool defaults"
```

---

### Task 3: CancellationToken → AbortSignal Migration

**Files:**
- Modify: `src/query/types.rs:14-39`
- Modify: `src/query/engine.rs:23-27,109-111,439-456`
- Modify: `src/tool.rs:76-102`
- Modify: `src/commands/context.rs:12`
- Modify: `src/state/app_state.rs:10-19`
- Modify: `src/tui/run.rs:443,497,536,703,813`
- Modify: `src/entrypoints/print.rs:105`
- Modify: `src/services/skills/mod.rs:321`
- Modify: `src/services/hooks/agent_hook.rs:151`
- Modify: `src/services/agent_runtime.rs:356,368,416`

- [ ] **Step 1: Add abort_signal to QueryContext**

In `src/query/types.rs`, add field to `QueryContext` struct (after `query_source` field):

```rust
    pub abort_signal: crate::utils::abort_signal::AbortSignal,
```

- [ ] **Step 2: Update query() signature in engine.rs**

In `src/query/engine.rs`, change the `query()` function signature (lines 23-27):

From:
```rust
pub async fn query(
    mut ctx: QueryContext,
    event_tx: mpsc::UnboundedSender<QueryEvent>,
    abort_signal: CancellationToken,
) -> crate::types::Result<QueryResult> {
```

To:
```rust
pub async fn query(
    mut ctx: QueryContext,
    event_tx: mpsc::UnboundedSender<QueryEvent>,
) -> crate::types::Result<QueryResult> {
```

Replace all `abort_signal` references in engine.rs with `ctx.abort_signal`. Key locations:
- Line 109: `if abort_signal.is_cancelled()` → `if ctx.abort_signal.is_cancelled()`
- Line 345: `execute_tool(tc, &tools_arc, &abort_signal, ...)` → `execute_tool(tc, &tools_arc, &ctx, ...)`
- Lines 439-446: Update `execute_tool()` to take `&QueryContext` instead of `&CancellationToken`
- Line 456: `abort_signal: abort_signal.clone()` → `abort_signal: ctx.abort_signal.clone()` (in ToolUseContext)

- [ ] **Step 3: Update ToolUseContext in tool.rs**

In `src/tool.rs` line 79, change:
```rust
pub abort_signal: CancellationToken,
```
To:
```rust
pub abort_signal: crate::utils::abort_signal::AbortSignal,
```

- [ ] **Step 4: Update CommandContext**

In `src/commands/context.rs` line 12, change:
```rust
pub abort_signal: CancellationToken,
```
To:
```rust
pub abort_signal: crate::utils::abort_signal::AbortSignal,
```

- [ ] **Step 5: Update BackgroundTask**

In `src/state/app_state.rs` line 16, change:
```rust
pub cancel_token: CancellationToken,
```
To:
```rust
pub cancel_token: crate::utils::abort_signal::AbortSignal,
```

- [ ] **Step 6: Update all CancellationToken::new() call sites**

In `src/tui/run.rs`, replace all `tokio_util::sync::CancellationToken::new()` with `crate::utils::abort_signal::AbortSignal::new()`:
- Lines 443, 497, 536, 703, 813

Also update the `query()` calls to remove the cancel parameter:
- Line 708: `query(ctx, event_tx.clone(), cancel)` → `query(ctx, event_tx.clone())`
- Line 816: same change

In `src/entrypoints/print.rs` line 105:
- `tokio_util::sync::CancellationToken::new()` → `crate::utils::abort_signal::AbortSignal::new()`
- Update `query()` call to remove cancel param

In `src/services/hooks/agent_hook.rs` line 151:
- `CancellationToken::new()` → `AbortSignal::new()`
- Add `abort_signal: abort_signal.clone(),` to the QueryContext construction (around line 153)
- Update `query()` call to remove cancel param (line 188)

**All QueryContext construction sites must include the new `abort_signal` field:**
- `src/tui/run.rs` submit_query (line ~676): `abort_signal: abort_signal.clone(),`
- `src/tui/run.rs` submit_query_skill (line ~786): `abort_signal: abort_signal.clone(),`
- `src/services/hooks/agent_hook.rs` (line ~153): `abort_signal: abort_signal.clone(),`
- `src/entrypoints/print.rs` (line ~107): `abort_signal: cancel.clone(),` (after renaming to AbortSignal)
- `src/services/agent_runtime.rs` foreground_agent (line ~270): `abort_signal: child_ctx.abort_signal.child_signal(),`
- `src/services/agent_runtime.rs` background_agent (line ~390): `abort_signal: AbortSignal::new(),`

In `src/services/agent_runtime.rs` line 356:
- `CancellationToken::new()` → `AbortSignal::new()`
- Line 368: `cancel_token: cancel_token.clone()` stays the same (field name unchanged)
- Line 416: `query(child_ctx, event_tx.clone(), cancel_token)` → `query(child_ctx, event_tx.clone())`

In `src/services/skills/mod.rs` line 321:
- `abort_signal: CancellationToken::new()` → `abort_signal: AbortSignal::new()`

- [ ] **Step 7: Fix all remaining compilation errors**

Run: `cargo check 2>&1`

Fix any remaining references to `CancellationToken` that need updating. The `.cancelled()` and `.is_cancelled()` methods have the same signatures on `AbortSignal`, so tool `tokio::select!` patterns should compile without changes.

- [ ] **Step 8: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All existing tests pass

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor: migrate CancellationToken to AbortSignal across codebase"
```

---

### Task 4: QueryCancelled Event & Partial Output Preservation

**Files:**
- Modify: `src/query/types.rs:84-123` (QueryEvent enum)
- Modify: `src/query/engine.rs:108-111` (cancellation handling)

- [ ] **Step 1: Add QueryCancelled variant to QueryEvent**

In `src/query/types.rs`, add to the `QueryEvent` enum (after `BriefMessage` variant, around line 122):

```rust
    QueryCancelled {
        partial_messages: Vec<crate::types::message::ApiMessage>,
        reason: crate::utils::abort_signal::AbortReason,
    },
```

- [ ] **Step 2: Update query engine cancellation to emit QueryCancelled**

In `src/query/engine.rs`, replace the abort check block (lines 109-111):

From:
```rust
if ctx.abort_signal.is_cancelled() {
    return Err(crate::types::AtomError::Cancelled);
}
```

To:
```rust
if ctx.abort_signal.is_cancelled() {
    // Preserve partial output as assistant message
    if !text_content.is_empty() {
        let partial_msg = crate::types::message::ApiMessage {
            role: crate::types::message::Role::Assistant,
            content: vec![crate::types::content::ContentBlock::Text {
                text: text_content.clone(),
            }],
        };
        ctx.messages.push(partial_msg);
    }
    let reason = ctx.abort_signal.reason().cloned()
        .unwrap_or(crate::utils::abort_signal::AbortReason::UserCancel);
    let _ = event_tx.send(QueryEvent::QueryCancelled {
        partial_messages: ctx.messages.clone(),
        reason,
    });
    return Err(crate::types::AtomError::Cancelled);
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check 2>&1 | head -20`
Expected: Compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src/query/types.rs src/query/engine.rs
git commit -m "feat: add QueryCancelled event with partial output preservation"
```

---

### Task 5: TUI Key Bindings — Escape, Ctrl+C, Ctrl+D

**Files:**
- Modify: `src/tui/app.rs:19-74` (App struct fields)
- Modify: `src/tui/app.rs:144-161` (InputMode enum)
- Modify: `src/tui/app.rs:164-219` (App::new)
- Modify: `src/tui/app.rs:221-232` (handle_key)
- Modify: `src/tui/run.rs:111-264` (event loop key handling)
- Modify: `src/tui/run.rs:596-720` (submit_query)

- [ ] **Step 1: Add new fields to App struct**

In `src/tui/app.rs`, add to the `App` struct (after `hit_areas` field, around line 73):

```rust
    /// Active query's abort signal (Some only while query is running)
    pub active_abort_signal: Option<crate::utils::abort_signal::AbortSignal>,
    /// Ctrl+C double-press detection timer
    pub last_ctrl_c_press: Option<Instant>,
    /// Ctrl+D double-press detection timer
    pub last_ctrl_d_press: Option<Instant>,
```

- [ ] **Step 2: Add ExitConfirm to InputMode**

In `src/tui/app.rs`, add to the `InputMode` enum (after `Question` variant):

```rust
    ExitConfirm {
        worktree_name: String,
        uncommitted_count: usize,
        selected: usize,
    },
```

- [ ] **Step 3: Initialize new fields in App::new()**

In `src/tui/app.rs`, add to the `Self { ... }` block in `App::new()`:

```rust
            active_abort_signal: None,
            last_ctrl_c_press: None,
            last_ctrl_d_press: None,
```

- [ ] **Step 4: Remove Ctrl+C/D from handle_key()**

In `src/tui/app.rs`, remove lines 224-228 (the Ctrl+C/D → `should_quit = true` block):

From:
```rust
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('d') => {
                    self.should_quit = true;
                }
                _ => {}
            }
            return false;
        }
```

To:
```rust
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            // Ctrl+C/D handled in run.rs event loop (cancel/exit flow)
            return false;
        }
```

- [ ] **Step 5: Store abort signal in submit_query**

In `src/tui/run.rs`, in the `submit_query` function (around line 703), change:

From:
```rust
    let cancel = crate::utils::abort_signal::AbortSignal::new();
```

To:
```rust
    let abort_signal = crate::utils::abort_signal::AbortSignal::new();
    app.active_abort_signal = Some(abort_signal.clone());
```

And update the QueryContext construction to include `abort_signal`:

```rust
    abort_signal: abort_signal.clone(),
```

Apply the same pattern to `submit_query_skill` (around line 813).

- [ ] **Step 6: Add Ctrl+C/D/Escape handling to event loop**

In `src/tui/run.rs`, in the key event handling section (before the Question/Permission checks, around line 121), add a new early check for Ctrl+C, Ctrl+D, and Escape:

```rust
Event::Key(key) => {
    // === Ctrl+C: interrupt or double-press exit ===
    if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
        && key.code == KeyCode::Char('c')
    {
        if app.is_loading {
            // Cancel running query
            if let Some(ref signal) = app.active_abort_signal {
                signal.cancel(crate::utils::abort_signal::AbortReason::Interrupt);
            }
            // Kill background agents
            {
                let state = app_state.read().await;
                for task in state.background_tasks.values() {
                    task.cancel_token.cancel(
                        crate::utils::abort_signal::AbortReason::Interrupt,
                    );
                }
            }
            // Dismiss permission/question prompts
            match std::mem::replace(&mut app.input_mode, InputMode::Normal) {
                InputMode::Permission { reply_tx, .. } => {
                    if let Some(tx) = reply_tx {
                        let _ = tx.send(PermissionReply::Deny);
                    }
                }
                InputMode::Question { reply_tx, .. } => {
                    if let Some(tx) = reply_tx {
                        let _ = tx.send(UserQuestionReply::Dismissed);
                    }
                }
                other => { app.input_mode = other; }
            }
        } else {
            // Double-press exit logic
            let now = Instant::now();
            if let Some(last) = app.last_ctrl_c_press {
                if now.duration_since(last).as_millis() < 800 {
                    // Check worktree
                    let has_worktree = {
                        let state = app_state.read().await;
                        state.current_worktree_session.is_some()
                    };
                    if has_worktree {
                        let state = app_state.read().await;
                        if let Some(ref session) = state.current_worktree_session {
                            let name = session.worktree_name.clone();
                            let count = crate::services::worktree::WorktreeService::count_uncommitted(&session.worktree_path).await.unwrap_or(0);
                            drop(state);
                            app.input_mode = InputMode::ExitConfirm {
                                worktree_name: name,
                                uncommitted_count: count,
                                selected: 0,
                            };
                        }
                    } else {
                        app.should_quit = true;
                    }
                    app.last_ctrl_c_press = None;
                } else {
                    app.last_ctrl_c_press = Some(now);
                    app.status = "Press Ctrl+C again to exit".into();
                }
            } else {
                app.last_ctrl_c_press = Some(now);
                app.status = "Press Ctrl+C again to exit".into();
            }
        }
        continue; // skip further key handling
    }

    // === Ctrl+D: double-press exit only ===
    if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
        && key.code == KeyCode::Char('d')
    {
        let now = Instant::now();
        if let Some(last) = app.last_ctrl_d_press {
            if now.duration_since(last).as_millis() < 800 {
                // Same exit flow as Ctrl+C double-press
                let has_worktree = {
                    let state = app_state.read().await;
                    state.current_worktree_session.is_some()
                };
                if has_worktree {
                    let state = app_state.read().await;
                    if let Some(ref session) = state.current_worktree_session {
                        let name = session.worktree_name.clone();
                        let count = crate::services::worktree::WorktreeService::count_uncommitted(&session.worktree_path).await.unwrap_or(0);
                        drop(state);
                        app.input_mode = InputMode::ExitConfirm {
                            worktree_name: name,
                            uncommitted_count: count,
                            selected: 0,
                        };
                    }
                } else {
                    app.should_quit = true;
                }
                app.last_ctrl_d_press = None;
            } else {
                app.last_ctrl_d_press = Some(now);
                app.status = "Press Ctrl+D again to exit".into();
            }
        } else {
            app.last_ctrl_d_press = Some(now);
            app.status = "Press Ctrl+D again to exit".into();
        }
        continue;
    }

    // === Escape: cancel query ===
    if key.code == KeyCode::Esc
        && !matches!(app.input_mode, InputMode::Permission { .. })
        && !matches!(app.input_mode, InputMode::Question { .. })
        && !matches!(app.input_mode, InputMode::ExitConfirm { .. })
    {
        if app.is_loading {
            if let Some(ref signal) = app.active_abort_signal {
                signal.cancel(crate::utils::abort_signal::AbortReason::UserCancel);
            }
            continue;
        }
        // Fall through to existing typeahead/command close
    }

    // ... existing Question/Permission/Command handling follows ...
```

- [ ] **Step 7: Handle QueryCancelled event in event loop**

In `src/tui/run.rs`, in the `query_event_rx.recv()` arm (around line 281), add handling for `QueryCancelled` before the existing `handle_query_event` call:

```rust
Some(event) = query_event_rx.recv() => {
    // Handle cancellation separately
    if let QueryEvent::QueryCancelled { partial_messages, reason } = &event {
        use crate::utils::abort_signal::AbortReason;
        app.api_messages = partial_messages.clone();
        if *reason != AbortReason::Interrupt {
            app.push_message(DisplayMessage::Text {
                role: TextRole::System,
                content: "[Request interrupted by user]".into(),
            });
            app.api_messages.push(ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "[Request interrupted by user]".into(),
                }],
            });
        }
        app.is_loading = false;
        app.active_abort_signal = None;
        app.loading_label = None;
    } else {
        let is_response_complete = matches!(event, QueryEvent::ResponseComplete { .. });
        app.handle_query_event(event);
        // ... existing loading label cleanup ...
    }
}
```

- [ ] **Step 8: Handle ExitConfirm input mode**

In `src/tui/run.rs`, add ExitConfirm key handling in the key event section (after the Permission handling block):

```rust
else if let InputMode::ExitConfirm { selected, .. } = &mut app.input_mode {
    match key.code {
        KeyCode::Left => { *selected = selected.saturating_sub(1); }
        KeyCode::Right => { *selected = (*selected + 1).min(2); }
        KeyCode::Enter => {
            if let InputMode::ExitConfirm { worktree_name: _, uncommitted_count: _, selected } =
                std::mem::replace(&mut app.input_mode, InputMode::Normal)
            {
                match selected {
                    0 => { // Keep & exit
                        app.should_quit = true;
                    }
                    1 => { // Remove & exit
                        let state = app_state.read().await;
                        if let Some(ref session) = state.current_worktree_session {
                            let session_clone = session.clone();
                            drop(state);
                            let _ = crate::services::worktree::WorktreeService::remove(&session_clone).await;
                            let mut state = app_state.write().await;
                            state.current_worktree_session = None;
                        }
                        app.should_quit = true;
                    }
                    _ => {} // Cancel — already reset to Normal
                }
            }
        }
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
        }
        _ => {}
    }
}
```

- [ ] **Step 9: Add QueryCancelled match arm to handle_query_event**

In `src/tui/app.rs`, find the `handle_query_event` method (around line 548) and add a match arm for `QueryCancelled` so the match is exhaustive:

```rust
QueryEvent::QueryCancelled { .. } => {
    // Handled in run.rs event loop before reaching here
}
```

- [ ] **Step 10: Reset abort signal on QueryCompleted**

In `src/tui/run.rs`, in the `QueryEvent::QueryCompleted` handling, add:

```rust
app.active_abort_signal = None;
```

- [ ] **Step 11: Verify compilation**

Run: `cargo check 2>&1 | head -30`
Expected: Compiles without errors

- [ ] **Step 12: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 13: Commit**

```bash
git add src/tui/app.rs src/tui/run.rs
git commit -m "feat: add Escape/Ctrl+C/Ctrl+D key bindings with cancel and exit flow"
```

---

### Task 6: ExitConfirm Dialog Rendering

**Files:**
- Modify: `src/tui/ui.rs` (render function)

- [ ] **Step 1: Add ExitConfirm rendering**

In `src/tui/ui.rs`, in the `render_input` function, add a branch for `InputMode::ExitConfirm` that renders the dialog options as a styled line in the input area:

```rust
InputMode::ExitConfirm { worktree_name, uncommitted_count, selected } => {
    let options = ["Keep & exit", "Remove & exit", "Cancel"];
    let spans: Vec<Span> = options.iter().enumerate().map(|(i, opt)| {
        if i == *selected {
            Span::styled(format!(" [{}] ", opt), Style::default().fg(Color::Black).bg(Color::Yellow))
        } else {
            Span::styled(format!("  {}  ", opt), Style::default().fg(Color::Gray))
        }
    }).collect();
    let title = format!("Worktree '{}' ({} uncommitted files). Exit?", worktree_name, uncommitted_count);
    // Render title line and options line
}
```

Exact rendering depends on the existing `render_input` structure — adapt to match the current layout pattern.

- [ ] **Step 2: Verify compilation and visual check**

Run: `cargo check 2>&1 | head -10`
Then manual test: run the app, enter a worktree, press Ctrl+C twice to see the dialog.

- [ ] **Step 3: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat: render ExitConfirm dialog for worktree cleanup on exit"
```

---

### Task 7: InterruptBehavior Dispatch Logic

**Files:**
- Modify: `src/query/engine.rs` (execute_tool and abort check)

- [ ] **Step 1: Add reason-aware cancellation check in the tool execution path**

In `src/query/engine.rs`, the abort check at line 109 cancels ALL tools regardless of their `InterruptBehavior`. Modify the check to be reason-aware.

Replace the abort check (the one modified in Task 4) to check behavior:

```rust
if ctx.abort_signal.is_cancelled() {
    let reason = ctx.abort_signal.reason().cloned()
        .unwrap_or(crate::utils::abort_signal::AbortReason::UserCancel);
    
    match reason {
        crate::utils::abort_signal::AbortReason::Interrupt => {
            // Only break for Cancel-behavior tools; Block tools continue
            // This check is at the outer loop level, so we break the tool loop
            // Individual tool execution handles its own tokio::select! abort
        }
        _ => {
            // UserCancel, Background, SiblingError → always cancel
            if !text_content.is_empty() {
                let partial_msg = crate::types::message::ApiMessage {
                    role: crate::types::message::Role::Assistant,
                    content: vec![crate::types::content::ContentBlock::Text {
                        text: text_content.clone(),
                    }],
                };
                ctx.messages.push(partial_msg);
            }
            let _ = event_tx.send(QueryEvent::QueryCancelled {
                partial_messages: ctx.messages.clone(),
                reason,
            });
            return Err(crate::types::AtomError::Cancelled);
        }
    }
}
```

- [ ] **Step 2: Add per-tool interrupt behavior check in execute_tool**

In `execute_tool()`, before executing the tool, check if the abort signal is cancelled with `Interrupt` reason and the tool's behavior is `Block`. If so, let it proceed:

```rust
// At the start of execute_tool, before tool.execute():
if ctx.abort_signal.is_cancelled() {
    if let Some(crate::utils::abort_signal::AbortReason::Interrupt) = ctx.abort_signal.reason() {
        // Find the tool and check its behavior
        if let Some(tool) = tools.iter().find(|t| t.name() == tc.name) {
            if tool.interrupt_behavior() == crate::tool::InterruptBehavior::Block {
                // Let this tool complete — don't abort
            } else {
                return Err(crate::types::AtomError::Cancelled);
            }
        }
    } else {
        return Err(crate::types::AtomError::Cancelled);
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check 2>&1 | head -20`
Expected: Compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src/query/engine.rs
git commit -m "feat: implement InterruptBehavior dispatch in tool execution"
```

---

### Task 8: Permission Prompt Abort Signal Integration

**Files:**
- Modify: `src/query/engine.rs` (permission await)

- [ ] **Step 1: Wrap permission await with abort signal**

In `src/query/engine.rs`, find the permission reply await (around line 282 where `reply_rx.await` is called). Wrap it in `tokio::select!`:

```rust
let reply = tokio::select! {
    reply = reply_rx => reply.unwrap_or(PermissionReply::Deny),
    _ = ctx.abort_signal.cancelled() => PermissionReply::Deny,
};
```

Apply the same pattern to the question reply await if it exists.

- [ ] **Step 2: Verify compilation**

Run: `cargo check 2>&1 | head -10`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add src/query/engine.rs
git commit -m "fix: wrap permission/question await with abort signal to prevent hang"
```

---

## Phase 2: Inline Overlay Positioning

### Task 9: Content-Aware Overlay Positioning

**Files:**
- Modify: `src/tui/ui.rs:16-26` (compute_command_overlay_area)
- Modify: `src/commands/traits.rs:75-80` (TuiCommand trait)
- Modify: `src/tui/run.rs:95-102` (render call site)

- [ ] **Step 1: Add content_height to TuiCommand trait**

In `src/commands/traits.rs`, add to the `TuiCommand` trait (after `handle_key` method, line 79):

```rust
    fn content_height(&self, state: &dyn std::any::Any) -> u16 {
        8
    }
```

- [ ] **Step 2: Update compute_command_overlay_area signature**

In `src/tui/ui.rs`, change the function (lines 16-26):

From:
```rust
pub fn compute_command_overlay_area(terminal_area: Rect) -> Rect {
    let input_top = terminal_area.y + terminal_area.height.saturating_sub(4);
    let max_height = terminal_area.height / 2;
    let height = max_height.max(8).min(terminal_area.height.saturating_sub(6));
    let width = terminal_area.width.min(80);
    let x = terminal_area.x + (terminal_area.width.saturating_sub(width)) / 2;
    let y = input_top.saturating_sub(height);
    Rect { x, y, width, height }
}
```

To:
```rust
pub fn compute_command_overlay_area(terminal_area: Rect, content_height: u16) -> Rect {
    let input_top = terminal_area.y + terminal_area.height.saturating_sub(4);
    let max_height = terminal_area.height / 2;
    let height = content_height.min(max_height).max(1);
    let y = input_top.saturating_sub(height);
    let width = terminal_area.width.min(80);
    let x = terminal_area.x + (terminal_area.width.saturating_sub(width)) / 2;
    Rect { x, y, width, height }
}
```

- [ ] **Step 3: Update render call site**

In `src/tui/run.rs` (around lines 95-102), change:

From:
```rust
let overlay_area = ui::compute_command_overlay_area(frame.area());
```

To:
```rust
let height = tui_cmd.content_height(active.state.as_ref());
let overlay_area = ui::compute_command_overlay_area(frame.area(), height);
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check 2>&1 | head -10`
Expected: Compiles (all TUI commands use the default `content_height` of 8)

- [ ] **Step 5: Implement content_height for key overlay commands**

Implement `content_height()` for the most important overlay commands that have dynamic content. Focus on the ones users see most:

**model.rs** — item count + 2 (border):
```rust
fn content_height(&self, state: &dyn std::any::Any) -> u16 {
    if let Some(state) = state.downcast_ref::<ModelState>() {
        (state.models.len() as u16 + 2).min(20)
    } else { 8 }
}
```

Apply similar pattern to `skills.rs`, `session.rs`, `output_style.rs`, and other list-based commands.

Commands without dynamic content can keep the default of 8.

- [ ] **Step 6: Verify compilation and visual check**

Run: `cargo check 2>&1 | head -10`
Manual test: run the app, try `/model` to verify the overlay sits above the input.

- [ ] **Step 7: Commit**

```bash
git add src/tui/ui.rs src/tui/run.rs src/commands/traits.rs src/commands/tui_cmds/
git commit -m "feat: content-aware overlay positioning anchored above input area"
```

---

## Final Verification

### Task 10: Full Build & Test

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1 | tail -30`
Expected: All tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | tail -20`
Expected: No warnings in changed files

- [ ] **Step 3: Manual smoke test**

1. Start the app
2. Send a query, press Escape mid-stream → verify partial output preserved, "[Request interrupted by user]" shown
3. Send a query, press Ctrl+C → verify interrupt works, background agents killed
4. While idle, press Ctrl+C twice → verify exit flow (or ExitConfirm if worktree active)
5. Try `/model` → verify overlay positioned above input
6. Send another query after cancellation → verify context preserved from partial output

- [ ] **Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: address issues found in smoke testing"
```
