# SkillTool Implementation Design

## Problem

The Rust version of atom-code-rs has skill loading infrastructure (`services/skills/mod.rs`, `CommandRegistry::skill_tool_commands()`) but the LLM cannot use skills because:

1. **No SkillTool** — no `Skill` tool the LLM can invoke
2. **No skill listing in system prompt** — LLM doesn't know skills exist

Users can only invoke skills via manual `/skill-name` slash commands. The LLM should be able to discover and invoke skills autonomously, matching the TypeScript implementation.

## Approach

Match the TS architecture: enrich `ToolUseContext` with shared state (provider, tools, commands, event/permission channels), implement `SkillTool` as a builtin tool that handles both inline and forked execution, and inject skill listings into the system prompt.

## Design

### 1. ToolUseContext Expansion

Add fields directly to `ToolUseContext` (matching TS's flat structure). All new fields are **non-Optional** — test code uses a `ToolUseContext::test(input)` helper with sensible defaults.

```rust
// src/tool.rs
pub struct ToolUseContext {
    pub input: serde_json::Value,
    pub abort_signal: CancellationToken,
    pub progress_tx: tokio::sync::mpsc::UnboundedSender<ToolProgress>,
    // New fields — matching TS ToolUseContext
    pub provider: Arc<dyn LlmProvider>,
    pub tools: Arc<Vec<Box<dyn Tool>>>,
    pub commands: Arc<CommandRegistry>,
    pub system_prompt: Vec<SystemBlock>,
    pub cwd: PathBuf,
    pub permission_ctx: ToolPermissionContext,
    pub permission_tx: tokio::sync::mpsc::UnboundedSender<PermissionAsk>,
    pub event_tx: tokio::sync::mpsc::UnboundedSender<QueryEvent>,
    pub model: String,
}

impl ToolUseContext {
    /// Create a minimal context for unit tests (uses a no-op provider)
    pub fn test(input: serde_json::Value) -> Self { ... }
}
```

`event_tx` and `permission_tx` are needed so forked sub-agents can stream events and request permissions through the parent's channels.

### 2. SkillTool

New file: `src/tools/skill_tool.rs`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "skill": { "type": "string", "description": "The skill name" },
    "args": { "type": "string", "description": "Optional arguments for the skill" }
  },
  "required": ["skill"]
}
```

**Execution flow — respects `ContextMode`:**

1. Extract `skill` name and optional `args` from input
2. Look up skill in `ctx.commands` — must be `Command::Prompt` variant
3. Check recursion: track active skill names via a thread-local or passed-in `HashSet<String>`. If skill is already running, return error. Prevents A→B→A infinite loops.
4. Call `cmd.get_prompt(args, cmd_ctx)` to get prompt text
5. Branch on `cmd.context_mode()`:
   - **Inline:** Return the prompt text as `ToolResult` content. The parent query loop injects it as context for the LLM to follow.
   - **Fork:** Create a new `QueryContext`:
     - Fresh messages: `[User { text: prompt_text }]`
     - Same tools, system prompt, provider from parent context
     - Child abort signal via `ctx.abort_signal.child_token()`
     - Parent's `event_tx` and `permission_tx` (so sub-agent events and permission requests reach the TUI)
     - Call `query()` — runs isolated query loop
     - Extract final assistant text from result messages
     - Return as `ToolResult`

**Permission:** read-only=false, concurrent-safe=false.

### 3. Skill Listing in System Prompt

Modify `services/context/mod.rs`:

- Add a `SkillListingConfig` struct to avoid growing `assemble_system_prompt()` parameter list:
  ```rust
  pub struct SystemPromptConfig<'a> {
      pub cwd: &'a Path,
      pub custom_prompt: Option<&'a str>,
      pub append_prompt: Option<&'a str>,
      pub output_style: Option<&'a str>,
      pub skill_entries: &'a [(String, String, Option<String>)], // (name, desc, when_to_use)
  }
  ```
- After existing blocks (core, AGENTS.md, memory, git), append skill listing text wrapped in `<system-reminder>` tags within a `SystemBlock::Text` (current enum only has `Text` variant)
- Format: `- name: description - when_to_use` per skill, 250 char cap per entry

### 4. Files Changed

| File | Change |
|------|--------|
| `src/tool.rs` | Add fields to `ToolUseContext`, add `ToolUseContext::test()` helper |
| `src/tools/mod.rs` | Add `skill_tool` module, register in `get_builtin_tools()` |
| `src/tools/skill_tool.rs` | New — SkillTool implementation with inline/fork branching |
| `src/services/context/mod.rs` | Introduce `SystemPromptConfig`, add skill listing block |
| `src/query/engine.rs` | Pass new fields when constructing `ToolUseContext` in `execute_tool()` |
| `src/tui/run.rs` | Build skill entries from registry, pass in `SystemPromptConfig` |

Existing tool implementations (BashTool, FileReadTool, etc.) are unchanged — they ignore the new fields. Existing test `make_ctx()` helpers migrate to `ToolUseContext::test()`.

### 5. Edge Cases

- **Unknown skill name:** Return `ToolResult` with `is_error: true` and descriptive message
- **Recursive invocation:** Track active skills, reject if already running
- **No provider in degraded mode:** Not applicable — field is non-Optional, always provided
- **Sub-agent abort:** Child token linked to parent via `child_token()`, cancellation propagates

## Non-goals

- Skill search/discovery (TS's `EXPERIMENTAL_SKILL_SEARCH`) — not in scope
- MCP skill integration — not in scope
- Budget-based skill listing truncation — use simple 250-char cap instead
- Plugin skills — not in scope
