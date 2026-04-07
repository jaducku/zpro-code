# Local Command Execution & Real Logic Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make local commands actually execute their `.call()` method and fill in real logic for commands that work without Phase 9 services.

**Architecture:** Fix `dispatch_slash_command` in run.rs to call `.call()` on local commands and handle results. Then update individual command files with real logic. Finally update key TUI commands with real UI.

**Tech Stack:** Rust, async-trait, tokio, ratatui

**Spec:** `docs/superpowers/specs/2026-04-03-local-command-execution-design.md`

---

### Task 1: Fix dispatch pipeline to actually call local commands

**Files:**
- Modify: `src/tui/run.rs`

- [ ] **Step 1: Make `dispatch_slash_command` async and call `.call()`**

The current `FoundCommand::Local` branch shows a generic "(executed)" message instead of calling the command. Change it to:

1. Extract the command name in the first pass (immutable borrow)
2. In the second pass, look up the command again and call `.call()` with a real `CommandContext`
3. Handle the returned `LocalCommandResult`:
   - `Text { value }` → push as system DisplayMessage
   - `Compact { display_text }` → clear messages + show display_text
   - `Skip` → do nothing
4. Special-case: if command name is "exit" or alias "quit"/"q", set `app.should_quit = true`

The function signature changes from `fn dispatch_slash_command(...)` to `async fn dispatch_slash_command(...)`. The call site in `handle_submit` also becomes async (it already runs inside an async block in tokio::select!).

Key change in run.rs — replace the `FoundCommand::Local` match arm:

```rust
FoundCommand::Local { is_exit } => {
    app.push_message(DisplayMessage {
        role: DisplayRole::User,
        content: format!("/{}{}", name, if args.is_empty() { String::new() } else { format!(" {}", args) }),
        tool_name: None,
    });

    // Build a lightweight CommandContext
    use crate::commands::context::{CommandContext, CommandOptions, MessageUpdate, UiEvent};
    use crate::state::AppState;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let (msg_tx, _msg_rx) = tokio::sync::mpsc::unbounded_channel::<MessageUpdate>();
    let (ui_tx, _ui_rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
    let ctx = CommandContext {
        app_state: Arc::new(RwLock::new(AppState::default())),
        abort_signal: tokio_util::sync::CancellationToken::new(),
        cwd: std::env::current_dir().unwrap_or_default(),
        message_tx: msg_tx,
        ui_event_tx: ui_tx,
        options: CommandOptions { theme: "default".to_string() },
    };

    // Call the command
    if let Some(Command::Local(cmd)) = app.command_registry.find(name) {
        match cmd.call(args, &ctx).await {
            Ok(LocalCommandResult::Text { value }) => {
                app.push_message(DisplayMessage {
                    role: DisplayRole::System,
                    content: value,
                    tool_name: None,
                });
            }
            Ok(LocalCommandResult::Compact { display_text }) => {
                app.messages.clear();
                app.api_messages.clear();
                if let Some(text) = display_text {
                    app.push_message(DisplayMessage {
                        role: DisplayRole::System,
                        content: text,
                        tool_name: None,
                    });
                }
            }
            Ok(LocalCommandResult::Skip) => {}
            Err(e) => {
                app.push_message(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("Error: {}", e),
                    tool_name: None,
                });
            }
        }
    }

    if is_exit {
        app.should_quit = true;
    }
}
```

Update `FoundCommand::Local` to track `is_exit` instead of `is_clear`:
```rust
Some(Command::Local(cmd)) => {
    let meta = cmd.meta();
    let is_exit = meta.name == "exit";
    FoundCommand::Local { is_exit }
}
```

Update `FoundCommand` enum:
```rust
enum FoundCommand {
    Local { is_exit: bool },
    Tui,
    Prompt { progress: String },
    NotFound,
}
```

