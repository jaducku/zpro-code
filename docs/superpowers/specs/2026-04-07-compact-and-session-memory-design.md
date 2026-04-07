# /compact Command & Session Memory Design

Faithful port of the original TypeScript `/compact` command, session memory extraction system, and auto-compact integration.

## Problem

- `/compact` and `/clear` both return `LocalCommandResult::Compact` and do the same thing (clear all messages)
- `/compact` doesn't invoke the compaction pipeline — no LLM summarization
- No session memory infrastructure exists
- Auto-compact in `engine.rs` uses staged ratio thresholds (0.60/0.75/0.85), diverging from the original's single-threshold REPL-level approach

## Architecture Overview

```
REPL loop (run.rs)
  ├─ /compact command → CompactCommand.call()
  │   ├─ try session memory compaction (no API call)
  │   ├─ fallback: microcompact → full_compact (LLM call)
  │   └─ return CompactResult → handler replaces api_messages
  │
  ├─ /clear command → ClearCommand.call()
  │   └─ return Clear → handler clears api_messages
  │
  ├─ query() → QueryCompleted
  │   ├─ post-sampling: session memory extraction (background fork)
  │   └─ auto_compact_if_needed()
  │       ├─ try session memory compaction
  │       └─ fallback: microcompact → full_compact
  │
  └─ query() → API prompt-too-long error
      └─ reactive compact (session memory → snip → microcompact → full_compact)
```

## Module Structure

### New: `src/services/session_memory/`

```
mod.rs           — pub init_session_memory(), SessionMemoryState
types.rs         — SessionMemoryConfig, extraction tracking state
prompts.rs       — DEFAULT_TEMPLATE, build_update_prompt(), section analysis, truncation
extraction.rs    — should_extract_memory(), extract_session_memory() (fork subagent)
utils.rs         — get_session_memory_content(), last_summarized_message_id, wait_for_extraction
compaction.rs    — try_session_memory_compaction(), calculate_messages_to_keep()
```

### Modified: `src/services/compact/`

```
prompt.rs        — NEW: NO_TOOLS_PREAMBLE, BASE_COMPACT_PROMPT (9-section), NO_TOOLS_TRAILER,
                   get_compact_prompt(), format_compact_summary(), get_compact_user_summary_message()
full_compact.rs  — Use prompt.rs, add PTL retry logic, abort signal support
types.rs         — CompactResult expanded, CompactStage::SessionMemory added
mod.rs           — auto_compact_if_needed(), get_auto_compact_threshold(), post_compact_cleanup()
```

### Modified: `src/commands/`

```
context.rs       — CommandContext gains provider, messages, session_memory
types.rs         — LocalCommandResult::Clear added, Compact holds CompactResult
local/compact.rs — Full compaction pipeline execution
local/clear.rs   — Returns LocalCommandResult::Clear
```

### Modified: `src/query/engine.rs`

- Remove staged ratio checks (0.60/0.75/0.85)
- Keep reactive compact on prompt-too-long error
- Add post-sampling session memory extraction trigger

### Modified: `src/tui/run.rs`

- Handler: separate Clear vs Compact result paths
- CommandContext: inject real provider, messages, session_memory
- After QueryCompleted: call auto_compact_if_needed()
- Loading state feedback during compaction

---

## Section 1: Session Memory Service

### 1.1 Template (prompts.rs)

Original template ported verbatim:

```markdown
# Session Title
_A short and distinctive 5-10 word descriptive title for the session._

# Current State
_What is actively being worked on right now? Pending tasks. Immediate next steps._

# Task specification
_What did the user ask to build? Design decisions or explanatory context._

# Files and Functions
_Important files, what they contain and why they are relevant._

# Workflow
_Bash commands usually run and in what order. How to interpret output._

# Errors & Corrections
_Errors encountered and how fixed. User corrections. Failed approaches._

# Codebase and System Documentation
_Important system components. How they work/fit together._

# Learnings
_What worked well. What has not. What to avoid._

# Key results
_If the user asked for specific output, repeat the exact result here._

# Worklog
_Step by step, what was attempted and done. Very terse summary._
```

Customizable via `~/.claude/session-memory/config/template.md`.

