# Permission System — Design Spec

## Overview

Implement tool execution permission checking before each tool call. 5 permission modes matching the original: Default (ask), Plan (block), AcceptEdits (auto-approve edits), Bypass (allow all), Auto (rule-based classification). TUI permission dialog for user prompts. Channel-based query engine ↔ TUI communication.

**References:** Original `src/utils/permissions/`, `src/types/permissions.ts`

---

## 1. Permission Modes

```rust
pub enum PermissionMode {
    Default,          // Ask for every tool call
    Plan,             // Block all tool execution (planning only)
    AcceptEdits,      // Auto-allow Edit/Write, ask for Bash
    Bypass,           // Allow everything (--dangerously-skip-permissions)
    Auto,             // Rule-based: Read/Glob/Grep → allow, Bash/Write → ask
}
```

CLI: `--permission-mode default|plan|acceptEdits|bypassPermissions`
CLI: `--dangerously-skip-permissions` → sets Bypass mode

## 2. Permission Rules (original 1:1)

```rust
pub struct PermissionRuleValue {
    pub tool_name: String,
    pub rule_content: Option<String>,  // "git:*", "/src/**"
}

pub enum PermissionBehavior { Allow, Deny, Ask }

pub enum PermissionRuleSource {
    UserSettings, ProjectSettings, LocalSettings,
    CliArg, Command, Session,
}

pub struct PermissionRule {
    pub source: PermissionRuleSource,
    pub rule_behavior: PermissionBehavior,
    pub rule_value: PermissionRuleValue,
}
```

Rule matching: `Bash(git:*)` matches tool_name="Bash" with rule_content pattern `git:*` against the actual command. Glob-style matching.

## 3. Permission Context

```rust
pub struct ToolPermissionContext {
    pub mode: PermissionMode,
    pub allow_rules: Vec<PermissionRule>,
    pub deny_rules: Vec<PermissionRule>,
}
```

## 4. Permission Decision (original 1:1)

```rust
pub enum PermissionDecision {
    Allow {
        rule: Option<PermissionRule>,
        reason: Option<String>,
    },
    Ask {
        tool_name: String,
        description: String,
        input: serde_json::Value,
    },
    Deny {
        reason: String,
        rule: Option<PermissionRule>,
    },
}
```

## 5. Check Permission Flow

```rust
pub fn check_permission(
    tool_name: &str,
    tool_input: &serde_json::Value,
    permission_meta: &PermissionMeta,
    ctx: &ToolPermissionContext,
) -> PermissionDecision
```

Flow (matches original):
1. **Bypass** → `Allow`
2. **Plan** → `Deny("Plan mode: tool execution blocked")`
3. **Deny rules** → match against `deny_rules`, if hit → `Deny`
4. **Allow rules** → match against `allow_rules`, if hit → `Allow`
5. **AcceptEdits** + tool is Edit/Write/NotebookEdit → `Allow`
6. **Auto mode** → classify:
   - Read, Glob, Grep, Bash(read-only commands) → `Allow`
   - Write, Edit, Bash(mutating commands) → `Ask`
7. **Default** → `Ask`

## 6. Auto-Mode Classification (rule-based, no ML)

```rust
pub fn auto_classify(tool_name: &str, input: &serde_json::Value) -> PermissionBehavior {
    match tool_name {
        "Read" | "Glob" | "Grep" => PermissionBehavior::Allow,
        "Bash" => {
            let cmd = input["command"].as_str().unwrap_or("");
            if is_readonly_bash(cmd) { Allow } else { Ask }
        }
        "Write" | "Edit" | "NotebookEdit" => PermissionBehavior::Ask,
        _ => PermissionBehavior::Ask,
    }
}

fn is_readonly_bash(cmd: &str) -> bool {
    // Commands that don't modify state
    let readonly = ["git status", "git log", "git diff", "git branch",
                    "ls", "cat", "head", "tail", "find", "grep", "rg",
                    "which", "pwd", "echo", "date", "wc", "sort", "uniq"];
    readonly.iter().any(|r| cmd.starts_with(r))
}
```

## 7. Query Engine Integration

In `query/engine.rs`, before `execute_tool()`:

```rust
let decision = check_permission(
    &tc.name, &tc.input, &tool.permission_metadata(&tc.input), &ctx.permission_ctx,
);

match decision {
    PermissionDecision::Allow { .. } => {
        let result = execute_tool(tc, ...).await;
    }
    PermissionDecision::Deny { reason, .. } => {
        let _ = event_tx.send(QueryEvent::ToolCallComplete {
            id: tc.id.clone(),
            result: ToolResultBlock::error(tc.id.clone(), reason),
        });
    }
    PermissionDecision::Ask { .. } => {
        // Send to TUI, wait for reply
        let _ = permission_tx.send(PermissionRequest {
            tool_use_id: tc.id.clone(),
            tool_name: tc.name.clone(),
            description: format_tool_description(&tc),
            input: tc.input.clone(),
            reply_tx: oneshot_tx,
        });
        match oneshot_rx.await {
            Ok(PermissionReply::Allow) => execute_tool(tc, ...).await,
            Ok(PermissionReply::AllowAlways(rule)) => {
                ctx.permission_ctx.allow_rules.push(rule);
                execute_tool(tc, ...).await
            }
            Ok(PermissionReply::Deny) => {
                ToolResultBlock::error(tc.id, "Permission denied by user")
            }
            Err(_) => ToolResultBlock::error(tc.id, "Permission dialog cancelled"),
        }
    }
}
```

## 8. TUI Dialog

Existing `App.permission_dialog` and `render_permission_dialog()` in `ui.rs` are activated.

Event loop addition:
```rust
// In tokio::select!
Some(req) = permission_ask_rx.recv() => {
    app.permission_dialog = Some(PermissionDialog {
        tool_name: req.tool_name,
        description: req.description,
        selected: PermissionChoice::Allow,
        reply_tx: Some(req.reply_tx),
    });
}
```

User presses Enter on Allow/AllowAlways/Deny → sends reply through `reply_tx`, clears dialog.

## 9. Channel Types

```rust
pub struct PermissionRequest {
    pub tool_use_id: String,
    pub tool_name: String,
    pub description: String,
    pub input: serde_json::Value,
    pub reply_tx: tokio::sync::oneshot::Sender<PermissionReply>,
}

pub enum PermissionReply {
    Allow,
    AllowAlways(PermissionRule),
    Deny,
}
```

## 10. File Structure

```
src/utils/permissions/
├── mod.rs                 // re-exports
├── permission_mode.rs     // PermissionMode enum
├── permission_rule.rs     // PermissionRule, matching
├── permission_result.rs   // PermissionDecision
├── checker.rs             // check_permission() main flow
├── auto_classify.rs       // rule-based auto classification
└── types.rs               // PermissionRequest, PermissionReply channels
```

Modify:
- `src/query/engine.rs` — add permission check before tool execution
- `src/tui/run.rs` — handle permission_ask_rx in event loop
- `src/tui/app.rs` — update PermissionDialog with reply_tx + 3 choices
- `src/tui/ui.rs` — update render_permission_dialog with AllowAlways option
- `src/main.rs` — parse --permission-mode, pass to query context
