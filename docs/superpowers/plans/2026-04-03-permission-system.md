# Permission System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add permission checking before tool execution with 5 modes, rule matching, TUI dialog, and channel-based query engine integration.

**Architecture:** Permission types already exist in `src/types/permissions.rs`. Add checker logic in `src/utils/permissions/`, integrate into `query/engine.rs` via permission channels, and activate the existing TUI permission dialog.

**Tech Stack:** tokio (channels, oneshot), globset (pattern matching), crossterm/ratatui (dialog)

**Spec:** `docs/superpowers/specs/2026-04-03-permission-system-design.md`

---

### Task 1: Permission checker — core logic

**Files:**
- Create: `src/utils/permissions/checker.rs`
- Create: `src/utils/permissions/auto_classify.rs`
- Create: `src/utils/permissions/mod.rs`
- Modify: `src/utils/mod.rs`

- [ ] **Step 1: Write tests for check_permission**

Tests in `checker.rs`:
- Bypass mode → always Allow
- Plan mode → always Deny
- Deny rule matches → Deny
- Allow rule matches → Allow
- AcceptEdits + Edit tool → Allow
- AcceptEdits + Bash tool → Ask
- Auto mode + Read tool → Allow
- Auto mode + Bash(git status) → Allow
- Auto mode + Bash(rm -rf) → Ask
- Default mode → always Ask

- [ ] **Step 2: Implement checker.rs**

```rust
// src/utils/permissions/checker.rs
use crate::types::permissions::*;

pub fn check_permission(
    tool_name: &str,
    tool_input: &serde_json::Value,
    _permission_meta: &PermissionMeta,
    ctx: &ToolPermissionContext,
) -> PermissionDecision {
    // 1. Bypass
    if matches!(ctx.mode, PermissionMode::BypassPermissions | PermissionMode::DontAsk) {
        return PermissionDecision::Allow { reason: "bypass mode".into() };
    }

    // 2. Plan
    if matches!(ctx.mode, PermissionMode::Plan) {
        return PermissionDecision::Deny {
            reason: "Plan mode: tool execution blocked".into(),
        };
    }

    // 3. Deny rules
    if matches_any_rule(tool_name, tool_input, &ctx.always_deny_rules) {
        return PermissionDecision::Deny {
            reason: "Denied by rule".into(),
        };
    }

    // 4. Allow rules
    if matches_any_rule(tool_name, tool_input, &ctx.always_allow_rules) {
        return PermissionDecision::Allow { reason: "Allowed by rule".into() };
    }

    // 5. AcceptEdits
    if matches!(ctx.mode, PermissionMode::AcceptEdits) {
        if is_edit_tool(tool_name) {
            return PermissionDecision::Allow { reason: "acceptEdits mode".into() };
        }
    }

    // 6. Auto
    if matches!(ctx.mode, PermissionMode::Auto) {
        match auto_classify(tool_name, tool_input) {
            PermissionBehavior::Allow => {
                return PermissionDecision::Allow { reason: "auto-classified safe".into() };
            }
            PermissionBehavior::Deny => {
                return PermissionDecision::Deny { reason: "auto-classified dangerous".into() };
            }
            PermissionBehavior::Ask => {} // fall through to ask
        }
    }

    // 7. Ask
    let description = format_tool_description(tool_name, tool_input);
    PermissionDecision::Ask { tool_name: tool_name.into(), description }
}

fn is_edit_tool(name: &str) -> bool {
    matches!(name, "Edit" | "Write" | "NotebookEdit")
}

fn matches_any_rule(tool_name: &str, input: &serde_json::Value, rules: &[PermissionRule]) -> bool {
    rules.iter().any(|rule| {
        if rule.tool_name != tool_name { return false; }
        match &rule.rule_content {
            None => true, // tool name match only
            Some(pattern) => match_rule_content(pattern, input),
        }
    })
}

fn match_rule_content(pattern: &str, input: &serde_json::Value) -> bool {
    // Simple glob matching: "git:*" matches command starting with "git"
    let content = input.get("command")
        .or_else(|| input.get("file_path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    
    if pattern.ends_with(":*") {
        let prefix = &pattern[..pattern.len() - 2];
        content.starts_with(prefix)
    } else if pattern.ends_with("*") {
        let prefix = &pattern[..pattern.len() - 1];
        content.starts_with(prefix)
    } else {
        content == pattern
    }
}

fn format_tool_description(tool_name: &str, input: &serde_json::Value) -> String {
    match tool_name {
        "Bash" => format!("Bash: {}", input["command"].as_str().unwrap_or("(unknown)")),
        "Edit" | "Write" => format!("{}: {}", tool_name, input["file_path"].as_str().unwrap_or("(unknown)")),
        "Read" => format!("Read: {}", input["file_path"].as_str().unwrap_or("(unknown)")),
        _ => tool_name.to_string(),
    }
}
```