### 1.2 Extraction Prompt (prompts.rs)

Original update prompt ported. Key rules:
- ONLY use Edit tool on the session memory file
- Maintain exact section structure (headers + italic descriptions preserved)
- Write detailed, info-dense content
- Keep each section under ~2000 tokens
- Total file budget: 12,000 tokens
- Always update "Current State" section
- Generate section size reminders when overbudget

Variable substitution: `{{currentNotes}}`, `{{notesPath}}`

### 1.3 Extraction Trigger (extraction.rs)

```rust
pub fn should_extract_memory(state: &SessionMemoryState, current_token_count: u64) -> bool
```

Conditions (original logic):
1. Initialization gate: `current_token_count >= config.minimum_message_tokens_to_init` (default: 10,000)
2. After initialization, BOTH required:
   - Token growth: `current_token_count - tokens_at_last_extraction >= config.minimum_tokens_between_update` (default: 5,000)
   - Tool calls: `tool_calls_since_last >= config.tool_calls_between_updates` (default: 3)
3. OR: token growth met + no tool calls in last assistant turn (natural break)

### 1.4 Extraction Execution (extraction.rs)

```rust
pub async fn extract_session_memory(
    state: &SessionMemoryState,
    messages: &[ApiMessage],
    provider: &Arc<dyn LlmProvider>,
) -> Result<(), AtomError>
```

1. Mark extraction started (timestamp)
2. Set up session memory file (create if not exists, read current)
3. Build update prompt with current content + section reminders
4. Fork subagent via existing `fork_subagent.rs` infrastructure:
   - Restricted tools: only Edit on memory file path
   - query_source: `"session_memory"` (recursion guard)
5. Record extraction token count
6. Update `last_summarized_message_id` (only if last turn has no tool calls)
7. Mark extraction completed

### 1.5 State (types.rs)

```rust
pub struct SessionMemoryConfig {
    pub minimum_message_tokens_to_init: u64,    // 10,000
    pub minimum_tokens_between_update: u64,     // 5,000
    pub tool_calls_between_updates: usize,      // 3
}

pub struct SessionMemoryState {
    config: SessionMemoryConfig,
    initialized: AtomicBool,
    last_summarized_message_id: RwLock<Option<String>>,
    extraction_started_at: RwLock<Option<Instant>>,
    tokens_at_last_extraction: AtomicU64,
    last_memory_message_index: RwLock<Option<usize>>,
}
```

Thread-safe: extraction runs in background, compaction reads state concurrently.

### 1.6 File I/O (utils.rs)

- `get_session_memory_path()`: `~/.claude/session-memory/{session_id}.md`
- `get_session_memory_content()`: read file, return `None` if not exists
- `is_session_memory_empty()`: compare trimmed content to template
- `wait_for_session_memory_extraction()`: poll extraction_started_at with 15s timeout, 1s intervals. Stale threshold: 60s.

---

## Section 2: Session Memory Compaction

### 2.1 Entry Point (compaction.rs)

```rust
pub async fn try_session_memory_compaction(
    messages: &[ApiMessage],
    state: &SessionMemoryState,
    auto_compact_threshold: Option<u64>,
) -> Option<CompactResult>
```

Returns `None` when:
- Session memory content doesn't exist or is empty (matches template)
- `last_summarized_message_id` not found in current messages
- Post-compact token count exceeds `auto_compact_threshold` (if provided)

### 2.2 Message Retention (compaction.rs)

```rust
pub fn calculate_messages_to_keep_index(
    messages: &[ApiMessage],
    last_summarized_index: usize,
    config: &SessionMemoryCompactConfig,
) -> usize
```

Session memory compact config (separate from extraction config):
```rust
pub struct SessionMemoryCompactConfig {
    pub min_tokens: u64,              // 10,000
    pub min_text_block_messages: usize, // 5
    pub max_tokens: u64,              // 40,000
}
```

Algorithm:
1. Start from `last_summarized_index + 1`
2. Expand backwards until BOTH minimums met (token count + text block message count)
3. Stop expanding if `max_tokens` reached
4. Adjust index to preserve tool_use/tool_result pairs (scan kept messages for orphaned tool_results, include their matching tool_use)
5. Filter out old compact boundary messages from kept set

