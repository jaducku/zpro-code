# SkillTool Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable the LLM to discover and invoke skills autonomously, matching the TypeScript implementation.

**Architecture:** Enrich `ToolUseContext` with shared state, add `provider` and `commands` to `QueryContext`, implement `SkillTool` as a builtin tool with inline/forked execution, and inject skill listings into the system prompt.

**Tech Stack:** Rust, async_trait, tokio, serde_json, Arc for shared state

**Spec:** `docs/superpowers/specs/2026-04-03-skill-tool-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/tool.rs` | `ToolUseContext` struct + `ToolUseContext::test()` helper |
| `src/services/api/provider.rs` | `NoopProvider` for tests |
| `src/query/types.rs` | `QueryContext` with `provider` and `commands` fields |
| `src/query/engine.rs` | `query()` using `ctx.provider`, `execute_tool()` with `QueryContext` ref |
| `src/tools/skill_tool.rs` | **New** — SkillTool implementation |
| `src/tools/mod.rs` | Module registration |
| `src/services/context/mod.rs` | System prompt with skill listing |
| `src/tui/app.rs` | `Arc<CommandRegistry>` storage |
| `src/tui/run.rs` | Pass shared state to `QueryContext` |
| `src/main.rs` | Build `Arc<CommandRegistry>`, skill entries, connect everything |
| `src/entrypoints/print.rs` | Updated `QueryContext` construction |

---

### Task 1: Expand ToolUseContext and QueryContext with shared state

**Files:**
- Modify: `src/tool.rs:59-65`
- Modify: `src/services/api/provider.rs`
- Modify: `src/query/types.rs:12-26`

- [ ] **Step 1: Add `NoopProvider` to `src/services/api/provider.rs`**

Append after the `LlmProvider` trait definition:

```rust
/// No-op provider for tests. Returns an empty stream immediately.
pub struct NoopProvider;

#[async_trait]
impl LlmProvider for NoopProvider {
    async fn create_message(
        &self,
        _request: CreateMessageRequest,
    ) -> crate::types::Result<MessageStream> {
        Ok(Box::pin(futures::stream::empty()))
    }
    fn model_info(&self) -> &ModelInfo {
        // SAFETY: leaking is fine in tests — small, bounded
        Box::leak(Box::new(ModelInfo {
            id: "noop".into(),
            name: "noop".into(),
            context_window: 128_000,
            max_output_tokens: 4096,
            capabilities: Capabilities::default(),
        }))
    }
    fn capabilities(&self) -> &Capabilities {
        Box::leak(Box::new(Capabilities::default()))
    }
}
```

- [ ] **Step 2: Add new fields to `ToolUseContext` in `src/tool.rs`**

Add imports at the top of `src/tool.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;
```

Expand the struct (keeping existing fields, adding new ones after `progress_tx`):

```rust
pub struct ToolUseContext {
    pub input: serde_json::Value,
    pub abort_signal: CancellationToken,
    pub progress_tx: tokio::sync::mpsc::UnboundedSender<ToolProgress>,
    // Shared state for sub-agent spawning (SkillTool, AgentTool)
    pub provider: Arc<dyn crate::services::api::provider::LlmProvider>,
    pub tools: Arc<Vec<Box<dyn Tool>>>,
    pub commands: Arc<crate::commands::registry::CommandRegistry>,
    pub system_prompt: Vec<crate::services::api::types::SystemBlock>,
    pub cwd: PathBuf,
    pub permission_ctx: crate::types::permissions::ToolPermissionContext,
    pub permission_tx: tokio::sync::mpsc::UnboundedSender<crate::query::types::PermissionAsk>,
    pub event_tx: tokio::sync::mpsc::UnboundedSender<crate::query::types::QueryEvent>,
    pub model: String,
}
```

- [ ] **Step 3: Add `ToolUseContext::test()` helper**

Add below the struct, gated by `#[cfg(test)]`:

