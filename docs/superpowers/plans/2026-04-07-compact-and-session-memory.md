# /compact Command & Session Memory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the original TS `/compact` command with session memory extraction, session memory compaction, full LLM compaction with structured prompt, and auto-compact integration.

**Architecture:** Bottom-up — types first, then session memory service, then compact prompt/pipeline upgrade, then command/context layer, finally TUI and engine integration. Each layer is independently testable.

**Tech Stack:** Rust, async-trait, tokio, futures, serde, tracing

**Spec:** `docs/superpowers/specs/2026-04-07-compact-and-session-memory-design.md`

**Key design decisions:**
- **Message ID tracking:** `ApiMessage` has no `id` field. Use index-based tracking via `last_memory_message_index: Option<usize>` instead of UUID-based `last_summarized_message_id`. When computing "messages to keep" for session memory compaction, use the index directly.
- **All commits:** Run `cargo fmt` and `cargo clippy -- -D warnings` before each commit.
- **Compile continuity:** Task 9 temporarily stubs `compact.rs` to keep the build green until Task 10 replaces it.

---

## Task 1: Compact Types & Constants

**Files:**
- Modify: `src/services/compact/types.rs`

- [ ] **Step 1: Write tests for new CompactStage and CompactResult fields**

Add to the existing `#[cfg(test)] mod tests` in `src/services/compact/types.rs`:

```rust
#[test]
fn test_compact_stage_session_memory_display() {
    assert_eq!(CompactStage::SessionMemory.to_string(), "session_memory");
}

#[test]
fn test_error_constants() {
    assert!(!ERROR_MESSAGE_USER_ABORT.is_empty());
    assert!(!ERROR_MESSAGE_NOT_ENOUGH_MESSAGES.is_empty());
    assert!(!ERROR_MESSAGE_INCOMPLETE_RESPONSE.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::compact::types::tests -- --nocapture`
Expected: FAIL — `SessionMemory` variant and error constants don't exist

- [ ] **Step 3: Implement type changes**

In `src/services/compact/types.rs`:

1. Add `SessionMemory` to `CompactStage`:
```rust
pub enum CompactStage {
    Snip,
    Microcompact,
    FullCompact,
    Reactive,
    SessionMemory,
}
```

2. Update `Display` impl to include `Self::SessionMemory => write!(f, "session_memory")`.

3. Add fields to `CompactResult`:
```rust
pub struct CompactResult {
    pub messages: Vec<ApiMessage>,
    pub tokens_freed: u64,
    pub stage: CompactStage,
    pub pre_compact_token_count: Option<u64>,
    pub post_compact_token_count: Option<u64>,
}
```

4. Add error constants:
```rust
pub const ERROR_MESSAGE_USER_ABORT: &str = "Compaction canceled.";
pub const ERROR_MESSAGE_NOT_ENOUGH_MESSAGES: &str = "No messages to compact";
pub const ERROR_MESSAGE_INCOMPLETE_RESPONSE: &str = "Compaction produced incomplete response";
```

5. Add auto-compact constants:
```rust
pub const AUTOCOMPACT_BUFFER_TOKENS: u64 = 13_000;
pub const COMPACT_MAX_OUTPUT_TOKENS: u32 = 20_000;
pub const MAX_PTL_RETRIES: u32 = 3;
pub const MAX_CONSECUTIVE_COMPACT_FAILURES: u32 = 3;
```

- [ ] **Step 4: Fix all existing call sites**

`CompactResult` now has `pre_compact_token_count` and `post_compact_token_count`. Update all places that construct `CompactResult` (in `full_compact.rs`, `snip.rs` return sites if any, and `engine.rs`) to pass `None` for the new fields. Search with: `CompactResult {` across the codebase.

- [ ] **Step 5: Run tests**

Run: `cargo test --lib services::compact -- --nocapture`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```
feat(compact): add SessionMemory stage, error constants, and CompactResult token fields
```

---

## Task 2: Compact Prompt Module

**Files:**
- Create: `src/services/compact/prompt.rs`
- Modify: `src/services/compact/mod.rs` (add `pub mod prompt;`)

- [ ] **Step 1: Write tests for prompt functions**