### 2.3 Result Construction

```rust
// boundary marker (User message)
let boundary = ApiMessage {
    role: Role::User,
    content: vec![ContentBlock::Text {
        text: format!("{}: session memory compaction]", COMPACT_BOUNDARY_PREFIX),
    }],
};

// summary message (User message with formatted session memory)
let summary_content = get_compact_user_summary_message(
    &truncated_memory,
    true,  // suppress_follow_up_questions (for auto-compact)
    transcript_path.as_deref(),
    true,  // recent_messages_preserved
);

let messages = vec![boundary, summary_msg]
    .into_iter()
    .chain(messages_to_keep)
    .collect();
```

---

## Section 3: Compact Prompt (prompt.rs)

### 3.1 Prompt Components

Ported verbatim from original `services/compact/prompt.ts`:

**NO_TOOLS_PREAMBLE:**
```
CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.

- Do NOT use Read, Bash, Grep, Glob, Edit, Write, or ANY other tool.
- You already have all the context you need in the conversation above.
- Tool calls will be REJECTED and will waste your only turn — you will fail the task.
- Your entire response must be plain text: an <analysis> block followed by a <summary> block.
```

**BASE_COMPACT_PROMPT:** 9-section structure with `<analysis>` + `<summary>` XML format. Sections:
1. Primary Request and Intent
2. Key Technical Concepts
3. Files and Code Sections
4. Errors and fixes
5. Problem Solving
6. All user messages (critical for feedback tracking)
7. Pending Tasks
8. Current Work (most recent messages priority)
9. Optional Next Step (with direct quotes from conversation)

**NO_TOOLS_TRAILER:**
```
REMINDER: Do NOT call any tools. Respond with plain text only — an <analysis> block followed by a <summary> block. Tool calls will be rejected and you will fail the task.
```

### 3.2 Public Functions

```rust
pub fn get_compact_prompt(custom_instructions: Option<&str>) -> String
// NO_TOOLS_PREAMBLE + BASE_COMPACT_PROMPT + optional "Additional Instructions:\n{}" + NO_TOOLS_TRAILER

pub fn format_compact_summary(summary: &str) -> String
// 1. Strip <analysis>...</analysis>
// 2. Replace <summary>...</summary> with "Summary:\n{content}"
// 3. Collapse excess whitespace

pub fn get_compact_user_summary_message(
    summary: &str,
    suppress_follow_up_questions: bool,
    transcript_path: Option<&str>,
    recent_messages_preserved: bool,
) -> String
// Wraps formatted summary with:
// "This session is being continued from a previous conversation..."
// + optional transcript path reference
// + optional "Recent messages are preserved verbatim."
// + optional "Continue... without asking further questions. Resume directly."
```

---

## Section 4: Full Compact Upgrade (full_compact.rs)

### 4.1 Changes

- Replace `SUMMARY_SYSTEM_PROMPT` with `get_compact_prompt()` from prompt.rs
- System prompt becomes a minimal instruction; the detailed prompt goes in the user message
- Add custom instructions passthrough
- Add PTL retry logic
- Add abort signal support

### 4.2 PTL Retry

```rust
const MAX_PTL_RETRIES: u32 = 3;

// In full_compact():
for attempt in 0..=MAX_PTL_RETRIES {
    match provider.create_message(request.clone()).await {
        Ok(stream) => { /* collect summary */ break; }
        Err(e) if is_prompt_too_long(&e) && attempt < MAX_PTL_RETRIES => {
            // Drop oldest ~20% of message groups from to_summarize
            // Prepend "[earlier conversation truncated for compaction retry]"
            // Rebuild request
            continue;
        }
        Err(e) => return Err(e),
    }
}
```

### 4.3 Abort Signal

```rust
pub async fn full_compact(
    messages: &[ApiMessage],
    provider: &dyn LlmProvider,
    context_window: u64,
    keep_turns: usize,
    custom_instructions: Option<&str>,
    abort_signal: &AbortSignal,         // NEW
) -> Result<CompactResult, AtomError>
```

Check `abort_signal.is_cancelled()` between stream chunks.

### 4.4 Boundary Marker

