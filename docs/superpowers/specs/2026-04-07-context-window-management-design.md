# Context Window Management Design Spec

**Date:** 2026-04-07
**Goal:** Multi-layer compaction pipeline matching the original TS Claude Code — snip, microcompact, full compact, reactive compact.

## Overview

Replace the current single-pass compaction (80% threshold, drop old messages) with a 4-stage pipeline that progressively frees tokens using increasingly expensive strategies.

**Replaces:** The existing `CompactResult`, `needs_compaction()`, and `build_compact_prompt()` in `src/services/compact/mod.rs` are removed and replaced by the new pipeline. The existing compaction block in `engine.rs` (lines 49-73) is replaced entirely.

**New files:**
- `src/services/compact/snip.rs`
- `src/services/compact/microcompact.rs`
- `src/services/compact/full_compact.rs`
- `src/services/compact/types.rs`

**Modified files:**
- `src/services/compact/mod.rs` (pipeline orchestrator)
- `src/query/engine.rs` (integrate pipeline, add reactive compact)
- `src/query/types.rs` (add Compacted event)
- `src/tui/app.rs` (handle Compacted event in `handle_query_event` exhaustive match)

---

## 1. Compaction Pipeline

### Execution order (per LLM call in query loop)

```
estimate tokens → ratio = tokens / context_window

if ratio > 0.60 → Stage 1: Snip
re-estimate
if ratio > 0.75 → Stage 2: Microcompact
re-estimate
if ratio > 0.85 → Stage 3: Full Compact (LLM call)
re-estimate
if ratio > 0.95 → HARD BLOCK (error, stop query — all stages exhausted)
proceed to API call
on API error "prompt_too_long" → Stage 4: Reactive Compact → retry once
```

Note: Hard block runs AFTER all proactive stages, giving snip/microcompact/full_compact a chance to reduce the ratio first. Threshold gaps are wider (15-point spacing) to account for the ~4 chars/token estimation heuristic being imprecise.

Each stage only runs if the previous stages didn't bring the ratio below threshold. The pipeline is additive — a single iteration may run 0, 1, 2, or all 3 proactive stages.

### Context window source

`ctx.provider.model_info().context_window` — per-model, configurable in `config.json`:
```json
{
  "llms": {
    "flash": {
      "context_window": 1000000,
      "model_id": "google/gemini-2.5-flash"
    }
  }
}
```
Default: 128,000 if not specified.

---

## 2. Types (`src/services/compact/types.rs`)

```rust
use crate::types::message::ApiMessage;

/// Result of any compaction operation.
pub struct CompactResult {
    pub messages: Vec<ApiMessage>,   // replacement message list
    pub tokens_freed: u64,           // estimated tokens freed
    pub stage: CompactStage,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompactStage {
    Snip,
    Microcompact,
    FullCompact,
    Reactive,
}

impl std::fmt::Display for CompactStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snip => write!(f, "snip"),
            Self::Microcompact => write!(f, "microcompact"),
            Self::FullCompact => write!(f, "full"),
            Self::Reactive => write!(f, "reactive"),
        }
    }
}

/// Marker content for the boundary system message.
pub const COMPACT_BOUNDARY_PREFIX: &str = "[Context compacted";
```

---

## 3. Snip (`src/services/compact/snip.rs`)

Cheapest compaction — just removes old messages and inserts a boundary marker.

### Trigger
Token ratio > 0.60

### Algorithm
1. Guard: need at least 1 message beyond what we keep
2. Find any existing compact boundary (message with `COMPACT_BOUNDARY_PREFIX` in content)
3. Determine `keep_last` by scanning backward for complete turn groups (not raw count):
   - A "turn" = user message + optional assistant(tool_use) + tool(result) chain + final assistant(text)
   - Keep the last 3 complete turns (variable message count)
   - This ensures tool_use/tool_result pairs are never split
4. Remove everything before the kept suffix
5. Insert boundary message at position 0

### Message pair safety
When determining the keep boundary, always snap to turn boundaries. A turn starts with a `Role::User` message. Never split an assistant→tool_result sequence — if keeping an assistant message with `ContentBlock::ToolUse`, the corresponding `Role::User` message containing the `ToolResult` must also be kept.

### Interface

```rust
/// keep_turns: number of complete turns to preserve (e.g., 3 = last 3 user→assistant exchanges)
pub fn snip(messages: &mut Vec<ApiMessage>, keep_turns: usize) -> u64 {
    // Returns: estimated tokens freed
}
```

### Boundary message format
```
[Context compacted: N earlier messages removed. ~K tokens freed]
```

Role: `User`. Content: `ContentBlock::Text` with the boundary text. Using `Role::User` (not `Role::System`) to ensure compatibility with all LLM providers — many providers reject `role: "system"` in the messages array.

---