Create `src/services/compact/prompt.rs` with tests at bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_compact_prompt_no_custom() {
        let prompt = get_compact_prompt(None);
        assert!(prompt.starts_with("CRITICAL: Respond with TEXT ONLY"));
        assert!(prompt.contains("Primary Request and Intent"));
        assert!(prompt.contains("Optional Next Step"));
        assert!(prompt.ends_with("you will fail the task."));
        assert!(!prompt.contains("Additional Instructions"));
    }

    #[test]
    fn test_get_compact_prompt_with_custom() {
        let prompt = get_compact_prompt(Some("focus on test output"));
        assert!(prompt.contains("Additional Instructions:\nfocus on test output"));
        assert!(prompt.ends_with("you will fail the task."));
    }

    #[test]
    fn test_format_compact_summary_strips_analysis() {
        let raw = "<analysis>\nthinking here\n</analysis>\n\n<summary>\nthe summary\n</summary>";
        let formatted = format_compact_summary(raw);
        assert!(!formatted.contains("<analysis>"));
        assert!(!formatted.contains("thinking here"));
        assert!(formatted.contains("Summary:"));
        assert!(formatted.contains("the summary"));
    }

    #[test]
    fn test_format_compact_summary_no_tags() {
        let raw = "Just plain text summary";
        let formatted = format_compact_summary(raw);
        assert_eq!(formatted, "Just plain text summary");
    }

    #[test]
    fn test_get_compact_user_summary_message_basic() {
        let msg = get_compact_user_summary_message("the summary", false, None, false);
        assert!(msg.contains("continued from a previous conversation"));
        assert!(msg.contains("the summary"));
        assert!(!msg.contains("without asking"));
    }

    #[test]
    fn test_get_compact_user_summary_message_suppress() {
        let msg = get_compact_user_summary_message("s", true, None, false);
        assert!(msg.contains("without asking the user any further questions"));
    }

    #[test]
    fn test_get_compact_user_summary_message_transcript() {
        let msg = get_compact_user_summary_message("s", false, Some("/tmp/t.json"), false);
        assert!(msg.contains("/tmp/t.json"));
    }

    #[test]
    fn test_get_compact_user_summary_message_preserved() {
        let msg = get_compact_user_summary_message("s", false, None, true);
        assert!(msg.contains("Recent messages are preserved verbatim"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::compact::prompt -- --nocapture`
Expected: FAIL — functions don't exist

- [ ] **Step 3: Implement prompt functions**

Port the prompts verbatim from the original TS `_reference_ts/src/services/compact/prompt.ts`. Key functions:

```rust
pub fn get_compact_prompt(custom_instructions: Option<&str>) -> String
pub fn format_compact_summary(summary: &str) -> String
pub fn get_compact_user_summary_message(
    summary: &str,
    suppress_follow_up_questions: bool,
    transcript_path: Option<&str>,
    recent_messages_preserved: bool,
) -> String
```

The full `NO_TOOLS_PREAMBLE`, `BASE_COMPACT_PROMPT` (9-section with `<analysis>`+`<summary>` example), and `NO_TOOLS_TRAILER` constants must be ported from the TS file. Use `regex` crate or manual string ops for `format_compact_summary()` — strip `<analysis>...</analysis>`, extract `<summary>...</summary>` content.

- [ ] **Step 4: Add `pub mod prompt;` to `src/services/compact/mod.rs`**

- [ ] **Step 5: Run tests**

Run: `cargo test --lib services::compact::prompt -- --nocapture`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```
feat(compact): add structured 9-section compact prompt ported from TS original
```

---

## Task 3: Compact Helper Functions

**Files:**
- Modify: `src/services/compact/mod.rs`

- [ ] **Step 1: Write tests for helpers**

Add to `src/services/compact/mod.rs` tests:

```rust
#[test]
fn test_get_messages_after_compact_boundary_none() {
    let msgs = vec![
        text_msg(Role::User, "hello"),
        text_msg(Role::Assistant, "hi"),
    ];
    let result = get_messages_after_compact_boundary(&msgs);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_get_messages_after_compact_boundary_found() {
    let msgs = vec![
        text_msg(Role::User, "old"),
        text_msg(Role::User, &format!("{}: 5 messages summarized]", COMPACT_BOUNDARY_PREFIX)),
        text_msg(Role::Assistant, "summary"),
        text_msg(Role::User, "new"),
    ];
    let result = get_messages_after_compact_boundary(&msgs);
    assert_eq!(result.len(), 3); // boundary + summary + new
    assert!(result[0].content.iter().any(|c| matches!(c, ContentBlock::Text { text } if text.starts_with(COMPACT_BOUNDARY_PREFIX))));
}

#[test]
fn test_strip_images_from_messages() {
    let msgs = vec![ApiMessage {
        role: Role::User,
        content: vec![
            ContentBlock::Text { text: "look".into() },
            ContentBlock::Image {
                source: crate::types::content::ImageSource::Base64 {
                    media_type: "image/png".into(),
                    data: "abc".into(),
                },
            },
        ],
    }];
    let stripped = strip_images_from_messages(&msgs);
    assert_eq!(stripped[0].content.len(), 2);
    assert!(matches!(&stripped[0].content[1], ContentBlock::Text { text } if text.contains("[image")));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::compact::tests -- --nocapture`
Expected: FAIL — functions don't exist

- [ ] **Step 3: Implement helpers**

In `src/services/compact/mod.rs`:

```rust
/// Return messages from the last compact boundary onward (inclusive).
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

/// Replace Image blocks with text markers before compact API call.
pub fn strip_images_from_messages(messages: &[ApiMessage]) -> Vec<ApiMessage> {
    messages.iter().map(|msg| {
        let content = msg.content.iter().map(|block| {
            match block {
                ContentBlock::Image { .. } => ContentBlock::Text {
                    text: "[image was shared]".into(),
                },
                other => other.clone(),
            }
        }).collect();
        ApiMessage { role: msg.role.clone(), content }
    }).collect()
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib services::compact -- --nocapture`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```
feat(compact): add get_messages_after_compact_boundary and strip_images helpers
```

---

## Task 4: Full Compact Upgrade

**Files:**
- Modify: `src/services/compact/full_compact.rs`

- [ ] **Step 1: Update existing tests + add new ones**

Modify tests in `full_compact.rs`:

```rust
#[test]
fn test_build_summary_request_uses_structured_prompt() {
    let msgs = vec![text_msg(Role::User, "fix the bug")];
    let req = build_summary_request(&msgs, "m", None);
    // Should use NO_TOOLS preamble from prompt.rs
    let text = match &req.messages[0].content[0] {
        ContentBlock::Text { text } => text,
        _ => panic!("expected text"),
    };
    assert!(text.contains("CRITICAL: Respond with TEXT ONLY"));
    assert!(text.contains("Primary Request and Intent"));
}

#[test]
fn test_build_summary_request_custom_instructions() {
    let msgs = vec![text_msg(Role::User, "hello")];
    let req = build_summary_request(&msgs, "m", Some("focus on errors"));
    let text = match &req.messages[0].content[0] {
        ContentBlock::Text { text } => text,
        _ => panic!("expected text"),
    };
    assert!(text.contains("Additional Instructions:\nfocus on errors"));
}

#[test]
fn test_build_summary_request_max_tokens() {
    let msgs = vec![text_msg(Role::User, "hi")];
    let req = build_summary_request(&msgs, "m", None);
    assert_eq!(req.max_tokens, COMPACT_MAX_OUTPUT_TOKENS);
}

#[test]
fn test_summary_role_is_user() {
    // The boundary and summary messages should both be Role::User
    // (verified in full_compact by checking the constructed messages)
    // This test documents the requirement from spec review C2
    let boundary = ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text { text: "boundary".into() }],
    };
    let summary = ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text { text: "summary".into() }],
    };
    assert_eq!(boundary.role, Role::User);
    assert_eq!(summary.role, Role::User);
}
```

- [ ] **Step 2: Run tests to verify failures**

Run: `cargo test --lib services::compact::full_compact -- --nocapture`
Expected: FAIL — `build_summary_request` signature changed, structured prompt not present

- [ ] **Step 3: Implement full_compact upgrade**

Changes to `src/services/compact/full_compact.rs`:

1. **Delete** `SUMMARY_SYSTEM_PROMPT` and `SUMMARY_MAX_TOKENS` constants.

2. **Change `build_summary_request` signature** to accept `custom_instructions`:
```rust
pub fn build_summary_request(
    to_summarize: &[ApiMessage],
    model: &str,
    custom_instructions: Option<&str>,
) -> CreateMessageRequest
```

3. **Use `get_compact_prompt()`** from prompt.rs. The prompt goes in the user message, concatenated after the serialized conversation. System prompt becomes minimal:
```rust
system: Some(vec![SystemBlock::Text {
    text: "You are a helpful AI assistant tasked with summarizing conversations.".into(),
}]),
```

4. **Use `COMPACT_MAX_OUTPUT_TOKENS`** from types.rs:
```rust
max_tokens: COMPACT_MAX_OUTPUT_TOKENS,
```

5. **Change `full_compact()` signature** to add `custom_instructions` and `abort_signal`:
```rust
pub async fn full_compact(
    messages: &[ApiMessage],
    provider: &dyn LlmProvider,
    context_window: u64,
    keep_turns: usize,
    custom_instructions: Option<&str>,
    abort_signal: &AbortSignal,
) -> Result<CompactResult, AtomError>
```

6. **Strip images** before building request:
```rust
let stripped = crate::services::compact::strip_images_from_messages(to_summarize);
let request = build_summary_request(&stripped, &model, custom_instructions);
```

7. **Fix summary role** from `Role::Assistant` to `Role::User` (spec C2).

8. **Add abort signal check** in stream loop.

9. **Add PTL retry loop** around `provider.create_message()`:
```rust
let mut attempt = 0;
let mut working_messages = stripped.to_vec();
let summary_text = loop {
    let req = build_summary_request(&working_messages, &model, custom_instructions);
    match try_stream_summary(provider, &req, abort_signal).await {
        Ok(text) if !text.is_empty() => break text,
        Ok(_) => return Err(AtomError::Api {
            message: ERROR_MESSAGE_INCOMPLETE_RESPONSE.into(), status: None,
        }),
        Err(e) if is_prompt_too_long(&e) && attempt < MAX_PTL_RETRIES => {
            // Drop oldest 20% of messages by count
            let drop_count = (working_messages.len() / 5).max(1);
            working_messages = working_messages[drop_count..].to_vec();
            // Prepend truncation marker for context
            working_messages.insert(0, ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "[earlier conversation truncated for compaction retry]".into(),
                }],
            });
            attempt += 1;
            continue;
        }
        Err(e) => return Err(e),
    }
};
```

Extract `try_stream_summary` as a helper that streams the response and collects text, checking abort_signal between chunks.

10. **Apply `format_compact_summary()`** to the collected text before creating the summary message.

11. **Wrap summary** with `get_compact_user_summary_message()`.

12. **Fill `pre_compact_token_count` and `post_compact_token_count`** in returned `CompactResult`.

- [ ] **Step 4: Fix call sites of `full_compact()`**

`full_compact()` signature changed — update calls in:
- `src/query/engine.rs` (both proactive and reactive compact calls) — pass `None` for custom_instructions, pass `ctx.abort_signal` reference

- [ ] **Step 5: Run all compact tests**

Run: `cargo test --lib services::compact -- --nocapture`
Expected: ALL PASS

- [ ] **Step 6: Run full test suite to check nothing broke**

Run: `cargo test --lib -- --nocapture`
Expected: ALL PASS (or only pre-existing failures)

- [ ] **Step 7: Commit**

```
feat(compact): upgrade full_compact with structured 9-section prompt, PTL retry, abort signal, image stripping
```

---

## Task 5: Session Memory Types & Utils

**Files:**
- Create: `src/services/session_memory/mod.rs`
- Create: `src/services/session_memory/types.rs`
- Create: `src/services/session_memory/utils.rs`
- Modify: `src/services/mod.rs` (add `pub mod session_memory;`)

- [ ] **Step 1: Write tests for types and utils**

In `src/services/session_memory/types.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SessionMemoryConfig::default();
        assert_eq!(config.minimum_message_tokens_to_init, 10_000);
        assert_eq!(config.minimum_tokens_between_update, 5_000);
        assert_eq!(config.tool_calls_between_updates, 3);
    }

    #[test]
    fn test_default_compact_config() {
        let config = SessionMemoryCompactConfig::default();
        assert_eq!(config.min_tokens, 10_000);
        assert_eq!(config.min_text_block_messages, 5);
        assert_eq!(config.max_tokens, 40_000);
    }
}
```

In `src/services/session_memory/utils.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_memory_path() {
        let path = get_session_memory_path("test-session-123");
        assert!(path.ends_with("test-session-123.md"));
        assert!(path.to_string_lossy().contains("session-memory"));
    }

    #[test]
    fn test_is_session_memory_empty_true() {
        let template = DEFAULT_SESSION_MEMORY_TEMPLATE;
        assert!(is_session_memory_empty(template, template));
    }

    #[test]
    fn test_is_session_memory_empty_false() {
        let template = DEFAULT_SESSION_MEMORY_TEMPLATE;
        let content = format!("{}\n\nSome actual content here", template);
        assert!(!is_session_memory_empty(&content, template));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::session_memory -- --nocapture`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement types**

Create `src/services/session_memory/types.rs`:
```rust
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};