```rust
let boundary = ApiMessage {
    role: Role::User,
    content: vec![ContentBlock::Text {
        text: format!(
            "{}: {} messages summarized by LLM. ~{}K tokens before compaction]",
            COMPACT_BOUNDARY_PREFIX,
            to_summarize.len(),
            tokens_before / 1000
        ),
    }],
};
```

### 4.5 Summary Max Tokens

```rust
const COMPACT_MAX_OUTPUT_TOKENS: u32 = 20_000;
// Capped at min(COMPACT_MAX_OUTPUT_TOKENS, model.max_output_tokens)
```

---

## Section 5: CommandContext & Result Types

### 5.1 CommandContext (context.rs)

```rust
pub struct CommandContext {
    // Existing
    pub app_state: Arc<RwLock<AppState>>,
    pub abort_signal: AbortSignal,
    pub cwd: PathBuf,
    pub message_tx: mpsc::UnboundedSender<MessageUpdate>,
    pub ui_event_tx: mpsc::UnboundedSender<UiEvent>,
    pub options: CommandOptions,
    // New
    pub provider: Option<Arc<dyn LlmProvider>>,
    pub messages: Arc<Vec<ApiMessage>>,
    pub session_memory: Option<Arc<SessionMemoryState>>,
}
```

### 5.2 LocalCommandResult (types.rs)

```rust
pub enum LocalCommandResult {
    Text { value: String },
    Compact { result: CompactResult },
    Clear,
    Skip,
}
```

### 5.3 CompactResult (compact/types.rs)

```rust
pub struct CompactResult {
    pub messages: Vec<ApiMessage>,
    pub tokens_freed: u64,
    pub stage: CompactStage,
    pub pre_compact_token_count: Option<u64>,
    pub post_compact_token_count: Option<u64>,
}

pub enum CompactStage {
    Snip,
    Microcompact,
    FullCompact,
    Reactive,
    SessionMemory,
}
```

### 5.4 Error Types

```rust
// In compact/types.rs or AtomError
pub const ERROR_MESSAGE_USER_ABORT: &str = "Compaction canceled.";
pub const ERROR_MESSAGE_NOT_ENOUGH_MESSAGES: &str = "No messages to compact";
pub const ERROR_MESSAGE_INCOMPLETE_RESPONSE: &str = "Compaction produced incomplete response";
```

---

## Section 6: /compact Command (local/compact.rs)

### 6.1 Execution Flow

```rust
async fn call(&self, args: &str, ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
    let provider = ctx.provider.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No provider available"))?;
    let messages = get_messages_after_compact_boundary(&ctx.messages);

    if messages.is_empty() {
        return Err(anyhow::anyhow!(ERROR_MESSAGE_NOT_ENOUGH_MESSAGES));
    }

    let custom_instructions = args.trim();
    let custom = if custom_instructions.is_empty() { None } else { Some(custom_instructions) };

    // 1. Try session memory compaction (no custom instructions only)
    if custom.is_none() {
        if let Some(sm) = &ctx.session_memory {
            if let Some(result) = try_session_memory_compaction(messages, sm, None).await {
                return Ok(LocalCommandResult::Compact { result });
            }
        }
    }

    // 2. Microcompact (reduce tokens before LLM call)
    let mut work_messages = messages.to_vec();
    microcompact(&mut work_messages, 3);

    // 3. Full compact (LLM summarization)
    let context_window = provider.model_info().context_window;
    let result = full_compact(
        &work_messages, provider.as_ref(), context_window, 3,
        custom, &ctx.abort_signal,
    ).await?;

    Ok(LocalCommandResult::Compact { result })
}
```

### 6.2 /clear Command (local/clear.rs)

```rust
async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
    Ok(LocalCommandResult::Clear)
}
```

---

## Section 7: Auto-Compact (compact/mod.rs)

### 7.1 Threshold

```rust
const AUTOCOMPACT_BUFFER_TOKENS: u64 = 13_000;

pub fn get_auto_compact_threshold(context_window: u64) -> u64 {
    // Effective = context_window - max_output_for_summary
    let effective = context_window.saturating_sub(20_000);
    effective.saturating_sub(AUTOCOMPACT_BUFFER_TOKENS)
}
```