```rust
#[cfg(test)]
impl ToolUseContext {
    pub fn test(input: serde_json::Value) -> Self {
        let (progress_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (permission_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, _) = tokio::sync::mpsc::unbounded_channel();
        Self {
            input,
            abort_signal: CancellationToken::new(),
            progress_tx,
            provider: Arc::new(crate::services::api::provider::NoopProvider),
            tools: Arc::new(Vec::new()),
            commands: Arc::new(crate::commands::registry::CommandRegistry::empty()),
            system_prompt: Vec::new(),
            cwd: PathBuf::from("/tmp"),
            permission_ctx: crate::types::permissions::ToolPermissionContext {
                mode: crate::types::permissions::PermissionMode::BypassPermissions,
                ..Default::default()
            },
            permission_tx,
            event_tx,
            model: "test".into(),
        }
    }
}
```

- [ ] **Step 4: Add `provider` and `commands` to `QueryContext` in `src/query/types.rs`**

Add import:
```rust
use std::sync::Arc;
use crate::commands::registry::CommandRegistry;
use crate::services::api::provider::LlmProvider;
```

Add fields to `QueryContext`:
```rust
pub struct QueryContext {
    // ... existing fields ...
    pub max_budget_usd: Option<f64>,
    // New
    pub commands: Arc<CommandRegistry>,
    pub provider: Arc<dyn LlmProvider>,
}
```

- [ ] **Step 5: Migrate all test `make_ctx()` helpers to `ToolUseContext::test()`**

In each of these files, replace the `make_ctx` body with `ToolUseContext::test(input)`:
- `src/tools/bash_tool.rs:129-136`
- `src/tools/file_read_tool.rs:116-123`
- `src/tools/file_edit_tool.rs:151-158`
- `src/tools/file_write_tool.rs:104-111`
- `src/tools/glob_tool.rs:100-107`
- `src/tools/grep_tool.rs:151-158`
- `src/tools/agent_tool.rs:140-147`

Each becomes:
```rust
fn make_ctx(input: serde_json::Value) -> ToolUseContext {
    ToolUseContext::test(input)
}
```

