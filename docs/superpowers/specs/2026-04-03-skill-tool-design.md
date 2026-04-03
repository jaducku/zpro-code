# SkillTool Implementation Design

## Problem

The Rust version of atom-code-rs has skill loading infrastructure (`services/skills/mod.rs`, `CommandRegistry::skill_tool_commands()`) but the LLM cannot use skills because:

1. **No SkillTool** — no `Skill` tool the LLM can invoke
2. **No skill listing in system prompt** — LLM doesn't know skills exist

Users can only invoke skills via manual `/skill-name` slash commands. The LLM should be able to discover and invoke skills autonomously, matching the TypeScript implementation.

## Approach

Match the TS architecture: enrich `ToolUseContext` with shared state (provider, tools, commands), implement `SkillTool` as a builtin tool that forks a sub-agent query loop, and inject skill listings into the system prompt.

## Design

### 1. ToolUseContext Expansion

Add fields directly to `ToolUseContext` (matching TS's flat structure, not behind a wrapper):

```rust
// src/tool.rs
pub struct ToolUseContext {
    pub input: serde_json::Value,
    pub abort_signal: CancellationToken,
    pub progress_tx: UnboundedSender<ToolProgress>,
    // New fields — matching TS ToolUseContext
    pub provider: Option<Arc<dyn LlmProvider>>,
    pub tools: Option<Arc<Vec<Box<dyn Tool>>>>,
    pub commands: Option<Arc<CommandRegistry>>,
    pub system_prompt: Option<Vec<SystemBlock>>,
    pub cwd: PathBuf,
    pub permission_ctx: Option<ToolPermissionContext>,
    pub model: String,
}
```

Fields are `Option` where the old test code doesn't provide them. Existing tools ignore these fields.

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

**Execution flow (mirrors TS `executeForkedSkill()`):**

1. Extract `skill` name and optional `args` from input
2. Look up skill in `ctx.commands` — must be `Command::Prompt` variant
3. Call `cmd.get_prompt(args, cmd_ctx)` to get prompt text
4. Create a new `QueryContext` (forked sub-agent):
   - Fresh message history: `[User { text: prompt_text }]`
   - Same tools, system prompt, provider from parent context
   - Own abort signal (child of parent's)
5. Call `query()` — runs isolated query loop
6. Extract final assistant text from result messages
7. Return as `ToolResult`

**Permission:** read-only=false, concurrent-safe=false.

### 3. Skill Listing in System Prompt

Modify `services/context/mod.rs`:

- `assemble_system_prompt()` gains a `skill_entries: &[(String, String, Option<String>)]` parameter (name, description, when_to_use)
- After existing blocks (core, AGENTS.md, memory, git), append a system-reminder block listing available skills
- Format: `- name: description - when_to_use` per skill, 250 char cap per entry
- Wrapped in `<system-reminder>` tags

### 4. Files Changed

| File | Change |
|------|--------|
| `src/tool.rs` | Add fields to `ToolUseContext` |
| `src/tools/mod.rs` | Add `skill_tool` module, register in `get_builtin_tools()` |
| `src/tools/skill_tool.rs` | New — SkillTool implementation |
| `src/services/context/mod.rs` | Add skill listing to `assemble_system_prompt()` |
| `src/query/engine.rs` | Pass new fields when constructing `ToolUseContext` in `execute_tool()` |
| `src/tui/run.rs` | Pass skill entries when building system prompt |

Existing tool implementations (BashTool, FileReadTool, etc.) are unchanged — they ignore the new fields. Test helpers that construct `ToolUseContext` need default values for new fields.

## Non-goals

- Skill search/discovery (TS's `EXPERIMENTAL_SKILL_SEARCH`) — not in scope
- MCP skill integration — not in scope
- Budget-based skill listing truncation — use simple 250-char cap instead
- Plugin skills — not in scope