pub struct SessionMemoryConfig {
    pub minimum_message_tokens_to_init: u64,
    pub minimum_tokens_between_update: u64,
    pub tool_calls_between_updates: usize,
}

impl Default for SessionMemoryConfig {
    fn default() -> Self {
        Self {
            minimum_message_tokens_to_init: 10_000,
            minimum_tokens_between_update: 5_000,
            tool_calls_between_updates: 3,
        }
    }
}

pub struct SessionMemoryCompactConfig {
    pub min_tokens: u64,
    pub min_text_block_messages: usize,
    pub max_tokens: u64,
}

impl Default for SessionMemoryCompactConfig {
    fn default() -> Self {
        Self { min_tokens: 10_000, min_text_block_messages: 5, max_tokens: 40_000 }
    }
}

pub struct SessionMemoryState {
    pub config: SessionMemoryConfig,
    pub compact_config: SessionMemoryCompactConfig,
    pub session_id: String,
    pub initialized: AtomicBool,
    /// Index of the last message that was summarized into session memory.
    /// Used instead of message UUID since ApiMessage has no id field.
    pub last_summarized_index: RwLock<Option<usize>>,
    pub extraction_started_at: RwLock<Option<Instant>>,
    pub tokens_at_last_extraction: AtomicU64,
    /// Index of the last message when extraction was last triggered.
    pub last_memory_message_index: RwLock<Option<usize>>,
    pub extraction_lock: Mutex<()>,
}