Make `handle_submit` async and update the call site in the event loop.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p atom-code`

- [ ] **Step 3: Test manually**

Run: `cargo run -p atom-code` then type `/exit` — app should quit.
Type `/cost` — should show "Session cost: $0.00 | Tokens: 0 in / 0 out".
Type `/clear` — should clear (ClearCommand.call() handles it via MessageUpdate, but since the channel isn't connected, it may not work — that's OK, run.rs still has the special clear handling in the existing code path. Actually, we removed it. We need to make sure clear still works.)

- [ ] **Step 4: Commit**

```bash
git add src/tui/run.rs
git commit -m "feat(tui): dispatch local commands with actual .call() execution"
```

---

### Task 2: Fill in local command real logic

**Files:**
- Modify: `src/commands/local/exit.rs`
- Modify: `src/commands/local/cost.rs`
- Modify: `src/commands/local/version.rs`
- Modify: `src/commands/local/clear.rs`
- Modify: `src/commands/local/vim.rs`
- Modify: `src/commands/local/copy.rs`

- [ ] **Step 1: Update exit.rs**

Already returns "Exiting..." — this is fine. The special `is_exit` handling in run.rs will set `should_quit`.

No change needed.

- [ ] **Step 2: Update cost.rs to read from CommandContext**

Since `App` state (token_count, session_cost) isn't in `AppState`, the pragmatic approach is to have the command return a placeholder that run.rs replaces. OR — simpler — just update run.rs to inject cost data into the FoundCommand extraction.

Actually, the simplest approach: add `token_count` and `session_cost` to the `FoundCommand::Local` variant, and format the message in run.rs for the `cost` command specifically. But that's hacky.

Better approach: put `token_count` and `session_cost` into `AppState` so CommandContext can access them. Read `src/state/app_state.rs` first to see what's there.

Pragmatic approach for now: just have cost.rs return a formatted string. Run.rs doesn't need to do anything special. The values are hardcoded for now (actual token tracking comes with LLM integration).

Keep cost.rs as-is — it already returns a reasonable stub.

- [ ] **Step 3: Update version.rs**

```rust
async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
    Ok(LocalCommandResult::Text {
        value: format!("atom-code v{}", env!("CARGO_PKG_VERSION")),
    })
}
```

- [ ] **Step 4: Update clear.rs to work without MessageUpdate channel**

Since the MessageUpdate channel isn't connected to anything yet, clear.rs should return a result that run.rs can act on. Use `LocalCommandResult::Compact` instead:

```rust
async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
    Ok(LocalCommandResult::Compact {
        display_text: Some("Conversation cleared.".to_string()),
    })
}
```

This way run.rs's `Compact` handler will clear messages and show the text.

- [ ] **Step 5: Update vim.rs**

```rust
async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
    // Actual vim mode toggle requires Phase 7 VIM module integration
    Ok(LocalCommandResult::Text {
        value: "Vim mode toggled. (Full vim integration pending)".to_string(),
    })
}
```

Already reasonable as-is. No change needed.

- [ ] **Step 6: Update copy.rs**

```rust
async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
    // Clipboard access requires platform-specific APIs
    Ok(LocalCommandResult::Text {
        value: "Copy to clipboard not yet available.".to_string(),
    })
}
```

- [ ] **Step 7: Run tests and verify**

Run: `cargo test -p atom-code -- --include-ignored`
Run: `cargo check -p atom-code`

- [ ] **Step 8: Commit**

```bash
git add src/commands/local/
git commit -m "feat(commands): fill in real logic for local commands (clear, version, exit)"
```

---

### Task 3: Fill in key TUI commands with real UI

**Files:**
- Modify: `src/commands/tui_cmds/status.rs`
- Modify: `src/commands/tui_cmds/model.rs`
- Modify: `src/commands/tui_cmds/plan.rs`

- [ ] **Step 1: Update status.rs with real status display**

Read the current status.rs first, then replace with a real UI showing:
- atom-code version
- Model name (from AppState or hardcoded for now)
- Connection status
- Token count / cost
- Working directory

```rust
pub struct StatusState {
    pub version: String,
    pub model: String,
    pub cwd: String,
}

fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
    Box::new(StatusState {
        version: format!("v{}", env!("CARGO_PKG_VERSION")),
        model: "(from session)".to_string(),
        cwd: ctx.cwd.display().to_string(),
    })
}

fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
    let state = state.downcast_ref::<StatusState>().unwrap();
    let block = Block::default()
        .title(" Status (q to close) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Version:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.version, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Model:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.model, Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("  CWD:      ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.cwd, Style::default().fg(Color::White)),
        ]),
        Line::from(""),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}
```

- [ ] **Step 2: Update model.rs**

Show current model and instructions:

```rust
pub struct ModelState {
    pub current_model: String,
}

fn render(...) {
    // Show current model name, note that changing requires --model flag
    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Current model: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.current_model, Style::default().fg(Color::Green)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Use --model flag to change model.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
}
```

- [ ] **Step 3: Update plan.rs**

Simple toggle message:

```rust
fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
    Box::new(PlanState { enabled: false })
}

fn render(...) {
    let status = if state.enabled { "ON" } else { "OFF" };
    let color = if state.enabled { Color::Green } else { Color::DarkGray };
    // Show plan mode status with toggle instruction
}

fn handle_key(...) {
    match key.code {
        KeyCode::Enter | KeyCode::Char(' ') => {
            state.enabled = !state.enabled;
            CommandAction::Continue
        }
        KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
        _ => CommandAction::Continue,
    }
}
```

- [ ] **Step 4: Run tests and verify**

Run: `cargo check -p atom-code`
Run: `cargo test -p atom-code -- --include-ignored`

- [ ] **Step 5: Commit**

```bash
git add src/commands/tui_cmds/
git commit -m "feat(commands): add real UI for status, model, plan TUI commands"
```
