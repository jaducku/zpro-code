# Agent Hooks Design Spec

## Overview

Implement the `HookCommand::Agent` variant in the hook executor — a multi-turn LLM agent that verifies conditions (e.g., stop hooks) using the existing `query()` engine. The agent uses `SyntheticOutputTool` to return structured `{ok, reason}` results.

## Context

- `HookCommand::Agent` type already defined in `types.rs` with `prompt`, `model`, `timeout`, `if` fields
- `agent_hook.rs` has `MAX_AGENT_HOOK_TURNS=50` and `parse_agent_output()` helper
- `executor.rs:258-267` currently returns "not yet implemented" `NonBlockingError`
- `HookContext` provides `provider` (LlmProvider) and `registry` (ProviderRegistry)
- TypeScript reference: `src/utils/hooks/execAgentHook.ts`

## Design

### Entry Point

`executor.rs` `HookCommand::Agent` arm calls `execute_agent_hook()` from `agent_hook.rs`.

```
executor.rs (HookCommand::Agent arm)
  -> agent_hook.rs::execute_agent_hook()
      -> prompt_hook.rs::build_prompt_input()  (reuse $ARGUMENTS substitution)
      -> SyntheticOutputTool::new({ok, reason} schema)
      -> query::engine::query()  (multi-turn loop)
      -> parse_agent_output()  (reuse existing helper)
```

### Function Signature

```rust
pub async fn execute_agent_hook(
    prompt: &str,
    model: Option<&str>,
    timeout: Option<u64>,
    env: &HookEnv,
    hook_ctx: &HookContext,
) -> HookExecutionResult
```

### Execution Flow

1. **Resolve provider**: If `model` specified, resolve via `hook_ctx.registry`; otherwise use `hook_ctx.provider`. If neither available, return `NonBlockingError`. Default model preference: use provider's own model (no small-model fallback — keep it simple).
2. **Build prompt**: `build_prompt_input(prompt, arguments_json)` — reuse from `prompt_hook.rs`.
3. **Create SyntheticOutputTool**: JSON Schema `{"type":"object","properties":{"ok":{"type":"boolean"},"reason":{"type":"string"}},"required":["ok"]}`.
4. **Filter tools**: `get_builtin_tools()` minus `AGENT_HOOK_DISALLOWED_TOOLS` minus existing StructuredOutput, plus new SyntheticOutputTool.
5. **Build QueryContext**:
   - `messages`: single user message with processed prompt
   - `system_prompt`: `Some(vec![SystemBlock::Text { text }])` — verification task description + StructuredOutput usage instruction
   - `permission_ctx.mode`: `DontAsk`
   - `max_turns`: `MAX_AGENT_HOOK_TURNS` (50)
   - `agent_depth`: None (hook agents are not part of the agent nesting hierarchy)
   - `query_source`: `Some("hook_agent".into())` for tracing/debugging
   - No thinking budget (keep it fast)
6. **Execute**: `tokio::time::timeout(duration, query(ctx, event_tx, abort))`. Default timeout: 60s (timeout field is in milliseconds, matching existing prompt hook convention).
7. **Extract result**: Scan `QueryResult.messages` backwards for a `ContentBlock::ToolUse` with `name == "StructuredOutput"`. The `input` field contains the raw `{ok, reason}` payload (already validated by SyntheticOutputTool). Pass to `parse_agent_output()`.
8. **Return** `HookExecutionResult`.

### Tool Filtering

```rust
const AGENT_HOOK_DISALLOWED_TOOLS: &[&str] = &[
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
```

Plus remove any existing tool named `"StructuredOutput"` before adding the hook's own SyntheticOutputTool.

### System Prompt

```
You are verifying a condition in a coding assistant hook.
Your task is to verify the given condition using the available tools.

Use as few steps as possible - be efficient and direct.

When done, return your result using the StructuredOutput tool with:
- ok: true if the condition is met
- ok: false with reason if the condition is not met
```

### Result Mapping

| Scenario | HookExecutionResult |
|----------|-------------------|
| provider/registry unavailable | `NonBlockingError` |
| Timeout | `NonBlockingError("Agent hook timed out")` |
| Max turns reached (no output) | `NonBlockingError("Agent hook exceeded max turns")` |
| StructuredOutput not called | `NonBlockingError("Agent hook did not return structured output")` |
| `ok=false` | `Blocking { reason }` |
| `ok=true` | `Success` |
| `query()` error | `NonBlockingError` |

### Result Extraction

After `query()` completes, scan `QueryResult.messages` backwards for a `ContentBlock::ToolUse` with `name == "StructuredOutput"`. The `input` field of the ToolUse block contains the raw `{ok, reason}` JSON payload that the LLM provided (already validated by SyntheticOutputTool during execution). Pass `input` to `parse_agent_output()`.

**Why ToolUse input, not ToolResult output:** SyntheticOutputTool wraps its output in `{"data":"...","structured_output":{...}}`, so extracting from ToolResult would require unwrapping. The ToolUse `input` is the direct `{ok, reason}` payload.

### Early Termination

The TS reference aborts immediately after receiving structured output. In Rust, `query()` will continue looping after SyntheticOutputTool executes (sends result back to LLM, LLM may respond again). This is an accepted deviation — the LLM typically stops after calling StructuredOutput (it's designed as a terminal tool), and the extra turn cost is minimal. If this proves wasteful in practice, we can add a `stop_after_tool` flag to QueryContext later.

**Mitigation:** The `max_turns` limit (50) caps worst-case cost. The system prompt explicitly instructs the LLM to use StructuredOutput "when done."

### Permission & Event Handling

- **Permissions**: `DontAsk` mode — all tool calls auto-approved, no TUI prompts.
- **Events**: Create a throwaway `mpsc::unbounded_channel()` for `event_tx` — hook agents don't need TUI updates. Events are simply dropped.
- **Abort**: Create a `CancellationToken` scoped to the hook execution; cancelled on timeout or parent abort.
- **Hooks**: Empty `HooksSettings` — hook agents don't trigger their own hooks (prevents recursion).

### QueryContext Dependencies

Several `QueryContext` fields need lightweight stubs since hook agents don't interact with the TUI:

- `permission_tx`: throwaway channel (DontAsk mode means no asks sent)
- `question_tx`: throwaway channel (non-interactive)
- `commands`: empty `CommandRegistry`
- `app_state`: fresh minimal `AppState` via `AppState::default()` wrapped in `Arc<RwLock<>>` — tools that read/write AppState will no-op, which is fine for hooks
- `mcp_manager`: None

## Files Changed

| File | Change |
|------|--------|
| `src/services/hooks/agent_hook.rs` | Add `execute_agent_hook()`, schema helper, disallowed tools const |
| `src/services/hooks/executor.rs` | Replace Agent arm stub with `execute_agent_hook()` call |

## Testing

- Unit test: `execute_agent_hook()` with mock provider returning StructuredOutput
- Unit test: tool filtering correctness (disallowed tools excluded, StructuredOutput present)
- Unit test: timeout handling
- Unit test: max turns exceeded
- Integration test: full agent hook execution through `execute_event_hooks()`