### 7.2 auto_compact_if_needed()

Called from run.rs after QueryCompleted event.

```rust
pub async fn auto_compact_if_needed(
    messages: &mut Vec<ApiMessage>,
    provider: &Arc<dyn LlmProvider>,
    session_memory: Option<&Arc<SessionMemoryState>>,
    consecutive_failures: &mut u32,
    snip_tokens_freed: u64,
) -> Option<CompactResult>
```

Flow:
1. Circuit breaker: `consecutive_failures >= 3` → return None
2. Token check: `estimate_message_tokens(messages) - snip_tokens_freed < threshold` → return None
3. Try session memory compaction (with threshold)
4. Fallback: microcompact → full_compact (suppress_follow_up_questions: true)
5. On success: `*consecutive_failures = 0`, return result
6. On failure: `*consecutive_failures += 1`, return None

### 7.3 post_compact_cleanup()

```rust
pub fn post_compact_cleanup() {
    // Reset microcompact state (if any cached state exists)
    // Clear any memoized caches relevant to compaction
}
```

Minimal in Rust — the TS version resets many JS-specific caches (memoized functions, growthbook state, etc.) that don't have Rust equivalents. Keep as an extension point.

---

## Section 8: Engine.rs Changes

### 8.1 Remove Staged Ratio Checks

Delete lines 51-105 (the ratio-based snip/microcompact/full_compact cascade).

### 8.2 Reactive Compact (keep, enhance)

```rust
Err(e) if is_context_overflow_error(&e) && !already_retried => {
    // 1. Try session memory compaction
    if let Some(ref sm) = ctx.session_memory {
        if let Some(result) = try_session_memory_compaction(&ctx.messages, sm, None).await {
            ctx.messages = result.messages;
            let _ = event_tx.send(QueryEvent::Compacted {
                stage: "session_memory".into(),
                tokens_freed: result.tokens_freed,
            });
            already_retried = true;
            continue;
        }
    }
    // 2. Fallback: snip → microcompact → full_compact
    let _ = snip(&mut ctx.messages, 3);
    let _ = microcompact(&mut ctx.messages, 3);
    if let Ok(result) = full_compact(...).await {
        ctx.messages = result.messages;
        let _ = event_tx.send(QueryEvent::Compacted {
            stage: "reactive".into(),
            tokens_freed: result.tokens_freed,
        });
    }
    already_retried = true;
    continue;
}
```

### 8.3 Post-Sampling Hook

After query completes with no tool calls (natural end):

```rust
if tool_calls.is_empty() {
    // Session memory extraction (non-blocking background)
    if let Some(ref sm) = ctx.session_memory {
        let sm = sm.clone();
        let messages = ctx.messages.clone();
        let provider = ctx.provider.clone();
        tokio::spawn(async move {
            sm.maybe_extract(&messages, &provider).await;
        });
    }
    // ... return Ok(QueryResult { ... })
}
```

---

## Section 9: TUI Integration (run.rs)

### 9.1 Handler Changes

```rust
Ok(LocalCommandResult::Compact { result }) => {
    app.api_messages = result.messages;
    app.messages.clear();
    app.needs_redraw = true;
    app.push_message(DisplayMessage::Text {
        role: TextRole::System,
        content: format!(
            "[Context compacted ({}): ~{}K tokens freed]",
            result.stage, result.tokens_freed / 1000
        ),
    });
}
Ok(LocalCommandResult::Clear) => {
    app.messages.clear();
    app.api_messages.clear();
    app.needs_redraw = true;
    app.push_message(DisplayMessage::Text {
        role: TextRole::System,
        content: "Conversation cleared.".into(),
    });
}
```

### 9.2 Auto-Compact After Query

```rust
// After QueryCompleted handling
if is_query_completed {
    // Auto-compact check
    if let Some(ref provider) = provider {
        if let Some(result) = auto_compact_if_needed(
            &mut app.api_messages,
            provider,
            session_memory.as_ref(),
            &mut compact_failures,
            snip_tokens_freed,
        ).await {
            app.messages.clear();
            app.needs_redraw = true;
            app.push_message(DisplayMessage::Text {
                role: TextRole::System,
                content: format!(
                    "[Context auto-compacted ({}): ~{}K tokens freed]",
                    result.stage, result.tokens_freed / 1000
                ),
            });
        }
    }
}
```