- [ ] **Step 3: Implement auto_classify.rs**

```rust
// src/utils/permissions/auto_classify.rs
use crate::types::permissions::PermissionBehavior;

pub fn auto_classify(tool_name: &str, input: &serde_json::Value) -> PermissionBehavior {
    match tool_name {
        "Read" | "Glob" | "Grep" => PermissionBehavior::Allow,
        "Bash" => {
            let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
            if is_readonly_bash(cmd) { PermissionBehavior::Allow } else { PermissionBehavior::Ask }
        }
        _ => PermissionBehavior::Ask,
    }
}

fn is_readonly_bash(cmd: &str) -> bool {
    let cmd_trimmed = cmd.trim();
    let readonly_prefixes = [
        "git status", "git log", "git diff", "git branch", "git show",
        "git remote", "git tag", "git rev-parse", "git ls-files",
        "ls", "cat ", "head ", "tail ", "find ", "grep ", "rg ",
        "which ", "where ", "pwd", "echo ", "date", "wc ", "sort ",
        "uniq ", "env", "printenv", "uname", "file ", "stat ",
        "du ", "df ", "whoami", "id ", "hostname",
        "cargo check", "cargo test", "cargo clippy", "cargo fmt --check",
        "npm test", "npx ", "node -e", "python -c", "python3 -c",
    ];
    readonly_prefixes.iter().any(|p| cmd_trimmed.starts_with(p))
}
```

- [ ] **Step 4: Create mod.rs and add PermissionDecision type**

```rust
// src/utils/permissions/mod.rs
pub mod checker;
pub mod auto_classify;

pub use checker::check_permission;
pub use auto_classify::auto_classify;

/// Result of a permission check (richer than types::PermissionResult)
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    Allow { reason: String },
    Ask { tool_name: String, description: String },
    Deny { reason: String },
}
```

Add `pub mod permissions;` to `src/utils/mod.rs`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p atom-code utils::permissions`

- [ ] **Step 6: Commit**

```bash
git add src/utils/permissions/ src/utils/mod.rs
git commit -m "feat(permissions): add permission checker with 5 modes and auto-classify"
```

---

### Task 2: Permission channels and query engine integration

**Files:**
- Modify: `src/query/engine.rs`
- Modify: `src/query/types.rs` or `src/query/mod.rs`

- [ ] **Step 1: Add permission types to query module**

Read `src/query/mod.rs` and `src/query/types.rs` first. Add:

```rust
use tokio::sync::{mpsc, oneshot};
use crate::types::permissions::ToolPermissionContext;

/// Permission request sent from query engine to TUI
pub struct PermissionAsk {
    pub tool_use_id: String,
    pub tool_name: String,
    pub description: String,
    pub reply_tx: oneshot::Sender<PermissionReply>,
}