- [ ] **Step 6: Verify compilation (won't fully pass tests yet — query engine call sites are stale)**

Run: `cargo check 2>&1 | tail -20`
Expected: Errors only from `query/engine.rs`, `tui/run.rs`, `entrypoints/print.rs` (missing new QueryContext fields). Tool tests should be fine.

- [ ] **Step 7: Commit**

```bash
git add src/tool.rs src/services/api/provider.rs src/query/types.rs src/tools/*.rs
git commit -m "refactor: expand ToolUseContext and QueryContext with shared state"
```

---

### Task 2: Update query engine to use new context fields

**Files:**
- Modify: `src/query/engine.rs:21-26` (query function)
- Modify: `src/query/engine.rs:375-394` (execute_tool function)
- Modify: `src/query/engine.rs:396+` (tests)

- [ ] **Step 1: Change `query()` signature — remove `provider` param, use `ctx.provider`**

```rust
pub async fn query(
    mut ctx: QueryContext,
    event_tx: mpsc::UnboundedSender<QueryEvent>,
    abort_signal: CancellationToken,
) -> crate::types::Result<QueryResult> {
```

Inside `query()`, replace `provider.create_message(request)` with `ctx.provider.create_message(request)`.

At the top of `query()`, wrap `ctx.tools` in an `Arc` for sharing with `execute_tool`:
```rust
let tools_arc = Arc::new(std::mem::take(&mut ctx.tools));
```

Then use `&tools_arc` where `&ctx.tools` was used (the tools list for finding tools by name).

- [ ] **Step 2: Simplify `execute_tool()` — pass `&QueryContext` instead of 12 params**

```rust
async fn execute_tool(
    tc: &ToolCall,
    tools: &[Box<dyn crate::tool::Tool>],
    abort_signal: &CancellationToken,
    ctx: &QueryContext,
    tools_arc: &Arc<Vec<Box<dyn crate::tool::Tool>>>,
    event_tx: &mpsc::UnboundedSender<QueryEvent>,
) -> crate::types::Result<crate::tool::ToolResult> {
    let tool = find_tool_by_name(tools, &tc.name).ok_or_else(|| crate::types::AtomError::Tool {
        tool_name: tc.name.clone(),
        message: format!("Tool '{}' not found", tc.name),
    })?;

    let (progress_tx, _progress_rx) = mpsc::unbounded_channel::<ToolProgress>();

    let tool_ctx = ToolUseContext {
        input: tc.input.clone(),
        abort_signal: abort_signal.clone(),
        progress_tx,
        provider: ctx.provider.clone(),
        tools: tools_arc.clone(),
        commands: ctx.commands.clone(),
        system_prompt: ctx.system_prompt.clone().unwrap_or_default(),
        cwd: ctx.cwd.clone(),
        permission_ctx: ctx.permission_ctx.clone(),
        permission_tx: ctx.permission_tx.clone(),
        event_tx: event_tx.clone(),
        model: ctx.model.clone(),
    };

    tool.execute(tool_ctx).await
}
```

- [ ] **Step 3: Update `execute_tool()` call site inside `query()`**

```rust
let result = execute_tool(tc, &tools_arc, &abort_signal, &ctx, &tools_arc, &event_tx).await;
```

- [ ] **Step 4: Update `query()` tests**

Update `test_query_context` helper:
```rust
fn test_query_context(
    messages: Vec<ApiMessage>,
    tools: Vec<Box<dyn crate::tool::Tool>>,
) -> QueryContext {
    let (permission_tx, _rx) = mpsc::unbounded_channel();
    QueryContext {
        messages,
        tools,
        system_prompt: None,
        model: "test".into(),
        max_tokens: 4096,
        permission_ctx: ToolPermissionContext {
            mode: PermissionMode::BypassPermissions,
            ..Default::default()
        },
        permission_tx,
        hooks: std::collections::HashMap::new(),
        cwd: std::path::PathBuf::from("."),
        max_turns: None,
        max_budget_usd: None,
        commands: Arc::new(crate::commands::registry::CommandRegistry::empty()),
        provider: Arc::new(crate::services::api::provider::NoopProvider),
    }
}
```

Update test `query()` calls (remove provider param):
```rust
// Before: query(ctx, &provider, event_tx, CancellationToken::new()).await
// After:
query(ctx, event_tx, CancellationToken::new()).await
```

In `TwoStepProvider` test — the provider is now in `QueryContext`:
```rust
let ctx = test_query_context(/* ... */);
// Override provider in ctx:
let ctx = QueryContext {
    provider: Arc::new(TwoStepProvider { call_count: cc }),
    ..ctx
};
```

Wait — actually the `test_query_context` already sets `provider: NoopProvider`. For the `TwoStepProvider` test, we need to set a different provider. Use struct update syntax:
```rust
let mut ctx = test_query_context(
    vec![ApiMessage { role: Role::User, content: vec![ContentBlock::Text { text: "test".into() }] }],
    vec![Box::new(EchoTool)],
);
ctx.provider = Arc::new(TwoStepProvider { call_count: cc });
```

- [ ] **Step 5: Run `cargo test` to verify**

Run: `cargo test 2>&1 | tail -20`
Expected: Compilation errors only from `tui/run.rs` and `entrypoints/print.rs` (call sites not yet updated). Query engine tests should pass.

Actually — `tui/run.rs` and `print.rs` also need updating now. Let's check with `cargo check`.

Run: `cargo check 2>&1 | grep "error\[" | head -10`
Expected: Errors only in `tui/run.rs` and `entrypoints/print.rs`.

- [ ] **Step 6: Commit**

```bash
git add src/query/engine.rs
git commit -m "refactor: simplify query() to use ctx.provider, pass QueryContext to execute_tool"
```

---

### Task 3: Update call sites (tui/run.rs, print.rs, main.rs, app.rs)

**Files:**
- Modify: `src/tui/app.rs:52` — `command_registry` field type
- Modify: `src/tui/app.rs:120` — `App::new()` signature
- Modify: `src/tui/run.rs:29-40` — `run_app()` signature
- Modify: `src/tui/run.rs:549-566` — `submit_query()` QueryContext construction
- Modify: `src/tui/run.rs:573` — `query()` call
- Modify: `src/tui/run.rs:586+` — `submit_query_skill()` QueryContext construction
- Modify: `src/entrypoints/print.rs:70-98` — QueryContext and query() call
- Modify: `src/main.rs:847-860` — `Arc<CommandRegistry>` and `run_app()` call

- [ ] **Step 1: Change `App.command_registry` to `Arc<CommandRegistry>`**

In `src/tui/app.rs`:

```rust
use std::sync::Arc;

pub struct App {
    // ...
    pub command_registry: Arc<CommandRegistry>,
    // ...
}

impl App {
    pub fn new(model: String, command_registry: Arc<CommandRegistry>) -> Self {
        // ... same body, just use command_registry directly ...
    }
}
```

Update the test at line ~844:
```rust
let reg = Arc::new(CommandRegistry::new());
let app = App::new("test".into(), reg);
```

- [ ] **Step 2: Update `run_app()` signature in `src/tui/run.rs`**

```rust
pub async fn run_app(
    provider: Option<Arc<dyn LlmProvider>>,
    mcp_manager: McpConnectionManager,
    system_prompt: Option<Vec<SystemBlock>>,
    command_registry: Arc<CommandRegistry>,  // was CommandRegistry
    // ... rest unchanged ...
) -> io::Result<()> {
```

Update `App::new()` call inside `run_app()`:
```rust
let mut app = App::new(model.clone(), command_registry);
```

- [ ] **Step 3: Update `submit_query()` — add `commands` and `provider` to QueryContext**

In `submit_query()` (around line 549):

```rust
let ctx = crate::query::QueryContext {
    messages: app.api_messages.clone(),
    tools,
    system_prompt: system_prompt.clone(),
    model: app.active_model().to_string(),
    max_tokens: provider.model_info().max_output_tokens,
    permission_ctx: ToolPermissionContext {
        mode: permission_mode.clone(),
        always_allow_rules: allow_rules.to_vec(),
        always_deny_rules: deny_rules.to_vec(),
        ..Default::default()
    },
    permission_tx: perm_tx,
    hooks,
    cwd,
    max_turns: None,
    max_budget_usd: None,
    commands: app.command_registry.clone(),
    provider: provider.clone(),
};
```

Update the `query()` call (remove provider param):
```rust
// Before: crate::query::engine::query(ctx, &*provider, event_tx.clone(), cancel).await
// After:
crate::query::engine::query(ctx, event_tx.clone(), cancel).await
```

- [ ] **Step 4: Update `submit_query_skill()` similarly**

Same pattern — add `commands` and `provider` to QueryContext, remove provider from `query()` call.

- [ ] **Step 5: Update `src/entrypoints/print.rs`**

Add imports:
```rust
use std::sync::Arc;
use crate::commands::registry::CommandRegistry;
```

Update QueryContext construction (around line 70):
```rust
let ctx = QueryContext {
    // ... existing fields ...
    max_budget_usd,
    commands: Arc::new(CommandRegistry::empty()),
    provider: provider.clone(),
};
```

Update query() call (line 98):
```rust
// Before: crate::query::engine::query(ctx, &*provider_clone, event_tx, cancel).await
// After:
crate::query::engine::query(ctx, event_tx, cancel).await
```

Remove the now-unused `provider_clone` variable.

- [ ] **Step 6: Update `src/main.rs` — use `Arc<CommandRegistry>`**

Around line 847:
```rust
let mut command_registry = CommandRegistry::new();
command_registry.load_skill_dir_commands(&cwd).await?;
let command_registry = Arc::new(command_registry);
```

`run_app()` call already takes `command_registry` — now it's `Arc<CommandRegistry>`.

- [ ] **Step 7: Run `cargo test` to verify everything compiles and passes**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/tui/app.rs src/tui/run.rs src/entrypoints/print.rs src/main.rs
git commit -m "refactor: wire Arc<CommandRegistry> and provider through all call sites"
```

---

### Task 4: Add skill listing to system prompt

**Files:**
- Modify: `src/services/context/mod.rs`
- Modify: `src/main.rs:781-786`

- [ ] **Step 1: Add `SystemPromptConfig` and `format_skill_listing()` to `src/services/context/mod.rs`**

Add after the imports:

```rust
/// Configuration for system prompt assembly with skill listing.
pub struct SystemPromptConfig<'a> {
    pub cwd: &'a Path,
    pub custom_prompt: Option<&'a str>,
    pub append_prompt: Option<&'a str>,
    pub output_style: Option<&'a str>,
    pub skill_entries: &'a [(String, String, Option<String>)],
}