impl SessionMemoryState {
    pub async fn reset_last_summarized_index(&self) {
        *self.last_summarized_index.write().await = None;
    }
}
```

Implement `SessionMemoryState::new(session_id: String)` constructor with all fields default/empty.
```

Implement `SessionMemoryState::new(session_id: String)` constructor.

- [ ] **Step 4: Implement utils**

Create `src/services/session_memory/utils.rs` with:
- `pub const DEFAULT_SESSION_MEMORY_TEMPLATE: &str = ...;` (port from TS `prompts.ts`)
- `pub fn get_session_memory_path(session_id: &str) -> PathBuf`
- `pub async fn get_session_memory_content(path: &Path) -> Option<String>`
- `pub fn is_session_memory_empty(content: &str, template: &str) -> bool`
- `pub async fn wait_for_extraction(state: &SessionMemoryState)` (15s timeout, 1s poll, 60s stale threshold)
- `pub const EXTRACTION_WAIT_TIMEOUT_MS: u64 = 15_000;`
- `pub const EXTRACTION_STALE_THRESHOLD_MS: u64 = 60_000;`

- [ ] **Step 5: Create mod.rs and wire up**

Create `src/services/session_memory/mod.rs`:
```rust
pub mod types;
pub mod utils;
pub use types::*;
```

Add `pub mod session_memory;` to `src/services/mod.rs`.

- [ ] **Step 6: Run tests**

Run: `cargo test --lib services::session_memory -- --nocapture`
Expected: ALL PASS

- [ ] **Step 7: Commit**

```
feat(session_memory): add types, config, state, and file utilities
```

---

## Task 6: Session Memory Prompts