/// User's reply to a permission ask
pub enum PermissionReply {
    Allow,
    AllowAlways { tool_name: String, rule_content: Option<String> },
    Deny,
}
```

Add `permission_ctx: ToolPermissionContext` and `permission_tx: mpsc::UnboundedSender<PermissionAsk>` to `QueryContext`.

- [ ] **Step 2: Integrate permission check into tool execution loop**

In `query/engine.rs`, replace the tool execution loop with permission-aware version:

```rust
for tc in &tool_calls {
    let tool = find_tool_by_name(&ctx.tools, &tc.name);
    let meta = tool.map(|t| t.permission_metadata(&tc.input))
        .unwrap_or_default();
    
    let decision = crate::utils::permissions::check_permission(
        &tc.name, &tc.input, &meta, &ctx.permission_ctx,
    );

    let result = match decision {
        PermissionDecision::Allow { .. } => {
            execute_tool(tc, &ctx.tools, &abort_signal).await
        }
        PermissionDecision::Deny { reason } => {
            Ok(crate::tool::ToolResult {
                content: format!("Permission denied: {}", reason),
                is_error: true,
            })
        }
        PermissionDecision::Ask { tool_name, description } => {
            // Send ask to TUI, wait for reply
            let (reply_tx, reply_rx) = oneshot::channel();
            let _ = ctx.permission_tx.send(PermissionAsk {
                tool_use_id: tc.id.clone(),
                tool_name,
                description,
                reply_tx,
            });
            match reply_rx.await {
                Ok(PermissionReply::Allow) => {
                    execute_tool(tc, &ctx.tools, &abort_signal).await
                }
                Ok(PermissionReply::AllowAlways { tool_name, rule_content }) => {
                    ctx.permission_ctx.always_allow_rules.push(
                        crate::types::permissions::PermissionRule {
                            tool_name,
                            rule_content,
                        }
                    );
                    execute_tool(tc, &ctx.tools, &abort_signal).await
                }
                Ok(PermissionReply::Deny) | Err(_) => {
                    Ok(crate::tool::ToolResult {
                        content: "Permission denied by user".into(),
                        is_error: true,
                    })
                }
            }
        }
    };
    // ... rest of tool result handling (existing code)
}
```

- [ ] **Step 3: Update submit_query in run.rs to create permission channel and context**

In `src/tui/run.rs`, `submit_query` needs to create the permission channel and pass `ToolPermissionContext`:

```rust
let (permission_tx, permission_rx) = mpsc::unbounded_channel::<PermissionAsk>();

let ctx = QueryContext {
    // existing fields...
    permission_ctx: ToolPermissionContext {
        mode: /* from CLI flag or default */,
        ..Default::default()
    },
    permission_tx,
};
```

Store `permission_rx` so the event loop can receive asks.

- [ ] **Step 4: Run `cargo check -p atom-code`**

- [ ] **Step 5: Commit**

```bash
git add src/query/ src/tui/run.rs
git commit -m "feat(permissions): integrate permission checking into query engine"
```

---

### Task 3: TUI permission dialog

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/run.rs`

- [ ] **Step 1: Update PermissionDialog in app.rs**

Read current `app.rs` PermissionDialog struct. Update to include reply channel and 3 choices:

```rust
pub struct PermissionDialog {
    pub tool_name: String,
    pub description: String,
    pub selected: usize,  // 0=Allow, 1=AlwaysAllow, 2=Deny
    pub reply_tx: Option<tokio::sync::oneshot::Sender<crate::query::PermissionReply>>,
}

impl PermissionDialog {
    pub fn choices() -> &'static [&'static str] {
        &["Allow", "Always Allow", "Deny"]
    }
}
```

Add key handler for permission dialog in app.rs:
```rust
pub fn handle_permission_key(&mut self, key: KeyEvent) -> bool {
    if let Some(dialog) = &mut self.permission_dialog {
        match key.code {
            KeyCode::Left => { dialog.selected = dialog.selected.saturating_sub(1); }
            KeyCode::Right => { dialog.selected = (dialog.selected + 1).min(2); }
            KeyCode::Enter => { return true; } // signal to resolve
            KeyCode::Char('y') => { dialog.selected = 0; return true; }
            KeyCode::Char('a') => { dialog.selected = 1; return true; }
            KeyCode::Char('n') | KeyCode::Esc => { dialog.selected = 2; return true; }
            _ => {}
        }
    }
    false
}
```