### 9.3 Loading Feedback

Before compact execution in handler:
```rust
app.is_loading = true;
app.loading_label = Some("Compacting...".into());
```

After completion:
```rust
app.is_loading = false;
app.loading_label = None;
```

### 9.4 CommandContext Injection

Replace dummy CommandContext with real values:
```rust
let ctx = CommandContext {
    app_state: app_state.clone(),
    abort_signal: app.active_abort_signal.clone()
        .unwrap_or_else(AbortSignal::new),
    cwd: std::env::current_dir().unwrap_or_default(),
    message_tx: msg_tx,
    ui_event_tx: ui_tx,
    options: CommandOptions { theme: "default".into() },
    provider: provider.clone(),
    messages: Arc::new(app.api_messages.clone()),
    session_memory: session_memory.clone(),
};
```

---

## Section 10: Helper Functions

### 10.1 get_messages_after_compact_boundary()

```rust
pub fn get_messages_after_compact_boundary(messages: &[ApiMessage]) -> &[ApiMessage] {
    let last_boundary = messages.iter().rposition(|m| {
        m.content.iter().any(|c| matches!(
            c,
            ContentBlock::Text { text } if text.starts_with(COMPACT_BOUNDARY_PREFIX)
        ))
    });
    match last_boundary {
        Some(idx) => &messages[idx..],
        None => messages,
    }
}
```

### 10.2 adjust_index_to_preserve_api_invariants()

For session memory compaction — ensures tool_use/tool_result pairs aren't split:

```rust
pub fn adjust_index_to_preserve_api_invariants(
    messages: &[ApiMessage],
    start_index: usize,
) -> usize
```

Scans kept messages for ToolResult blocks, finds matching ToolUse in earlier messages, expands start_index backward to include them.

### 10.3 truncate_session_memory_for_compact()

Per-section truncation before inserting into compact summary:
- Max ~2000 tokens per section (8000 chars)
- Truncates at line boundaries
- Appends `[... section truncated for length ...]`
- Returns `(truncated_content, was_truncated)`

---

## Constants Summary

| Constant | Value | Usage |
|----------|-------|-------|
| AUTOCOMPACT_BUFFER_TOKENS | 13,000 | Auto-compact threshold buffer |
| COMPACT_MAX_OUTPUT_TOKENS | 20,000 | Max summary output tokens |
| MAX_PTL_RETRIES | 3 | Prompt-too-long retry limit |
| MAX_CONSECUTIVE_FAILURES | 3 | Circuit breaker threshold |
| SM_MIN_TOKENS | 10,000 | Session memory keep minimum tokens |
| SM_MIN_TEXT_MESSAGES | 5 | Session memory keep minimum messages |
| SM_MAX_TOKENS | 40,000 | Session memory keep maximum tokens |
| SM_INIT_TOKENS | 10,000 | Extraction initialization threshold |
| SM_UPDATE_TOKENS | 5,000 | Extraction growth threshold |
| SM_TOOL_CALLS | 3 | Extraction tool call threshold |
| SM_MAX_SECTION_LENGTH | 2,000 | Per-section token limit (template) |
| SM_MAX_TOTAL_TOKENS | 12,000 | Total session memory token limit |
| EXTRACTION_WAIT_TIMEOUT | 15s | Wait for in-progress extraction |
| EXTRACTION_STALE_THRESHOLD | 60s | Skip stale extractions |

---

## Review Findings — Addressed

Issues identified during spec review against original TS, now incorporated:

### C1: Image stripping before compact API call

The original `compact.ts` (lines 146-202) strips image/document blocks from messages before the summary API call. Without this, image-heavy sessions will hit prompt-too-long on the compact request itself.

**Resolution:** Add `strip_images_from_messages()` step in full_compact before building the summary request. Replace `ContentBlock::Image` with `ContentBlock::Text { text: "[image was shared]" }`.

### C2: Summary message role — User, not Assistant