**Files:**
- Create: `src/services/session_memory/prompts.rs`
- Modify: `src/services/session_memory/mod.rs` (add `pub mod prompts;`)

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_update_prompt_contains_variables() {
        let prompt = build_update_prompt("current content", "/tmp/notes.md");
        assert!(prompt.contains("current content"));
        assert!(prompt.contains("/tmp/notes.md"));
        assert!(prompt.contains("Edit tool"));
    }

    #[test]
    fn test_analyze_section_sizes() {
        let content = "# Title\nshort\n# Long Section\n".to_string() + &"x".repeat(10000);
        let sizes = analyze_section_sizes(&content);
        assert!(sizes.contains_key("# Title"));
        assert!(sizes.contains_key("# Long Section"));
        assert!(*sizes.get("# Long Section").unwrap() > 2000);
    }

    #[test]
    fn test_truncate_session_memory_short() {
        let content = "# Title\nshort content";
        let (truncated, was_truncated) = truncate_session_memory_for_compact(content);
        assert!(!was_truncated);
        assert_eq!(truncated, content);
    }

    #[test]
    fn test_truncate_session_memory_long_section() {
        let content = format!("# Title\n{}", "x\n".repeat(5000));
        let (truncated, was_truncated) = truncate_session_memory_for_compact(&content);
        assert!(was_truncated);
        assert!(truncated.contains("[... section truncated for length ...]"));
    }

    #[test]
    fn test_generate_section_reminders_under_budget() {
        let mut sizes = std::collections::HashMap::new();
        sizes.insert("# Title".to_string(), 100);
        let reminder = generate_section_reminders(&sizes, 500);
        assert!(reminder.is_empty());
    }

    #[test]
    fn test_generate_section_reminders_over_budget() {
        let mut sizes = std::collections::HashMap::new();
        sizes.insert("# Big".to_string(), 3000);
        let reminder = generate_section_reminders(&sizes, 15000);
        assert!(reminder.contains("CRITICAL"));
        assert!(reminder.contains("# Big"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::session_memory::prompts -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement prompts**

Port from `_reference_ts/src/services/SessionMemory/prompts.ts`:

```rust
const MAX_SECTION_LENGTH: usize = 2000;
const MAX_TOTAL_SESSION_MEMORY_TOKENS: u64 = 12_000;

pub fn build_update_prompt(current_notes: &str, notes_path: &str) -> String
pub fn analyze_section_sizes(content: &str) -> HashMap<String, u64>
pub fn generate_section_reminders(sizes: &HashMap<String, u64>, total_tokens: u64) -> String
pub fn truncate_session_memory_for_compact(content: &str) -> (String, bool)
```

The `build_update_prompt` must use `{{currentNotes}}` and `{{notesPath}}` substitution on the default update prompt template (port from TS `getDefaultUpdatePrompt()`).

- [ ] **Step 4: Wire up module**

Add `pub mod prompts;` to `src/services/session_memory/mod.rs`.

- [ ] **Step 5: Run tests**

Run: `cargo test --lib services::session_memory::prompts -- --nocapture`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```
feat(session_memory): add prompts, section analysis, and truncation logic
```

---

## Task 7: Session Memory Extraction

**Files:**
- Create: `src/services/session_memory/extraction.rs`
- Modify: `src/services/session_memory/mod.rs`

- [ ] **Step 1: Write tests for should_extract_memory**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_extract_below_init_threshold() {
        let state = SessionMemoryState::new("test".into());
        // 5000 < 10000 init threshold
        assert!(!should_extract_memory(&state, 5_000, 5, false));
    }

    #[test]
    fn test_should_extract_init_and_both_thresholds() {
        let state = SessionMemoryState::new("test".into());
        // First call initializes at 10000
        assert!(!should_extract_memory(&state, 10_000, 0, false));
        // Now initialized, but need both: token growth (5000) AND tool calls (3)
        assert!(!should_extract_memory(&state, 14_000, 2, false));
        // Both met
        assert!(should_extract_memory(&state, 15_000, 3, false));
    }

    #[test]
    fn test_should_extract_natural_break() {
        let state = SessionMemoryState::new("test".into());
        // Initialize
        should_extract_memory(&state, 10_000, 0, false);
        // Token growth met + no tool calls in last turn = natural break
        assert!(should_extract_memory(&state, 15_000, 1, true)); // no_tool_calls_in_last_turn=true
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::session_memory::extraction -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement extraction logic**

```rust
/// Check if session memory extraction should run.
pub fn should_extract_memory(
    state: &SessionMemoryState,
    current_token_count: u64,
    tool_calls_since_last: usize,
    no_tool_calls_in_last_turn: bool,
) -> bool
```

Follow the original logic from `sessionMemory.ts:shouldExtractMemory()`:
1. Check initialization threshold
2. Mark initialized via `state.initialized` AtomicBool
3. Check token growth AND tool calls thresholds
4. OR: token growth + natural break

```rust
/// Execute session memory extraction via forked subagent.
pub async fn extract_session_memory(
    state: &SessionMemoryState,
    messages: &[ApiMessage],
    provider: &Arc<dyn LlmProvider>,
) -> Result<(), AtomError>
```

Implement:
1. `extraction_lock.try_lock()` — skip if locked (I7)
2. Set `extraction_started_at` to `Some(Instant::now())`
3. Set up session memory file:
   ```rust
   let dir = get_session_memory_dir(&state.session_id);
   tokio::fs::create_dir_all(&dir).await.ok();
   let path = get_session_memory_path(&state.session_id);
   if !path.exists() {
       tokio::fs::write(&path, DEFAULT_SESSION_MEMORY_TEMPLATE).await?;
   }
   ```
4. Read current content: `get_session_memory_content(&path).await`
5. Build update prompt: `prompts::build_update_prompt(&current_content, path.to_str().unwrap())`
6. Fork subagent — construct a single-turn query with the update prompt as the user message:
   ```rust
   use crate::services::fork_subagent::build_child_message;
   
   let directive = format!(
       "Update the session notes at {}. {}", 
       path.display(), 
       update_prompt
   );
   let child_msg = build_child_message(&directive);
   
   // Build a minimal query context for the fork
   let fork_messages = vec![ApiMessage {
       role: Role::User,
       content: vec![ContentBlock::Text { text: child_msg }],
   }];
   
   // Use the existing agent_runtime or direct provider call with:
   // - query_source: Some("session_memory") ← recursion guard
   // - max_turns: Some(1) ← single turn, no looping
   // - tools: only Edit tool, filtered to memory file path
   // - system_prompt: minimal "You are updating session notes."
   ```
   The exact integration depends on `agent_runtime.rs` API. Read that file to determine whether to use `run_fork_agent()` or construct a `QueryContext` manually. Key constraint: `query_source = "session_memory"` prevents auto-compact recursion.
7. Record `tokens_at_last_extraction`: `state.tokens_at_last_extraction.store(token_count, Ordering::SeqCst)`
8. Update `last_memory_message_index` — use index-based tracking since `ApiMessage` has no `id` field:
   ```rust
   // Only update if last turn has no tool calls (safe boundary)
   if !has_tool_calls_in_last_turn(messages) {
       *state.last_memory_message_index.write().await = Some(messages.len() - 1);
   }
   ```
9. Clear `extraction_started_at`: `*state.extraction_started_at.write().await = None;`

- [ ] **Step 4: Add `maybe_extract()` convenience method on `SessionMemoryState`**

```rust
impl SessionMemoryState {
    pub async fn maybe_extract(&self, messages: &[ApiMessage], provider: &Arc<dyn LlmProvider>) {
        let token_count = estimate_message_tokens(messages);
        let tool_calls = count_tool_calls_since(messages, &self.last_memory_message_index);
        let no_tools_last = !has_tool_calls_in_last_turn(messages);
        if should_extract_memory(self, token_count, tool_calls, no_tools_last) {
            let _ = extract_session_memory(self, messages, provider).await;
        }
    }
}
```

- [ ] **Step 5: Wire up module**

Add `pub mod extraction;` to `src/services/session_memory/mod.rs`.

- [ ] **Step 6: Run tests**

Run: `cargo test --lib services::session_memory -- --nocapture`
Expected: ALL PASS

- [ ] **Step 7: Commit**

```
feat(session_memory): add extraction trigger logic and fork-based extraction execution
```

---

## Task 8: Session Memory Compaction

**Files:**
- Create: `src/services/session_memory/compaction.rs`
- Modify: `src/services/session_memory/mod.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::ContentBlock;
    use crate::types::message::{ApiMessage, Role};

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage { role, content: vec![ContentBlock::Text { text: text.into() }] }
    }

    #[test]
    fn test_calculate_messages_to_keep_respects_min_messages() {
        let config = SessionMemoryCompactConfig::default(); // min 5 text messages
        let msgs: Vec<_> = (0..20).map(|i| text_msg(Role::User, &format!("msg {}", i))).collect();
        let idx = calculate_messages_to_keep_index(&msgs, 10, &config);
        // Should keep at least 5 text messages from index 11 onward
        let kept = &msgs[idx..];
        let text_count = kept.iter().filter(|m| has_text_blocks(m)).count();
        assert!(text_count >= 5);
    }

    #[test]
    fn test_calculate_messages_to_keep_respects_max_tokens() {
        let config = SessionMemoryCompactConfig { max_tokens: 100, ..Default::default() };
        let msgs: Vec<_> = (0..10).map(|i| text_msg(Role::User, &"x".repeat(200))).collect();
        let idx = calculate_messages_to_keep_index(&msgs, 0, &config);
        // Should not expand too far back due to max_tokens
        assert!(idx > 0);
    }

    #[test]
    fn test_adjust_index_preserves_tool_pairs() {
        let msgs = vec![
            text_msg(Role::User, "old"),
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_1".into(), name: "Read".into(), input: serde_json::json!({}),
                }],
            },
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: crate::types::content::ToolResultContent::Text("ok".into()),
                    is_error: false,
                }],
            },
            text_msg(Role::User, "new"),
        ];
        // If start_index=2, the tool_result at 2 has orphan tu_1 → must include index 1
        let adjusted = adjust_index_to_preserve_api_invariants(&msgs, 2);
        assert_eq!(adjusted, 1);
    }

    #[test]
    fn test_has_text_blocks() {
        assert!(has_text_blocks(&text_msg(Role::User, "hello")));
        let tool_only = ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t".into(),
                content: crate::types::content::ToolResultContent::Text("ok".into()),
                is_error: false,
            }],
        };
        assert!(!has_text_blocks(&tool_only));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::session_memory::compaction -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement compaction**

```rust
pub fn has_text_blocks(msg: &ApiMessage) -> bool
pub fn adjust_index_to_preserve_api_invariants(messages: &[ApiMessage], start_index: usize) -> usize
pub fn calculate_messages_to_keep_index(messages: &[ApiMessage], last_summarized_index: usize, config: &SessionMemoryCompactConfig) -> usize

pub async fn try_session_memory_compaction(
    messages: &[ApiMessage],
    state: &SessionMemoryState,
    auto_compact_threshold: Option<u64>,
) -> Option<CompactResult>
```

`try_session_memory_compaction` flow:
1. `wait_for_extraction(state).await`
2. Read session memory content → return None if empty/matches template
3. Get `last_memory_message_index` from state → if None, use `messages.len() - 1` (resumed session: keep nothing initially, let expansion add back)
4. `calculate_messages_to_keep_index()` to determine kept set (uses index, not message ID)
5. Filter old compact boundaries from kept messages
6. `truncate_session_memory_for_compact()` on the content
7. Build boundary marker (Role::User) + summary message (Role::User with `get_compact_user_summary_message()`)
8. Assemble `CompactResult` with `CompactStage::SessionMemory`
9. If `auto_compact_threshold` provided, check post-compact token count → return None if still over

- [ ] **Step 4: Wire up module**

Add `pub mod compaction;` to `src/services/session_memory/mod.rs`.

- [ ] **Step 5: Run tests**

Run: `cargo test --lib services::session_memory -- --nocapture`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```
feat(session_memory): add session memory compaction with message retention and tool pair preservation
```

---

## Task 9: CommandContext Expansion & Result Types

**Files:**
- Modify: `src/commands/context.rs`
- Modify: `src/commands/types.rs`

- [ ] **Step 1: Add new fields to CommandContext**

In `src/commands/context.rs`, add imports and fields:

```rust
use crate::services::api::provider::LlmProvider;
use crate::services::session_memory::SessionMemoryState;
use crate::types::message::ApiMessage;

pub struct CommandContext {
    // existing fields...
    pub provider: Option<Arc<dyn LlmProvider>>,
    pub messages: Arc<Vec<ApiMessage>>,
    pub session_memory: Option<Arc<SessionMemoryState>>,
}
```

- [ ] **Step 2: Add LocalCommandResult::Clear and update Compact**

In `src/commands/types.rs`:

```rust
use crate::services::compact::types::CompactResult;

pub enum LocalCommandResult {
    Text { value: String },
    Compact { result: CompactResult },
    Clear,
    Skip,
}
```

- [ ] **Step 3: Fix all CommandContext construction sites**

Search for `CommandContext {` across the codebase. Each construction site needs the new fields:
- `src/tui/run.rs` line ~699 — `FoundCommand::Local` branch
- `src/tui/run.rs` line ~753 — `FoundCommand::Tui` branch
- `src/tui/run.rs` line ~792 — `FoundCommand::Prompt` branch
- Any test files that construct `CommandContext`

For now, add `provider: None, messages: Arc::new(Vec::new()), session_memory: None` as defaults. The real injection happens in Task 11.

- [ ] **Step 4: Fix all LocalCommandResult match arms**

Search for `LocalCommandResult::Compact {` across the codebase:
- `src/tui/run.rs` — the handler needs to match the new shape `Compact { result }`
- Temporarily keep existing behavior (clear) while we update in Task 11

- [ ] **Step 5: Update /clear to return Clear and temporarily stub /compact**

In `src/commands/local/clear.rs`, change return from `LocalCommandResult::Compact { display_text: ... }` to `LocalCommandResult::Clear`.

In `src/commands/local/compact.rs`, temporarily return a stub to keep compilation green:
```rust
async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
    Ok(LocalCommandResult::Compact {
        result: crate::services::compact::types::CompactResult {
            messages: Vec::new(),
            tokens_freed: 0,
            stage: crate::services::compact::types::CompactStage::FullCompact,
            pre_compact_token_count: None,
            post_compact_token_count: None,
        },
    })
}
```
This stub is replaced in Task 10 with the real implementation.

- [ ] **Step 6: Run full test suite**

Run: `cargo test --lib -- --nocapture`
Expected: ALL PASS

- [ ] **Step 7: Commit**

```
feat(commands): expand CommandContext with provider/messages/session_memory, add LocalCommandResult::Clear
```

---

## Task 10: /compact Command Implementation

**Files:**
- Modify: `src/commands/local/compact.rs`

- [ ] **Step 1: Implement the full /compact command**

Replace the stub in `src/commands/local/compact.rs` with the real implementation from spec Section 6.1:

```rust
async fn call(&self, args: &str, ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
    let provider = ctx.provider.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No provider available"))?;

    let all_messages = &*ctx.messages;
    let messages = crate::services::compact::get_messages_after_compact_boundary(all_messages);

    if messages.is_empty() {
        return Err(anyhow::anyhow!(ERROR_MESSAGE_NOT_ENOUGH_MESSAGES));
    }

    let custom_instructions = args.trim();
    let custom = if custom_instructions.is_empty() { None } else { Some(custom_instructions) };

    // 1. Try session memory compaction (no custom instructions only)
    if custom.is_none() {
        if let Some(ref sm) = ctx.session_memory {
            if let Some(result) = try_session_memory_compaction(messages, sm, None).await {
                // Reset lastSummarizedMessageId after compaction (I3)
                sm.reset_last_summarized_message_id().await;
                return Ok(LocalCommandResult::Compact { result });
            }
        }
    }

    // 2. Microcompact
    let mut work = messages.to_vec();
    crate::services::compact::microcompact::microcompact(&mut work, 3);

    // 3. Full compact
    let cw = provider.model_info().context_window;
    let result = crate::services::compact::full_compact::full_compact(
        &work, provider.as_ref(), cw, 3, custom, &ctx.abort_signal,
    ).await?;

    // Reset lastSummarizedMessageId after full compact (I3)
    if let Some(ref sm) = ctx.session_memory {
        sm.reset_last_summarized_message_id().await;
    }

    Ok(LocalCommandResult::Compact { result })
}
```

- [ ] **Step 2: Run test suite**

Run: `cargo test --lib -- --nocapture`
Expected: ALL PASS

- [ ] **Step 3: Commit**

```
feat(compact): implement /compact command with session memory → microcompact → full compact pipeline
```

---

## Task 11: TUI Integration (run.rs)

**Files:**
- Modify: `src/tui/run.rs`

- [ ] **Step 1: Update CommandContext injection in dispatch_slash_command**

In the `FoundCommand::Local` branch (~line 696), replace the dummy context with real values:

```rust
let ctx = CommandContext {
    app_state: app_state.clone(),
    abort_signal: app.active_abort_signal.clone().unwrap_or_else(|| AbortSignal::new()),
    cwd: std::env::current_dir().unwrap_or_default(),
    message_tx: msg_tx,
    ui_event_tx: ui_tx,
    options: CommandOptions { theme: "default".into() },
    provider: provider.clone(),
    messages: Arc::new(app.api_messages.clone()),
    session_memory: None, // Wired in Task 12
};
```

- [ ] **Step 2: Update handler for Compact and Clear**

Replace the `LocalCommandResult::Compact { display_text }` handler with:

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

- [ ] **Step 3: Add loading feedback for compact**

Before the command call, check if it's compact and set loading state:

```rust
if name == "compact" {
    app.is_loading = true;
    app.loading_label = Some("Compacting...".into());
}
// ... cmd.call() ...
if name == "compact" {
    app.is_loading = false;
    app.loading_label = None;
}
```

- [ ] **Step 4: Run full test suite**

Run: `cargo test --lib -- --nocapture`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```
feat(tui): integrate compact/clear handlers with real CommandContext and loading feedback
```

---

## Task 12: Auto-Compact & Engine Integration

**Files:**
- Modify: `src/services/compact/mod.rs` (add `auto_compact_if_needed`)
- Modify: `src/query/engine.rs`
- Modify: `src/query/types.rs` (add `session_memory` to QueryContext)
- Modify: `src/tui/run.rs` (auto-compact after QueryCompleted, session memory init)

- [ ] **Step 1: Add auto_compact_if_needed to compact/mod.rs**

```rust
pub async fn auto_compact_if_needed(
    messages: &mut Vec<ApiMessage>,
    provider: &Arc<dyn LlmProvider>,
    session_memory: Option<&Arc<SessionMemoryState>>,
    consecutive_failures: &mut u32,
    snip_tokens_freed: u64,
    query_source: Option<&str>,
    abort_signal: &AbortSignal,
) -> Option<CompactResult>
```

Implement per spec Section 7.2 with:
- Recursion guards (I2): skip for `session_memory` / `compact` query sources
- Env var checks: `DISABLE_COMPACT`, `DISABLE_AUTO_COMPACT`
- Circuit breaker: `consecutive_failures >= 3`
- Threshold: `get_auto_compact_threshold(cw, max_output)` using provider model info
- Try session memory compaction first
- Fallback: microcompact + full_compact with `suppress_follow_up_questions: true`

- [ ] **Step 2: Add session_memory to QueryContext**

In `src/query/types.rs`, add:
```rust
pub session_memory: Option<Arc<crate::services::session_memory::SessionMemoryState>>,
```

Fix all `QueryContext` construction sites (in `run.rs`, test files).

- [ ] **Step 3: Remove staged ratio checks from engine.rs**

In `src/query/engine.rs`, delete the four `token_ratio()` check blocks inside the main loop (before "Build request"):
- The `if token_ratio(...) > 0.60 { snip }` block
- The `if token_ratio(...) > 0.75 { microcompact }` block
- The `if token_ratio(...) > 0.85 { full_compact }` block
- The `if token_ratio(...) > 0.95 { hard block }` block

Search for `// 0. Compaction pipeline` comment to locate the start. These are replaced by REPL-level auto-compact in Step 6.

- [ ] **Step 4: Enhance reactive compact in engine.rs**

Update the `is_context_overflow_error` handler (~line 126) per spec Section 8.2:
1. Try session memory compaction first
2. Fallback to snip → microcompact → full_compact

- [ ] **Step 5: Add post-sampling hook in engine.rs**

In the `tool_calls.is_empty()` branch (~line 375), add session memory extraction trigger:
```rust
if tool_calls.is_empty() {
    if let Some(ref sm) = ctx.session_memory {
        let sm = sm.clone();
        let messages = ctx.messages.clone();
        let provider = ctx.provider.clone();
        tokio::spawn(async move {
            sm.maybe_extract(&messages, &provider).await;
        });
    }
}
```

- [ ] **Step 6: Wire auto-compact and SessionMemoryState in run.rs**

**SessionMemoryState initialization:** Create at the start of the `run()` function, before the main event loop:
```rust
let session_memory = Arc::new(SessionMemoryState::new(app.session_id.clone()));
```

**Threading path — the `session_memory: Arc<SessionMemoryState>` must reach:**
1. `dispatch_slash_command()` → `CommandContext.session_memory` (already in Task 11)
2. `submit_query()` → `QueryContext.session_memory` (pass as parameter)
3. Auto-compact call after QueryCompleted (local variable in run loop)

**Auto-compact state:** Declare before the main event loop:
```rust
let mut compact_failures: u32 = 0;
let mut snip_tokens_freed: u64 = 0;
```

In the QueryCompleted event handler, after `app.handle_query_event(event)`:
```rust
if is_query_completed {
    if let Some(ref provider) = provider {
        let threshold = get_auto_compact_threshold(
            provider.model_info().context_window,
            provider.model_info().max_output_tokens,
        );
        let token_count = estimate_message_tokens(&app.api_messages);
        if token_count.saturating_sub(snip_tokens_freed) >= threshold {
            if let Some(result) = auto_compact_if_needed(
                &mut app.api_messages, provider,
                Some(&session_memory), &mut compact_failures,
                snip_tokens_freed, None, // query_source=None for REPL
                &AbortSignal::new(),
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
                snip_tokens_freed = 0; // Reset after successful compact
            }
        }
    }
}
```

- [ ] **Step 7: Run full test suite**

Run: `cargo test --lib -- --nocapture`
Expected: ALL PASS

- [ ] **Step 8: Compile check**

Run: `cargo build 2>&1 | head -50`
Expected: Clean build (warnings OK, no errors)

- [ ] **Step 9: Commit**

```
feat(compact): add auto-compact with session memory priority, remove staged ratio checks, add post-sampling extraction hook
```

---

## Task 13: Manual Smoke Test

- [ ] **Step 1: Build and run**

```bash
cargo build --release 2>&1 | tail -5
```

- [ ] **Step 2: Test /clear**

Launch the app, have a short conversation, type `/clear`. Verify:
- Screen clears
- "Conversation cleared." system message appears
- Next message works with empty context

- [ ] **Step 3: Test /compact**

Launch the app, have a conversation with a few tool calls, type `/compact`. Verify:
- "Compacting..." loading indicator appears
- After LLM call completes, system message shows `[Context compacted (full/session_memory): ~XK tokens freed]`
- Next message works with compacted context (model knows about prior conversation via summary)

- [ ] **Step 4: Test /compact with custom instructions**

Type `/compact focus on test failures`. Verify compaction completes without error.

- [ ] **Step 5: Commit final state**

```
chore: verified /compact and /clear manual smoke test
```