- [ ] **Step 2: Update render_permission_dialog in ui.rs**

Read current `ui.rs` `render_permission_dialog`. Update to show 3 choices with keyboard hints:

```
┌─ Permission Required ───────────────────────┐
│                                              │
│  Tool: Bash                                  │
│  Command: git push origin main               │
│                                              │
│  [Allow (y)]  [Always Allow (a)]  [Deny (n)] │
│                                              │
│  ← → to select, Enter to confirm             │
└──────────────────────────────────────────────┘
```

- [ ] **Step 3: Handle permission_rx in event loop**

In `run.rs` event loop, add permission_rx handling in the `tokio::select!`:

```rust
// Store permission_rx as Option in the outer scope
// When a query is submitted, set it to Some(rx)

// In tokio::select!:
Some(ask) = async {
    match &mut permission_rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
} => {
    app.permission_dialog = Some(PermissionDialog {
        tool_name: ask.tool_name,
        description: ask.description,
        selected: 0,
        reply_tx: Some(ask.reply_tx),
    });
}
```

When user confirms dialog choice:
```rust
if app.permission_dialog.is_some() {
    let resolved = app.handle_permission_key(key);
    if resolved {
        let dialog = app.permission_dialog.take().unwrap();
        if let Some(reply_tx) = dialog.reply_tx {
            let reply = match dialog.selected {
                0 => PermissionReply::Allow,
                1 => PermissionReply::AllowAlways {
                    tool_name: dialog.tool_name.clone(),
                    rule_content: None, // TODO: extract from description
                },
                _ => PermissionReply::Deny,
            };
            let _ = reply_tx.send(reply);
        }
    }
    continue; // skip normal key handling
}
```

- [ ] **Step 4: Run `cargo check -p atom-code`**

- [ ] **Step 5: Commit**

```bash
git add src/tui/
git commit -m "feat(permissions): add TUI permission dialog with Allow/AlwaysAllow/Deny"
```

---

### Task 4: CLI flag wiring and --dangerously-skip-permissions

**Files:**
- Modify: `src/main.rs`
- Modify: `src/tui/run.rs`

- [ ] **Step 1: Parse --permission-mode and --dangerously-skip-permissions**

In `main.rs`, construct `ToolPermissionContext` from CLI flags:

```rust
let permission_mode = if cli.dangerously_skip_permissions {
    PermissionMode::BypassPermissions
} else {
    cli.permission_mode.as_deref()
        .map(|s| match s {
            "default" => PermissionMode::Default,
            "plan" => PermissionMode::Plan,
            "acceptEdits" => PermissionMode::AcceptEdits,
            "bypassPermissions" => PermissionMode::BypassPermissions,
            "auto" => PermissionMode::Auto,
            _ => PermissionMode::Default,
        })
        .unwrap_or(PermissionMode::Default)
};
```

Pass `permission_mode` to `run_app`.

- [ ] **Step 2: Update run_app signature**

Add `permission_mode: PermissionMode` parameter. Use it in `submit_query` when building `ToolPermissionContext`.

- [ ] **Step 3: Verify**

Run: `cargo run -p atom-code -- --dangerously-skip-permissions --api-base ... --api-key ... --model ...`
Expected: Tools execute without permission prompts.

Run: `cargo run -p atom-code -- --permission-mode plan --api-base ... --api-key ... --model ...`
Expected: Tools show "Permission denied: Plan mode" messages.

- [ ] **Step 4: Run tests and commit**

Run: `cargo test -p atom-code -- --include-ignored`

```bash
git add src/main.rs src/tui/run.rs
git commit -m "feat(permissions): wire --permission-mode and --dangerously-skip-permissions"
```

---

### Task 5: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace -- --include-ignored`

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`

- [ ] **Step 3: Manual test — Default mode**

Run app, send a message that triggers Bash tool. Expect permission dialog.

- [ ] **Step 4: Manual test — Bypass mode**

Run with `--dangerously-skip-permissions`. Expect no dialog.

- [ ] **Step 5: Fix any issues and commit**