const MAX_SKILL_DESC_CHARS: usize = 250;

fn format_skill_listing(entries: &[(String, String, Option<String>)]) -> String {
    entries.iter().map(|(name, desc, when_to_use)| {
        let full_desc = match when_to_use {
            Some(wtu) => format!("{} - {}", desc, wtu),
            None => desc.clone(),
        };
        let truncated = if full_desc.len() > MAX_SKILL_DESC_CHARS {
            format!("{}…", &full_desc[..MAX_SKILL_DESC_CHARS - 1])
        } else {
            full_desc
        };
        format!("- {}: {}", name, truncated)
    }).collect::<Vec<_>>().join("\n")
}

pub fn assemble_system_prompt_with_config(config: &SystemPromptConfig) -> Vec<SystemBlock> {
    let mut blocks = assemble_system_prompt(
        config.cwd,
        config.custom_prompt,
        config.append_prompt,
        config.output_style,
    );

    if !config.skill_entries.is_empty() {
        let listing = format_skill_listing(config.skill_entries);
        blocks.push(SystemBlock::Text {
            text: format!(
                "<system-reminder>\nThe following skills are available for use with the Skill tool:\n\n{}\n</system-reminder>",
                listing
            ),
        });
    }

    blocks
}
```

- [ ] **Step 2: Update `src/main.rs` TUI path to use `assemble_system_prompt_with_config()`**

After `command_registry.load_skill_dir_commands()` (around line 848), build skill entries and use new function:

```rust
use atom_code::services::context::{assemble_system_prompt_with_config, SystemPromptConfig};

