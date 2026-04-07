# Cancellation System & Inline Overlay Positioning

**Date**: 2026-04-07
**Status**: Draft

## Overview

Two independent improvements to the TUI:

1. **Phase 1 — Cancellation System**: Full abort/cancel support matching the original TypeScript Claude Code CLI. Escape cancels the current query, Ctrl+C interrupts (kills background agents) with double-press-to-exit, partial output is preserved, and tools define interrupt behavior.

2. **Phase 2 — Inline Overlay Positioning**: Fix overlay positioning so command overlays (model picker, skills, etc.) fit snugly above the input area with content-aware height instead of a fixed half-screen block.

---

## Phase 1: Cancellation System

### 1.1 AbortSignal Core

**File**: `src/utils/abort_signal.rs` (new)

#### AbortReason enum

```rust
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
```

#### AbortSignal struct

```rust
use std::sync::{Arc, OnceLock};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct AbortSignal {
    token: CancellationToken,
    reason: Arc<OnceLock<AbortReason>>,
}
```

**Design choice**: `CancellationToken` + `Arc<OnceLock<AbortReason>>`.

- `CancellationToken`: battle-tested tokio cancellation primitive, supports hierarchical child tokens
- `OnceLock`: lock-free, set-once semantics — first cancel's reason wins, no race conditions, no mutex poisoning
- Ordering: call `reason.set()` before `token.cancel()` on the same thread. The CancellationToken's internal atomic guarantees that any thread observing cancellation will also see the previously-set reason

#### API

```rust
impl AbortSignal {
    /// Create a new independent signal.
    pub fn new() -> Self;

    /// Cancel with a reason. First call wins (OnceLock).
    /// Sets reason, then cancels token.
    pub fn cancel(&self, reason: AbortReason);

    /// Check if cancelled (lock-free AtomicBool read).
    pub fn is_cancelled(&self) -> bool;

    /// Await cancellation (tokio intrinsic, cancel-safe).
    pub async fn cancelled(&self);

    /// Get the cancellation reason (lock-free read after init).
    pub fn reason(&self) -> Option<&AbortReason>;

    /// Create a child signal sharing the same reason OnceLock.
    /// Parent cancel → child cancelled. Child cancel does NOT affect parent.
    pub fn child_signal(&self) -> AbortSignal;
}
```

**Why shared OnceLock for parent-child**: In the query→tool hierarchy, only the UI sets the reason on the query signal. Tools read it. Sibling controllers and background agents use separate `AbortSignal::new()` trees, so no cross-contamination.

### 1.2 Existing CancellationToken Migration

Move `abort_signal` into `QueryContext` (currently a separate parameter to `query()`). Replace `CancellationToken` with `AbortSignal` in all locations:

| Location | Current | Change |
|----------|---------|--------|
| `src/query/types.rs` QueryContext | No abort_signal field | Add `abort_signal: AbortSignal` |
| `src/query/engine.rs:26` query() | `abort_signal: CancellationToken` param | Remove param, use `ctx.abort_signal` |
| `src/query/engine.rs:442` execute_tool() | `abort_signal: CancellationToken` param | Use AbortSignal |
| `src/tool.rs:79` ToolUseContext | `abort_signal: CancellationToken` | → `AbortSignal` |
| `src/commands/context.rs:12` CommandContext | `abort_signal: CancellationToken` | → `AbortSignal` |
| `src/state/app_state.rs:16` BackgroundTask | `cancel_token: CancellationToken` | → `AbortSignal` |
| `src/tui/run.rs:443,497,536,703,813` | `CancellationToken::new()` | → `AbortSignal::new()` |
| `src/entrypoints/print.rs:105` | `CancellationToken` | → `AbortSignal` |
| `src/services/skills/mod.rs:313-321` | `CancellationToken` | → `AbortSignal` |
| `src/services/hooks/agent_hook.rs:151` | `CancellationToken` | → `AbortSignal` |
| `src/services/agent_runtime.rs:356` | `CancellationToken` | → `AbortSignal` |

All `tokio::select!` patterns in tools remain unchanged (same `.cancelled()` interface).
`submit_query()` / `submit_query_skill()` in `run.rs`: clone and store `AbortSignal` in `app.active_abort_signal` before passing to spawned task.

### 1.3 TUI Key Bindings

#### Escape (`chat:cancel`)

```
Escape pressed
  → is_loading == true && active_abort_signal.is_some()?
    → Yes: abort_signal.cancel(UserCancel)
    → No:  close typeahead if open (existing behavior)
```

Inactive when:
- Permission prompt active (`InputMode::Permission`)
- Question prompt active (`InputMode::Question`)
- ExitConfirm dialog active

#### Ctrl+C (`app:interrupt`)