## 4. Microcompact (`src/services/compact/microcompact.rs`)

Truncates long tool results in-place to free tokens without losing conversation structure.

### Trigger
Token ratio > 0.75 (after snip)

### Algorithm
1. Iterate messages in reverse
2. Skip the last `preserve_last` tool_result messages (default 3) — they're recent and likely important
3. For remaining messages, find `tool_result` content blocks with text length > `max_chars` (default 2000)
4. Truncate: keep first 500 chars + `\n...[truncated N chars]...\n` + last 500 chars
5. Remove image blocks from non-recent messages (images in old turns are rarely needed)

### Interface

```rust
pub fn microcompact(messages: &mut Vec<ApiMessage>, preserve_last: usize) -> u64 {
    // Mutates messages in-place
    // Returns: estimated tokens freed
}
```

### What counts as a tool_result

`ContentBlock::ToolResult { tool_use_id, content, is_error }` where `content` is `ToolResultContent` — an enum with two variants:

```rust
pub enum ToolResultContent {
    Text(String),          // Direct text — truncate this string
    Blocks(Vec<ContentBlock>),  // Nested blocks — iterate and truncate Text blocks within
}
```

For `ToolResultContent::Text(s)`: truncate `s` directly.
For `ToolResultContent::Blocks(blocks)`: iterate blocks, find `ContentBlock::Text { text }` entries, truncate each long text. Leave non-text blocks (images) for the image removal pass.

---

## 5. Full Compact (`src/services/compact/full_compact.rs`)

LLM-based summarization — the most expensive but most effective strategy.

### Trigger
Token ratio > 0.85 (after snip + microcompact)

### Algorithm
1. Split messages: `to_summarize` (older) and `to_keep` (last 3 complete turns, same turn-boundary logic as snip)
2. Build a summary request:
   - System prompt: compaction instructions (see below)
   - Messages: the `to_summarize` batch