let skill_entries: Vec<(String, String, Option<String>)> = command_registry
    .skill_tool_commands()
    .iter()
    .map(|cmd| {
        let meta = cmd.meta();
        (
            meta.name.to_string(),
            meta.description.to_string(),
            meta.when_to_use.map(|s| s.to_string()),
        )
    })
    .collect();

let system_prompt = Some(assemble_system_prompt_with_config(&SystemPromptConfig {
    cwd: &cwd,
    custom_prompt: cli.system_prompt.as_deref(),
    append_prompt: cli.append_system_prompt.as_deref(),
    output_style: None,
    skill_entries: &skill_entries,
}));
```

The print mode path can keep using `assemble_system_prompt()` (no skill registry in print mode).

- [ ] **Step 3: Add tests for `format_skill_listing()`**

Add to the existing `#[cfg(test)] mod tests` in `src/services/context/mod.rs`:

```rust
#[test]
fn test_format_skill_listing_basic() {
    let entries = vec![
        ("commit".into(), "Commit changes".into(), Some("Use after making changes".into())),
        ("review".into(), "Review code".into(), None),
    ];
    let listing = format_skill_listing(&entries);
    assert!(listing.contains("- commit: Commit changes - Use after making changes"));
    assert!(listing.contains("- review: Review code"));
}

#[test]
fn test_format_skill_listing_truncation() {
    let long_desc = "x".repeat(300);
    let entries = vec![("long".into(), long_desc, None)];
    let listing = format_skill_listing(&entries);
    assert!(listing.len() < 300);
    assert!(listing.contains('…'));
}

#[test]
fn test_format_skill_listing_empty() {
    let entries: Vec<(String, String, Option<String>)> = vec![];
    let listing = format_skill_listing(&entries);
    assert!(listing.is_empty());
}

#[test]
fn test_assemble_with_skills() {
    let dir = TempDir::new().unwrap();
    let config = SystemPromptConfig {
        cwd: dir.path(),
        custom_prompt: None,
        append_prompt: None,
        output_style: None,
        skill_entries: &[("brainstorming".into(), "Creative work".into(), Some("Use before features".into()))],
    };
    let blocks = assemble_system_prompt_with_config(&config);
    let full: String = blocks.iter().map(|b| match b { SystemBlock::Text { text } => text.as_str() }).collect::<Vec<_>>().join("\n");
    assert!(full.contains("<system-reminder>"));
    assert!(full.contains("brainstorming"));
}
```