```
Ctrl+C pressed
  → is_loading == true?
    → Yes: abort_signal.cancel(Interrupt)
           Kill ALL background agents (iterate app_state.background_tasks, cancel each)
           If permission prompt is active: send PermissionReply::Deny through reply_tx
           If question prompt is active: reject pending prompts
    → No:
      → last_ctrl_c_press within 800ms?
        → Yes: initiate exit flow (ExitFlow or immediate quit)
        → No:  record timestamp, show "Press Ctrl+C again to exit"
```

When `app:interrupt` handles the event (query was running), it returns early — does NOT trigger double-press-to-exit.

**Edge case — permission/question prompt during Ctrl+C**: The query engine awaits `reply_rx` at `engine.rs:282` for permission prompts. When Ctrl+C fires, the permission await must be wrapped in `tokio::select!` with the abort signal so the engine doesn't hang waiting for a reply that will never come.

#### Ctrl+D (`app:exit`)

Same double-press logic as Ctrl+C (800ms window), but no interrupt behavior — purely exit.

**Migration note**: The existing `app.handle_key()` in `app.rs:224-229` (Ctrl+C/D → `should_quit = true`) must be removed. All Ctrl+C/D handling moves to the event loop in `run.rs`.

### 1.4 Tool interruptBehavior

#### Trait extension

```rust
// src/tool.rs (where the Tool trait lives)

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterruptBehavior {
    Cancel,  // Abort on Interrupt reason
    Block,   // Continue to completion on Interrupt reason
}

pub trait Tool: Send + Sync {
    // ... existing methods ...

    fn interrupt_behavior(&self) -> InterruptBehavior {
        InterruptBehavior::Block  // default: complete the operation
    }
}
```

#### Reason-based dispatch

| AbortReason | InterruptBehavior::Cancel | InterruptBehavior::Block |
|-------------|--------------------------|-------------------------|
| UserCancel  | Cancel immediately        | Cancel immediately       |
| Interrupt   | Cancel immediately        | Wait for completion      |
| SiblingError| Cancel (same batch only)  | Cancel (same batch only) |
| Background  | Cancel immediately        | Cancel immediately       |

UserCancel and Background override behavior — always cancel regardless.

#### Per-tool defaults

| Tool | Behavior | Rationale |
|------|----------|-----------|
| BashTool | Block | Long-running, interrupting mid-execution risks state inconsistency |
| ReadTool | Cancel | Read-only, safe to abort |
| GlobTool | Cancel | Read-only, safe to abort |
| GrepTool | Cancel | Read-only, safe to abort |
| EditTool | Block | File modification, partial write risk |
| WriteTool | Block | File modification, partial write risk |
| WebFetchTool | Cancel | Network request, safe to abort |
| WebSearchTool | Cancel | Network request, safe to abort |
| SleepTool | Cancel | Just waiting, no side effects |
| AgentTool | Block | Preserve sub-agent work |
| TaskCreateTool | Cancel | Safe to abort |
| TaskUpdateTool | Cancel | Safe to abort |
| NotebookEditTool | Block | File modification |

All tools not listed above default to `InterruptBehavior::Block` (the trait default).

### 1.5 Partial Output Preservation & Interruption Message

#### New QueryEvent variant

```rust
pub enum QueryEvent {
    // ... existing variants ...

    /// Query was cancelled. Carries partial messages and reason.
    QueryCancelled {
        partial_messages: Vec<ApiMessage>,
        reason: AbortReason,
    },
}
```

#### Query engine cancellation handling

When `abort_signal.is_cancelled()` detected in the query loop:

1. Collect partial assistant message (streamed text so far) into `partial_messages`
2. Send `QueryEvent::QueryCancelled { partial_messages, reason }` via event channel
3. Return `Err(AtomError::Cancelled)`

#### TUI event handling

```rust
QueryEvent::QueryCancelled { partial_messages, reason } => {
    // 1. Sync api_messages with partial output
    app.api_messages.extend(partial_messages);

    // 2. Add interruption message (skip for Interrupt reason)
    if reason != AbortReason::Interrupt {
        app.messages.push(DisplayMessage::Text {
            role: TextRole::System,
            content: "[Request interrupted by user]".into(),
        });
        // Also add to api_messages so LLM has context
        app.api_messages.push(ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "[Request interrupted by user]".into(),
            }],
        });
    }

    // 3. Reset state
    app.is_loading = false;
    app.active_abort_signal = None;
}
```

### 1.6 ExitFlow Dialog

#### Trigger

Double-press Ctrl+C or Ctrl+D within 800ms while idle (no query running).

#### Behavior

```
worktree active (app_state.current_worktree_session.is_some())?
  → Yes: Show ExitConfirm dialog
         - Display worktree name and uncommitted file count
         - Options: [Keep worktree & exit] / [Remove worktree & exit] / [Cancel]
  → No:  Quit immediately (app.should_quit = true)
```