3. Call the LLM (same provider) with no tools, single response
4. Build result:
   - Boundary message (system)
   - Summary message (assistant, the LLM's summary)
   - Kept messages (preserved suffix)

### Summary system prompt
```
You are summarizing a conversation between a user and an AI coding assistant.
Preserve all important context:
- Decisions made and their reasoning
- Files created, modified, or deleted
- Current goals and next steps
- Any constraints or requirements mentioned
- Error states or blockers encountered

Be concise but complete. Output a structured summary.
```

### Interface

```rust
/// keep_turns: number of complete turns to preserve (same turn-boundary logic as snip)
pub async fn full_compact(
    messages: &[ApiMessage],
    provider: &dyn LlmProvider,
    context_window: u64,
    keep_turns: usize,
) -> Result<CompactResult, AtomError>
```

### LLM call

Build a `CreateMessageRequest` directly and call `provider.stream_message()`, collecting the full response. Do NOT use `query_without_streaming` — it only supports a flat prompt string, not system+messages structure.

The summary request:
- System: compaction instructions (above)
- Messages: `to_summarize` messages wrapped in a single user message with serialized content
- max_tokens: 4096 (keep summary concise)
- No tools

### Circuit breaker

Track consecutive failures in the query loop (not inside full_compact). After 3 consecutive failures, skip full_compact and log a warning. Reset counter on success.

---

## 6. Reactive Compact

Not a separate module — implemented inline in `engine.rs` error handling.

### Trigger
API returns an error matching context overflow patterns.

### Error detection

```rust
fn is_context_overflow_error(error: &AtomError) -> bool {
    let msg = error.to_string().to_lowercase();
    msg.contains("prompt_too_long")
        || msg.contains("context_length")
        || msg.contains("max_context")
        || msg.contains("too many tokens")
        || msg.contains("context_window_exceeded")
}
```

### Algorithm
1. Detect context overflow error after API call
2. Run snip → microcompact → full_compact sequentially
3. Replace messages with compacted result
4. Retry the API call once
5. If retry also fails, propagate the error

### Guard
A boolean `already_retried` prevents infinite loops. Only one reactive compact per query loop iteration.

---

## 7. Query Engine Integration (`src/query/engine.rs`)

### Replace lines 49-73

The current simple compaction block is replaced with the pipeline:

```rust
// Proactive compaction pipeline
let cw = ctx.provider.model_info().context_window;

// Helper: compute ratio without capturing ctx (avoids borrow conflict)
fn token_ratio(messages: &[ApiMessage], context_window: u64) -> f64 {
    estimate_message_tokens(messages) as f64 / context_window as f64
}

// Stage 1: Snip (cheapest — 3 turns preserved)
if token_ratio(&ctx.messages, cw) > 0.60 {
    let freed = compact::snip::snip(&mut ctx.messages, 3);
    if freed > 0 {
        let _ = event_tx.send(QueryEvent::Compacted {
            stage: "snip".into(), tokens_freed: freed,
        });
    }
}

// Stage 2: Microcompact (preserve last 3 tool results)
if token_ratio(&ctx.messages, cw) > 0.75 {
    let freed = compact::microcompact::microcompact(&mut ctx.messages, 3);
    if freed > 0 {
        let _ = event_tx.send(QueryEvent::Compacted {
            stage: "microcompact".into(), tokens_freed: freed,
        });
    }
}

// Stage 3: Full Compact (LLM call — 3 turns preserved)
if token_ratio(&ctx.messages, cw) > 0.85 && compact_failures < 3 {
    match compact::full_compact::full_compact(
        &ctx.messages, ctx.provider.as_ref(), cw, 3,
    ).await {
        Ok(result) => {
            let freed = result.tokens_freed;
            ctx.messages = result.messages;
            compact_failures = 0;
            let _ = event_tx.send(QueryEvent::Compacted {
                stage: "full".into(), tokens_freed: freed,
            });
        }
        Err(e) => {
            compact_failures += 1;
            let _ = event_tx.send(QueryEvent::Error(
                format!("Compaction failed ({}/3): {}", compact_failures, e),
            ));
        }
    }
}

// Hard block — only after all proactive stages exhausted
if token_ratio(&ctx.messages, cw) > 0.95 {
    let _ = event_tx.send(QueryEvent::Error(
        "Context window 95% exceeded after all compaction stages. Cannot continue.".into()
    ));
    break;
}
```

### Reactive compact (after API error)

```rust
// In the streaming error handler:
Err(e) if is_context_overflow_error(&e) && !already_retried => {
    // Run full pipeline (3 turns preserved at each stage)
    let _ = compact::snip::snip(&mut ctx.messages, 3);
    let _ = compact::microcompact::microcompact(&mut ctx.messages, 3);
    if let Ok(result) = compact::full_compact::full_compact(
        &ctx.messages, ctx.provider.as_ref(), cw, 3,
    ).await {
        ctx.messages = result.messages;
        let _ = event_tx.send(QueryEvent::Compacted {
            stage: "reactive".into(),
            tokens_freed: result.tokens_freed,
        });
    }
    already_retried = true;
    continue; // retry the loop
}
```

### New variables in query loop

```rust
let mut compact_failures: u32 = 0;
let mut already_retried = false; // resets at start of each loop iteration
```

`already_retried` resets to `false` at the start of each loop iteration (before the API call). This allows reactive compact to trigger once per turn. The `compact_failures` counter persists across the entire query (circuit breaker for full_compact).

### Concurrency note

Each query has its own `ctx.messages` (owned, not shared). Agent sub-queries (via AgentTool) operate on their own message list. No concurrent access to the same message list is possible, so no locking is needed.

---

## 8. QueryEvent Addition

Add to `QueryEvent` enum in `src/query/types.rs`:

```rust
/// Context was compacted to fit token budget
Compacted {
    stage: String,      // "snip", "microcompact", "full", "reactive"
    tokens_freed: u64,
},
```

---

## 9. TUI Integration

In `src/tui/app.rs`, handle `QueryEvent::Compacted`:

```rust
QueryEvent::Compacted { stage, tokens_freed } => {
    self.messages.push(DisplayMessage::Text {
        role: TextRole::System,
        content: format!("[Context compacted ({}): ~{}K tokens freed]",
            stage, tokens_freed / 1000),
    });
}
```

**Status bar limitation:** The status bar `ctx: N%` uses `app.token_count / app.context_window`, where `token_count` is a cumulative counter from `QueryEvent::Usage` (actual API usage). It does NOT decrease after compaction. This means `ctx%` may show a stale high value after compaction. This is acceptable — the value reflects total session token usage, not current context size. A future improvement could add an `estimated_context_usage` field updated from `estimate_message_tokens()` after each compaction.

---

## 10. Token Estimation

The existing `estimate_message_tokens()` in `src/services/token_estimation/mod.rs` is sufficient for compaction decisions (heuristic ~4 chars/token). Real usage comes from API responses.

No changes needed to token estimation.

---

## Dependencies

No new crate dependencies. Uses existing:
- `crate::services::api::provider::LlmProvider` for full_compact LLM calls
- `crate::services::token_estimation` for estimates
- `crate::types::message::{ApiMessage, Role}`
- `crate::types::content::ContentBlock`

---

## Testing Strategy

- **snip.rs:** Unit tests — snip with various message counts, boundary preservation, empty messages
- **microcompact.rs:** Unit tests — truncation of long tool results, preservation of recent ones, image removal
- **full_compact.rs:** Unit test with mock provider — verify message structure of result
- **types.rs:** Display trait tests
- **Integration:** engine.rs compaction triggers tested via end-to-end query with mock provider returning known token counts
- **Reactive:** Test error detection patterns with various error message formats