The existing Rust `full_compact.rs` line 76 creates the summary as `Role::Assistant`. The original TS creates it as a User message (`createUserMessage`). This is a required change — the summary must be `Role::User`.

### C3/C4: buildPostCompactMessages ordering

Add explicit `build_post_compact_messages()` function in compact/mod.rs:
```rust
pub fn build_post_compact_messages(result: &CompactResult) -> Vec<ApiMessage> {
    // boundary → summary → kept messages (already flattened in result.messages)
    result.messages.clone()
}
```

The flat `messages` field in `CompactResult` is an intentional simplification. The original's separate fields (`boundaryMarker`, `summaryMessages`, `messagesToKeep`, `attachments`, `hookResults`) exist for transcript annotation and delta attachment composition, which are not yet ported. If needed later, `CompactResult` can be expanded.

### I1: Threshold calculation respects model max_output

```rust
pub fn get_auto_compact_threshold(context_window: u64, max_output_tokens: u32) -> u64 {
    let max_summary = std::cmp::min(max_output_tokens as u64, 20_000);
    let effective = context_window.saturating_sub(max_summary);
    effective.saturating_sub(AUTOCOMPACT_BUFFER_TOKENS)
}
```

### I2: shouldAutoCompact recursion guards

Add to `auto_compact_if_needed()`:
```rust
// Recursion guards
if query_source == Some("session_memory") || query_source == Some("compact") {
    return None;
}
// Env var check
if std::env::var("DISABLE_COMPACT").is_ok() || std::env::var("DISABLE_AUTO_COMPACT").is_ok() {
    return None;
}
```

Updated signature:
```rust
pub async fn auto_compact_if_needed(
    messages: &mut Vec<ApiMessage>,
    provider: &Arc<dyn LlmProvider>,
    session_memory: Option<&Arc<SessionMemoryState>>,
    consecutive_failures: &mut u32,
    snip_tokens_freed: u64,
    query_source: Option<&str>,      // NEW — recursion guard
) -> Option<CompactResult>
```

### I3: Reset lastSummarizedMessageId after full compact

In `/compact` command after successful full_compact:
```rust
if let Some(sm) = &ctx.session_memory {
    sm.reset_last_summarized_message_id();
}
```

### I4: Wait for extraction before session memory compaction

Inside `try_session_memory_compaction()`:
```rust
state.wait_for_extraction().await;  // 15s timeout, 1s poll
let content = get_session_memory_content(&session_memory_path).await;
```

### I5: Thinking block dedup

Current Rust `ApiMessage` doesn't have a `message.id` field — streaming produces distinct messages rather than split entries with shared IDs. This issue doesn't apply. Noted for future reference if streaming behavior changes.

### I6: SUMMARY_MAX_TOKENS change noted

Changing from 4,096 to 20,000 is a 5x increase. This matches the original TS `COMPACT_MAX_OUTPUT_TOKENS`. The increase is necessary for the detailed 9-section prompt to produce adequate summaries.

### I7: Sequential extraction execution

Session memory extraction must be serialized — concurrent extractions would race on the same file.

```rust
// In SessionMemoryState
extraction_lock: tokio::sync::Mutex<()>,

// In maybe_extract()
let _guard = self.extraction_lock.try_lock();
if _guard.is_err() {
    return; // Another extraction already in progress
}
```

### S6: Flat text vs real messages for compact summarization

Current Rust serializes conversation to flat text in one User message. The original sends real messages to a forked agent with cache sharing. For now, keep flat text serialization (simpler, already working) but add the structured compact prompt. The prompt quality improvement is the primary win. Cache-sharing optimization is deferred.

---

## Not Ported (Original Stubs or N/A)

- **Reactive compact module** — stub in original, not ported
- **snipCompact module** — stub in original; current Rust snip.rs is more complete, retained as-is
- **Cached microcompact** — feature-gated in original, not ported (current microcompact is functional)
- **Prompt cache sharing** — forked-agent cache optimization, deferred to future work
- **Delta attachments** (tool/agent/MCP re-announcement) — no deferred tools system in Rust, deferred
- **GrowthBook feature flags** — replaced with compile-time feature flags or env vars
- **Telemetry/analytics events** — replaced with tracing::info/warn
- **Context collapse** — separate system, not in scope