#### InputMode variant

```rust
pub enum InputMode {
    // ... existing variants ...

    ExitConfirm {
        worktree_name: String,
        uncommitted_count: usize,
        selected: usize,  // 0=Keep, 1=Remove, 2=Cancel
    },
}
```

- **Keep**: `app.should_quit = true` (worktree remains on disk)
- **Remove**: `WorktreeService::remove()` then `app.should_quit = true`
- **Cancel** / **Escape**: Return to `InputMode::Normal`

### 1.7 App State Additions

```rust
pub struct App {
    // ... existing fields ...

    /// Active query's abort signal (Some only while query is running)
    pub active_abort_signal: Option<AbortSignal>,

    /// Ctrl+C double-press detection timer
    pub last_ctrl_c_press: Option<Instant>,

    /// Ctrl+D double-press detection timer
    pub last_ctrl_d_press: Option<Instant>,
}
```

---

## Phase 2: Inline Overlay Positioning

### 2.1 Problem

`compute_command_overlay_area()` uses a fixed height (`terminal_area.height / 2`) regardless of content. Overlays float in the middle of the screen instead of sitting snugly above the input area.

### 2.2 Solution

#### Updated compute function

```rust
// src/tui/ui.rs

pub fn compute_command_overlay_area(terminal_area: Rect, content_height: u16) -> Rect {
    // Input (3 rows) + status bar (1 row) = 4 rows from bottom
    let input_top = terminal_area.y + terminal_area.height.saturating_sub(4);

    // Fit to content, capped at half screen
    let max_height = terminal_area.height / 2;
    let height = content_height.min(max_height).max(1);

    // Anchor to input area top edge
    let y = input_top.saturating_sub(height);
    let width = terminal_area.width.min(80);
    let x = terminal_area.x + (terminal_area.width.saturating_sub(width)) / 2;

    Rect { x, y, width, height }
}
```

#### TuiCommand trait extension

```rust
pub trait TuiCommand {
    // ... existing methods ...

    /// Content height needed for overlay (rows). Default: 8.
    fn content_height(&self, state: &dyn std::any::Any) -> u16 {
        8
    }
}
```

#### Render call site update

```rust
// src/tui/run.rs
let height = tui_cmd.content_height(active.state.as_ref());
let overlay_area = ui::compute_command_overlay_area(frame.area(), height);
frame.render_widget(Clear, overlay_area);
tui_cmd.render(frame, overlay_area, active.state.as_mut());
```

#### Per-command content_height implementations

Each TUI command (model picker, skill picker, etc.) implements `content_height()` based on its actual item count + borders/padding.

---

## Phase Dependencies

```
Phase 1 and Phase 2 are fully independent — can be implemented in any order or in parallel.

Phase 1 internal order:
  1.1 AbortSignal core
  → 1.2 CancellationToken migration
  → 1.3 TUI key bindings (depends on 1.1, 1.2)
  → 1.4 interruptBehavior (depends on 1.1)
  → 1.5 Partial output preservation (depends on 1.2, 1.3)
  → 1.6 ExitFlow dialog (depends on 1.3)
```

## Files Affected

### Phase 1
| File | Change |
|------|--------|
| `src/utils/abort_signal.rs` | New — AbortSignal, AbortReason |
| `src/utils/mod.rs` | Add `pub mod abort_signal` |
| `src/query/types.rs` | Add `abort_signal: AbortSignal` to QueryContext |
| `src/query/engine.rs` | Remove abort_signal param, use ctx.abort_signal, emit QueryCancelled |
| `src/query/mod.rs` | Add `QueryCancelled` variant to `QueryEvent` |
| `src/tui/app.rs` | Add fields, Escape/Ctrl+C/D handling, ExitConfirm |
| `src/tui/run.rs` | Store abort signal, handle QueryCancelled, double-press timer |
| `src/tool.rs` | Add `InterruptBehavior`, `interrupt_behavior()` to Tool trait |
| `src/tools/*.rs` | Implement `interrupt_behavior()` per tool |
| `src/services/agent_runtime.rs` | Migrate to AbortSignal |
| `src/types/error.rs` | Ensure `AtomError::Cancelled` exists |
| `src/entrypoints/print.rs` | Migrate to AbortSignal |
| `src/services/skills/mod.rs` | Migrate to AbortSignal |
| `src/services/hooks/agent_hook.rs` | Migrate to AbortSignal |

### Phase 2
| File | Change |
|------|--------|
| `src/tui/ui.rs` | `compute_command_overlay_area` add content_height param |
| `src/tui/run.rs` | Pass content_height at render call site |
| `src/commands/traits.rs` | Add `content_height()` to TuiCommand trait |
| `src/commands/*.rs` | Implement `content_height()` per command |