- [ ] **Step 4: Run `cargo test`**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/services/context/mod.rs src/main.rs
git commit -m "feat: inject skill listing into system prompt via system-reminder"
```

---

### Task 5: Implement SkillTool

**Files:**
- Create: `src/tools/skill_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create `src/tools/skill_tool.rs`**

```rust
use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::commands::context::{CommandContext, CommandOptions};
use crate::commands::traits::Command;
use crate::commands::types::ContextMode;
use crate::query::engine::query;
use crate::query::types::QueryContext;
use crate::state::AppState;
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};

pub struct SkillTool;

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        "Execute a skill within the main conversation.\n\n\
         When users ask you to perform tasks, check if any of the available skills match. \
         Skills provide specialized capabilities and domain knowledge.\n\n\
         When users reference a \"slash command\" or \"/<something>\" (e.g., \"/commit\", \"/review-pr\"), \
         they are referring to a skill. Use this tool to invoke it.\n\n\
         How to invoke:\n\
         - Use this tool with the skill name and optional arguments\n\
         - Examples:\n\
           - skill: \"pdf\" - invoke the pdf skill\n\
           - skill: \"commit\", args: \"-m 'Fix bug'\" - invoke with arguments\n\n\
         Important:\n\
         - Available skills are listed in system-reminder messages in the conversation\n\
         - When a skill matches the user's request, this is a BLOCKING REQUIREMENT: \
         invoke the relevant Skill tool BEFORE generating any other response about the task\n\
         - NEVER mention a skill without actually calling this tool\n\
         - Do not invoke a skill that is already running\n\
         - Do not use this tool for built-in CLI commands (like /help, /clear, etc.)\n\
         - If you see a <command-name> tag in the current conversation turn, the skill has \
         ALREADY been loaded - follow the instructions directly instead of calling this tool again"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "The skill name. E.g., \"commit\", \"review-pr\", or \"pdf\""
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the skill"
                }
            },
            "required": ["skill"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let skill_name = ctx.input.get("skill").and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Skill".into(),
                message: "Missing required 'skill' parameter".into(),
            })?;

        let args = ctx.input.get("args").and_then(|v| v.as_str()).unwrap_or("");

        // Look up skill in registry
        let command = ctx.commands.find(skill_name);
        let prompt_cmd = match command {
            Some(Command::Prompt(cmd)) => cmd,
            _ => {
                return Ok(ToolResult {
                    content: format!(
                        "Skill '{}' not found. Check available skills in the system-reminder.",
                        skill_name
                    ),
                    is_error: true,
                });
            }
        };

        let context_mode = prompt_cmd.context_mode();

        // Build CommandContext for get_prompt()
        let (msg_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (ui_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let cmd_ctx = CommandContext {
            app_state: Arc::new(tokio::sync::RwLock::new(AppState::default())),
            abort_signal: ctx.abort_signal.child_token(),
            cwd: ctx.cwd.clone(),
            message_tx: msg_tx,
            ui_event_tx: ui_tx,
            options: CommandOptions { theme: "default".to_string() },
        };

        // Get prompt text
        let blocks = match prompt_cmd.get_prompt(args, &cmd_ctx).await {
            Ok(b) => b,
            Err(e) => {
                return Ok(ToolResult {
                    content: format!("Failed to load skill '{}': {}", skill_name, e),
                    is_error: true,
                });
            }
        };

        let prompt_text: String = blocks.iter().filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        }).collect::<Vec<_>>().join("\n");

        if prompt_text.is_empty() {
            return Ok(ToolResult {
                content: format!("Skill '{}' produced no content.", skill_name),
                is_error: true,
            });
        }

        match context_mode {
            ContextMode::Inline => {
                Ok(ToolResult {
                    content: format!("<command-name>{}</command-name>\n{}", skill_name, prompt_text),
                    is_error: false,
                })
            }
            ContextMode::Fork => {
                execute_forked_skill(skill_name, &prompt_text, &ctx).await
            }
        }
    }

    fn is_read_only(&self) -> bool {
        false
    }
}

/// Run a skill in an isolated sub-agent with its own message history.
async fn execute_forked_skill(
    skill_name: &str,
    prompt_text: &str,
    parent_ctx: &ToolUseContext,
) -> crate::types::Result<ToolResult> {
    let child_abort = parent_ctx.abort_signal.child_token();
    let event_tx = parent_ctx.event_tx.clone();

    let query_ctx = QueryContext {
        messages: vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text { text: prompt_text.to_string() }],
        }],
        tools: parent_ctx.tools.as_ref().clone(),
        system_prompt: Some(parent_ctx.system_prompt.clone()),
        model: parent_ctx.model.clone(),
        max_tokens: 16_384,
        permission_ctx: parent_ctx.permission_ctx.clone(),
        permission_tx: parent_ctx.permission_tx.clone(),
        hooks: std::collections::HashMap::new(),
        cwd: parent_ctx.cwd.clone(),
        max_turns: Some(25),
        max_budget_usd: None,
        commands: parent_ctx.commands.clone(),
        provider: parent_ctx.provider.clone(),
    };

    let result = query(query_ctx, event_tx, child_abort).await;

    match result {
        Ok(qr) => {
            let text = qr.messages.iter().rev()
                .find(|m| m.role == Role::Assistant)
                .and_then(|m| {
                    m.content.iter().find_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| format!("Skill '{}' completed with no text output.", skill_name));

            Ok(ToolResult { content: text, is_error: false })
        }
        Err(e) => {
            Ok(ToolResult {
                content: format!("Skill '{}' failed: {}", skill_name, e),
                is_error: true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_tool_name() {
        assert_eq!(SkillTool.name(), "Skill");
    }

    #[test]
    fn test_skill_tool_schema() {
        let schema = SkillTool.input_schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("skill")));
    }

    #[tokio::test]
    async fn test_skill_not_found() {
        let ctx = ToolUseContext::test(serde_json::json!({ "skill": "nonexistent" }));
        let result = SkillTool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_skill_missing_param() {
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = SkillTool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Register in `src/tools/mod.rs`**

Add module declaration:
```rust
pub mod skill_tool;
```

Add to `get_builtin_tools()`:
```rust
Box::new(skill_tool::SkillTool),
```

Update test assertion count:
```rust
assert!(tools.len() >= 7); // was 6, now includes SkillTool
```

- [ ] **Step 3: Run `cargo test`**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/tools/skill_tool.rs src/tools/mod.rs
git commit -m "feat: implement SkillTool for LLM-invokable skill execution"
```

---

### Task 6: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass, zero failures.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | tail -20`
Expected: No warnings.

- [ ] **Step 3: Verify brainstorming skill would be discovered**

Run: `ls ~/.atom/skills/brainstorming/SKILL.md`
Expected: File exists.

- [ ] **Step 4: Commit any final fixes**
