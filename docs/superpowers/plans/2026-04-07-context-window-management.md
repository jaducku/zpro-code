# Context Window Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Multi-layer compaction pipeline (snip, microcompact, full compact, reactive compact) to manage context window overflow.

**Architecture:** Four new files under `src/services/compact/` (types, snip, microcompact, full_compact) replace the existing simple compaction. Pipeline integrates into `src/query/engine.rs` with progressive threshold checks (60%/75%/85%/95%). Reactive compact catches API overflow errors and retries.

**Tech Stack:** Existing crates only — tokio async, serde, futures (for stream collection in full_compact)

**Spec:** `docs/superpowers/specs/2026-04-07-context-window-management-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/services/compact/types.rs` | Create | CompactResult, CompactStage, COMPACT_BOUNDARY_PREFIX |
| `src/services/compact/snip.rs` | Create | Turn-aware message removal with boundary marker |
| `src/services/compact/microcompact.rs` | Create | In-place tool result truncation |
| `src/services/compact/full_compact.rs` | Create | LLM-based conversation summarization |
| `src/services/compact/mod.rs` | Modify | Replace old code, re-export new modules, add `token_ratio` helper |
| `src/query/engine.rs` | Modify | Replace compaction block with pipeline, add reactive compact |
| `src/query/types.rs` | Modify | Add `Compacted` variant to `QueryEvent` |
| `src/tui/app.rs` | Modify | Handle `QueryEvent::Compacted` in `handle_query_event` |

---

## Task 1: Types Module

**Files:**
- Create: `src/services/compact/types.rs`
- Modify: `src/services/compact/mod.rs`

- [ ] **Step 1: Write tests for CompactStage Display**

```rust
// src/services/compact/types.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_stage_display() {
        assert_eq!(CompactStage::Snip.to_string(), "snip");
        assert_eq!(CompactStage::Microcompact.to_string(), "microcompact");
        assert_eq!(CompactStage::FullCompact.to_string(), "full");
        assert_eq!(CompactStage::Reactive.to_string(), "reactive");
    }

    #[test]
    fn test_boundary_prefix() {
        assert!(COMPACT_BOUNDARY_PREFIX.starts_with("[Context"));
    }
}
```

- [ ] **Step 2: Implement types.rs**

```rust
// src/services/compact/types.rs
use crate::types::message::ApiMessage;

/// Result of any compaction operation.
pub struct CompactResult {
    pub messages: Vec<ApiMessage>,
    pub tokens_freed: u64,
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

/// Marker content prefix for boundary messages.
pub const COMPACT_BOUNDARY_PREFIX: &str = "[Context compacted";
```

- [ ] **Step 3: Update mod.rs — replace old code, add module declarations**

Replace the entire content of `src/services/compact/mod.rs` with:

```rust
pub mod types;
pub mod snip;
pub mod microcompact;
pub mod full_compact;

pub use types::*;

use crate::types::message::ApiMessage;
use crate::services::token_estimation::estimate_message_tokens;

/// Compute the token ratio (estimated_tokens / context_window).
pub fn token_ratio(messages: &[ApiMessage], context_window: u64) -> f64 {
    if context_window == 0 {
        return 0.0;
    }
    estimate_message_tokens(messages) as f64 / context_window as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::message::Role;
    use crate::types::content::ContentBlock;

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage {
            role,
            content: vec![ContentBlock::Text { text: text.to_string() }],
        }
    }

    #[test]
    fn test_token_ratio_empty() {
        assert_eq!(token_ratio(&[], 128_000), 0.0);
    }

    #[test]
    fn test_token_ratio_zero_window() {
        let msgs = vec![text_msg(Role::User, "hello")];
        assert_eq!(token_ratio(&msgs, 0), 0.0);
    }

    #[test]
    fn test_token_ratio_basic() {
        // "hello" = 5 chars ≈ 2 tokens + 4 overhead = 6 tokens
        let msgs = vec![text_msg(Role::User, "hello")];
        let r = token_ratio(&msgs, 100);
        assert!(r > 0.0 && r < 0.1);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib services::compact -- --nocapture`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add src/services/compact/types.rs src/services/compact/mod.rs
git commit -m "feat(compact): add CompactResult, CompactStage types and token_ratio helper"
```

---

## Task 2: Snip

**Files:**
- Create: `src/services/compact/snip.rs`

- [ ] **Step 1: Write tests**

```rust
// src/services/compact/snip.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::message::{ApiMessage, Role};
    use crate::types::content::ContentBlock;

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage {
            role,
            content: vec![ContentBlock::Text { text: text.to_string() }],
        }
    }

    fn tool_use_msg(id: &str, name: &str) -> ApiMessage {
        ApiMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({}),
            }],
        }
    }

    fn tool_result_msg(id: &str, text: &str) -> ApiMessage {
        ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: crate::types::content::ToolResultContent::Text(text.to_string()),
                is_error: false,
            }],
        }
    }

    #[test]
    fn test_snip_no_op_few_messages() {
        // 2 messages, keep 3 turns — nothing to snip
        let mut msgs = vec![
            text_msg(Role::User, "hello"),
            text_msg(Role::Assistant, "hi"),
        ];
        let freed = snip(&mut msgs, 3);
        assert_eq!(freed, 0);
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn test_snip_removes_old_messages() {
        let mut msgs = vec![
            text_msg(Role::User, "old question 1"),
            text_msg(Role::Assistant, "old answer 1"),
            text_msg(Role::User, "old question 2"),
            text_msg(Role::Assistant, "old answer 2"),
            text_msg(Role::User, "recent question"),
            text_msg(Role::Assistant, "recent answer"),
        ];
        let freed = snip(&mut msgs, 1); // keep 1 turn
        assert!(freed > 0);
        // Should have: boundary + last 2 messages (1 turn)
        assert!(msgs.len() <= 3);
        // First message should be boundary
        assert!(msgs[0].content.iter().any(|c| {
            matches!(c, ContentBlock::Text { text } if text.starts_with(super::super::types::COMPACT_BOUNDARY_PREFIX))
        }));
    }

    #[test]
    fn test_snip_preserves_tool_pairs() {
        let mut msgs = vec![
            text_msg(Role::User, "old"),
            text_msg(Role::Assistant, "old reply"),
            text_msg(Role::User, "do something"),
            tool_use_msg("t1", "bash"),
            tool_result_msg("t1", "output"),
            text_msg(Role::Assistant, "done"),
            text_msg(Role::User, "recent"),
            text_msg(Role::Assistant, "recent reply"),
        ];
        let freed = snip(&mut msgs, 2); // keep 2 turns
        assert!(freed > 0);
        // The tool_use and tool_result should both be present or both absent
        let has_tool_use = msgs.iter().any(|m| m.content.iter().any(|c| matches!(c, ContentBlock::ToolUse { .. })));
        let has_tool_result = msgs.iter().any(|m| m.content.iter().any(|c| matches!(c, ContentBlock::ToolResult { .. })));
        assert_eq!(has_tool_use, has_tool_result);
    }

    #[test]
    fn test_snip_empty_messages() {
        let mut msgs = vec![];
        let freed = snip(&mut msgs, 3);
        assert_eq!(freed, 0);
    }
}
```

- [ ] **Step 2: Implement snip**

```rust
// src/services/compact/snip.rs
use crate::services::token_estimation::estimate_message_tokens;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};

use super::types::COMPACT_BOUNDARY_PREFIX;

/// Remove old messages, keeping the last `keep_turns` complete turns.
/// A turn starts with a Role::User message and includes all subsequent
/// assistant/tool messages until the next user message.
/// Returns estimated tokens freed.
pub fn snip(messages: &mut Vec<ApiMessage>, keep_turns: usize) -> u64 {
    if messages.is_empty() || keep_turns == 0 {
        return 0;
    }

    let tokens_before = estimate_message_tokens(messages);

    // Find turn boundaries (indices where Role::User messages start)
    let turn_starts: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == Role::User)
        .map(|(i, _)| i)
        .collect();

    // Need more turns than keep_turns to snip anything
    if turn_starts.len() <= keep_turns {
        return 0;
    }

    // Keep from this index onward
    let keep_from = turn_starts[turn_starts.len() - keep_turns];

    // Nothing to remove
    if keep_from == 0 {
        return 0;
    }

    let removed_count = keep_from;
    let kept = messages.split_off(keep_from);
    let tokens_removed = estimate_message_tokens(messages);

    // Build boundary message
    let boundary = ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: format!(
                "{}: {} earlier messages removed. ~{}K tokens freed]",
                COMPACT_BOUNDARY_PREFIX,
                removed_count,
                tokens_removed / 1000,
            ),
        }],
    };

    *messages = vec![boundary];
    messages.extend(kept);

    let tokens_after = estimate_message_tokens(messages);
    tokens_before.saturating_sub(tokens_after)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib services::compact::snip -- --nocapture`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/services/compact/snip.rs
git commit -m "feat(compact): add snip — turn-aware message removal"
```

---

## Task 3: Microcompact

**Files:**
- Create: `src/services/compact/microcompact.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::message::{ApiMessage, Role};
    use crate::types::content::{ContentBlock, ToolResultContent};

    fn tool_result_msg(id: &str, text: &str) -> ApiMessage {
        ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: ToolResultContent::Text(text.to_string()),
                is_error: false,
            }],
        }
    }

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage {
            role,
            content: vec![ContentBlock::Text { text: text.to_string() }],
        }
    }

    #[test]
    fn test_microcompact_truncates_long_tool_result() {
        let long_text = "x".repeat(5000);
        let mut msgs = vec![
            tool_result_msg("t1", &long_text),
            text_msg(Role::User, "recent"),
            text_msg(Role::Assistant, "reply"),
        ];
        let freed = microcompact(&mut msgs, 0); // preserve none
        assert!(freed > 0);
        // First message should be truncated
        if let ContentBlock::ToolResult { content: ToolResultContent::Text(text), .. } = &msgs[0].content[0] {
            assert!(text.len() < 5000);
            assert!(text.contains("truncated"));
        } else {
            panic!("Expected truncated ToolResult");
        }
    }

    #[test]
    fn test_microcompact_preserves_recent() {
        let long_text = "x".repeat(5000);
        let mut msgs = vec![
            tool_result_msg("t1", &long_text),
        ];
        let freed = microcompact(&mut msgs, 1); // preserve last 1
        assert_eq!(freed, 0); // should not touch it
        if let ContentBlock::ToolResult { content: ToolResultContent::Text(text), .. } = &msgs[0].content[0] {
            assert_eq!(text.len(), 5000);
        }
    }

    #[test]
    fn test_microcompact_short_text_unchanged() {
        let mut msgs = vec![
            tool_result_msg("t1", "short result"),
        ];
        let freed = microcompact(&mut msgs, 0);
        assert_eq!(freed, 0);
    }

    #[test]
    fn test_microcompact_blocks_variant() {
        let long_text = "y".repeat(5000);
        let mut msgs = vec![
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: ToolResultContent::Blocks(vec![
                        ContentBlock::Text { text: long_text },
                    ]),
                    is_error: false,
                }],
            },
        ];
        let freed = microcompact(&mut msgs, 0);
        assert!(freed > 0);
    }

    #[test]
    fn test_microcompact_empty() {
        let mut msgs = vec![];
        let freed = microcompact(&mut msgs, 0);
        assert_eq!(freed, 0);
    }
}
```

- [ ] **Step 2: Implement microcompact**

```rust
// src/services/compact/microcompact.rs
use crate::services::token_estimation::estimate_tokens;
use crate::types::content::{ContentBlock, ToolResultContent};
use crate::types::message::ApiMessage;

const MAX_CHARS: usize = 2000;
const KEEP_HEAD: usize = 500;
const KEEP_TAIL: usize = 500;

/// Truncate long tool results in-place, preserving the last `preserve_last`
/// tool_result messages. Returns estimated tokens freed.
pub fn microcompact(messages: &mut Vec<ApiMessage>, preserve_last: usize) -> u64 {
    // Count tool_result messages from the end to determine which to preserve
    let mut tool_result_count = 0;
    let mut preserve_indices = std::collections::HashSet::new();

    for (i, msg) in messages.iter().enumerate().rev() {
        let has_tool_result = msg.content.iter().any(|c| matches!(c, ContentBlock::ToolResult { .. }));
        if has_tool_result {
            if tool_result_count < preserve_last {
                preserve_indices.insert(i);
            }
            tool_result_count += 1;
        }
    }

    let mut total_freed: u64 = 0;

    for (i, msg) in messages.iter_mut().enumerate() {
        if preserve_indices.contains(&i) {
            continue;
        }

        for block in &mut msg.content {
            if let ContentBlock::ToolResult { content, .. } = block {
                total_freed += truncate_tool_content(content);
            }
        }
    }

    // Remove image blocks from old messages
    total_freed += remove_old_images(messages, &preserve_indices);

    total_freed
}

fn truncate_tool_content(content: &mut ToolResultContent) -> u64 {
    match content {
        ToolResultContent::Text(text) => truncate_text(text),
        ToolResultContent::Blocks(blocks) => {
            let mut freed = 0u64;
            for block in blocks.iter_mut() {
                if let ContentBlock::Text { text } = block {
                    freed += truncate_text(text);
                }
            }
            freed
        }
    }
}

fn truncate_text(text: &mut String) -> u64 {
    if text.len() <= MAX_CHARS {
        return 0;
    }

    let original_tokens = estimate_tokens(text);
    let removed = text.len() - KEEP_HEAD - KEEP_TAIL;
    let head = text[..KEEP_HEAD].to_string();
    let tail = text[text.len() - KEEP_TAIL..].to_string();
    *text = format!("{}\n...[truncated {} chars]...\n{}", head, removed, tail);
    let new_tokens = estimate_tokens(text);
    original_tokens.saturating_sub(new_tokens)
}

/// Remove image blocks from non-preserved messages.
fn remove_old_images(messages: &mut [ApiMessage], preserve_indices: &std::collections::HashSet<usize>) -> u64 {
    let mut freed = 0u64;
    for (i, msg) in messages.iter_mut().enumerate() {
        if preserve_indices.contains(&i) {
            continue;
        }
        let before = msg.content.len();
        msg.content.retain(|c| !matches!(c, ContentBlock::Image { .. }));
        let removed = before - msg.content.len();
        freed += removed as u64 * 1000; // ~1000 tokens per image (estimate)
    }
    freed
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib services::compact::microcompact -- --nocapture`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/services/compact/microcompact.rs
git commit -m "feat(compact): add microcompact — in-place tool result truncation"
```

---

## Task 4: Full Compact

**Files:**
- Create: `src/services/compact/full_compact.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::message::{ApiMessage, Role};
    use crate::types::content::ContentBlock;

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage {
            role,
            content: vec![ContentBlock::Text { text: text.to_string() }],
        }
    }

    #[test]
    fn test_find_turn_boundary() {
        let msgs = vec![
            text_msg(Role::User, "old"),
            text_msg(Role::Assistant, "old reply"),
            text_msg(Role::User, "recent"),
            text_msg(Role::Assistant, "recent reply"),
        ];
        let boundary = find_keep_boundary(&msgs, 1);
        assert_eq!(boundary, 2); // keep from index 2 (last 1 turn)
    }

    #[test]
    fn test_find_turn_boundary_all_kept() {
        let msgs = vec![
            text_msg(Role::User, "q"),
            text_msg(Role::Assistant, "a"),
        ];
        let boundary = find_keep_boundary(&msgs, 3);
        assert_eq!(boundary, 0); // only 1 turn, keep all
    }

    #[test]
    fn test_build_summary_request() {
        let msgs = vec![
            text_msg(Role::User, "hello"),
            text_msg(Role::Assistant, "world"),
        ];
        let req = build_summary_request(&msgs, "test-model");
        assert!(req.system.is_some());
        assert!(!req.messages.is_empty());
        assert_eq!(req.max_tokens, 4096);
        assert!(req.tools.is_none());
    }
}
```

- [ ] **Step 2: Implement full_compact**

```rust
// src/services/compact/full_compact.rs
use futures::StreamExt;

use crate::services::api::provider::LlmProvider;
use crate::services::api::types::*;
use crate::services::token_estimation::estimate_message_tokens;
use crate::types::content::ContentBlock;
use crate::types::error::AtomError;
use crate::types::message::{ApiMessage, Role};

use super::types::{CompactResult, CompactStage, COMPACT_BOUNDARY_PREFIX};

const SUMMARY_MAX_TOKENS: u32 = 4096;

const SUMMARY_SYSTEM_PROMPT: &str = "\
You are summarizing a conversation between a user and an AI coding assistant.\n\
Preserve all important context:\n\
- Decisions made and their reasoning\n\
- Files created, modified, or deleted\n\
- Current goals and next steps\n\
- Any constraints or requirements mentioned\n\
- Error states or blockers encountered\n\n\
Be concise but complete. Output a structured summary.";

/// Run LLM-based summarization. Keeps the last `keep_turns` complete turns,
/// summarizes everything before that.
pub async fn full_compact(
    messages: &[ApiMessage],
    provider: &dyn LlmProvider,
    context_window: u64,
    keep_turns: usize,
) -> Result<CompactResult, AtomError> {
    let keep_from = find_keep_boundary(messages, keep_turns);

    if keep_from == 0 {
        return Err(AtomError::Generic("Nothing to compact — all messages are within keep range".into()));
    }

    let to_summarize = &messages[..keep_from];
    let to_keep = &messages[keep_from..];

    let tokens_before = estimate_message_tokens(messages);

    // Call LLM for summary
    let model = provider.model_info().id.clone();
    let request = build_summary_request(to_summarize, &model);

    let mut stream = provider.create_message(request).await?;
    let mut summary_text = String::new();

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::ContentBlockDelta { delta: ContentDelta::TextDelta { text }, .. } => {
                summary_text.push_str(&text);
            }
            _ => {}
        }
    }

    if summary_text.is_empty() {
        return Err(AtomError::Generic("LLM returned empty summary".into()));
    }

    // Build result messages
    let boundary = ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: format!(
                "{}: {} messages summarized by LLM]",
                COMPACT_BOUNDARY_PREFIX,
                to_summarize.len(),
            ),
        }],
    };

    let summary = ApiMessage {
        role: Role::Assistant,
        content: vec![ContentBlock::Text { text: summary_text }],
    };

    let mut result_messages = vec![boundary, summary];
    result_messages.extend_from_slice(to_keep);

    let tokens_after = estimate_message_tokens(&result_messages);

    Ok(CompactResult {
        messages: result_messages,
        tokens_freed: tokens_before.saturating_sub(tokens_after),
        stage: CompactStage::FullCompact,
    })
}

/// Find the message index where kept messages start (turn-boundary aligned).
pub fn find_keep_boundary(messages: &[ApiMessage], keep_turns: usize) -> usize {
    let turn_starts: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == Role::User)
        .map(|(i, _)| i)
        .collect();

    if turn_starts.len() <= keep_turns {
        return 0; // keep everything
    }

    turn_starts[turn_starts.len() - keep_turns]
}

/// Build a CreateMessageRequest for the summary LLM call.
pub fn build_summary_request(to_summarize: &[ApiMessage], model: &str) -> CreateMessageRequest {
    // Serialize the messages into a single user message for the summary call
    let conversation_text: String = to_summarize
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
                _ => "System",
            };
            let content: String = m.content.iter().map(|c| match c {
                ContentBlock::Text { text } => text.clone(),
                ContentBlock::ToolUse { name, .. } => format!("[Tool call: {}]", name),
                ContentBlock::ToolResult { content, .. } => {
                    match content {
                        crate::types::content::ToolResultContent::Text(t) => {
                            if t.len() > 200 { format!("{}...", &t[..200]) } else { t.clone() }
                        }
                        crate::types::content::ToolResultContent::Blocks(_) => "[tool result blocks]".into(),
                    }
                }
                _ => String::new(),
            }).collect::<Vec<_>>().join("\n");
            format!("{}: {}", role, content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    CreateMessageRequest {
        model: model.to_string(),
        messages: vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: format!("Summarize this conversation:\n\n{}", conversation_text),
            }],
        }],
        tools: None,
        system: Some(vec![SystemBlock::Text {
            text: SUMMARY_SYSTEM_PROMPT.to_string(),
        }]),
        max_tokens: SUMMARY_MAX_TOKENS,
        temperature: None,
        stream: true,
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib services::compact::full_compact -- --nocapture`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/services/compact/full_compact.rs
git commit -m "feat(compact): add full_compact — LLM-based conversation summarization"
```

---

## Task 5: QueryEvent::Compacted + TUI Handler

**Files:**
- Modify: `src/query/types.rs`
- Modify: `src/tui/app.rs`

- [ ] **Step 1: Add Compacted variant to QueryEvent**

In `src/query/types.rs`, add to the `QueryEvent` enum (before `QueryCancelled`):

```rust
/// Context was compacted to fit token budget
Compacted {
    stage: String,
    tokens_freed: u64,
},
```

- [ ] **Step 2: Handle in app.rs**

In `src/tui/app.rs`, in `handle_query_event()`, add a match arm for `Compacted`:

```rust
QueryEvent::Compacted { stage, tokens_freed } => {
    self.messages.push(DisplayMessage::Text {
        role: TextRole::System,
        content: format!("[Context compacted ({}): ~{}K tokens freed]",
            stage, tokens_freed / 1000),
    });
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib`
Expected: All pass (the new variant must be handled in all exhaustive matches)

- [ ] **Step 4: Commit**

```bash
git add src/query/types.rs src/tui/app.rs
git commit -m "feat(compact): add QueryEvent::Compacted and TUI handler"
```

---

## Task 6: Query Engine Integration

**Files:**
- Modify: `src/query/engine.rs`

- [ ] **Step 1: Add compact_failures and already_retried variables**

At the top of the `query()` function, after `let mut total_cost: f64 = 0.0;`, add:

```rust
let mut compact_failures: u32 = 0;
```

Inside the loop, at the start (after `loop {` and before the budget check), add:

```rust
let mut already_retried = false;
```

- [ ] **Step 2: Replace the existing compaction block (lines 49-73)**

Replace the block starting with `// 0. Check token budget — auto-compact if needed` through the closing `}` with the full pipeline:

```rust
// 0. Compaction pipeline
let cw = ctx.provider.model_info().context_window;

// Stage 1: Snip
if crate::services::compact::token_ratio(&ctx.messages, cw) > 0.60 {
    let freed = crate::services::compact::snip::snip(&mut ctx.messages, 3);
    if freed > 0 {
        let _ = event_tx.send(QueryEvent::Compacted {
            stage: "snip".into(),
            tokens_freed: freed,
        });
    }
}

// Stage 2: Microcompact
if crate::services::compact::token_ratio(&ctx.messages, cw) > 0.75 {
    let freed = crate::services::compact::microcompact::microcompact(&mut ctx.messages, 3);
    if freed > 0 {
        let _ = event_tx.send(QueryEvent::Compacted {
            stage: "microcompact".into(),
            tokens_freed: freed,
        });
    }
}

// Stage 3: Full Compact (LLM call)
if crate::services::compact::token_ratio(&ctx.messages, cw) > 0.85 && compact_failures < 3 {
    match crate::services::compact::full_compact::full_compact(
        &ctx.messages, ctx.provider.as_ref(), cw, 3,
    ).await {
        Ok(result) => {
            let freed = result.tokens_freed;
            ctx.messages = result.messages;
            compact_failures = 0;
            let _ = event_tx.send(QueryEvent::Compacted {
                stage: "full".into(),
                tokens_freed: freed,
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
if crate::services::compact::token_ratio(&ctx.messages, cw) > 0.95 {
    let _ = event_tx.send(QueryEvent::Error(
        "Context window 95% exceeded after all compaction. Cannot continue.".into(),
    ));
    break;
}
```

- [ ] **Step 3: Add reactive compact error handler**

**Important:** The current `engine.rs` uses `ctx.provider.create_message(request).await?` (line ~92) which propagates errors immediately via `?`. This must be changed to a `match` block to catch context overflow errors for reactive compact:

```rust
// Change from:
//   let mut stream = ctx.provider.create_message(request).await?;
// To:
let mut stream = match ctx.provider.create_message(request).await {
    Ok(s) => s,
    Err(e) if is_context_overflow_error(&e) && !already_retried => {
        // reactive compact (see below)
        // ...
        already_retried = true;
        continue;
    }
    Err(e) => return Err(e),
};
```

Add the error detection function:

```rust
fn is_context_overflow_error(error: &crate::types::error::AtomError) -> bool {
    let msg = error.to_string().to_lowercase();
    msg.contains("prompt_too_long")
        || msg.contains("context_length")
        || msg.contains("max_context")
        || msg.contains("too many tokens")
        || msg.contains("context_window_exceeded")
}
```

In the streaming error handler, before the generic error path:

```rust
if is_context_overflow_error(&e) && !already_retried {
    let cw = ctx.provider.model_info().context_window;
    let _ = crate::services::compact::snip::snip(&mut ctx.messages, 3);
    let _ = crate::services::compact::microcompact::microcompact(&mut ctx.messages, 3);
    if let Ok(result) = crate::services::compact::full_compact::full_compact(
        &ctx.messages, ctx.provider.as_ref(), cw, 3,
    ).await {
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

- [ ] **Step 4: Run full tests**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add src/query/engine.rs
git commit -m "feat(compact): integrate multi-layer compaction pipeline into query engine"
```

---

## Task 7: Final Integration Test & Cleanup

**Files:**
- All modified files

- [ ] **Step 1: Run full test suite**

Run: `cargo test --lib`
Expected: All tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | grep "src/services/compact\|src/query/engine" | head -20`
Expected: No warnings from compact or engine files

- [ ] **Step 3: Verify release build**

Run: `cargo build --release 2>&1 | tail -3`
Expected: Clean build

- [ ] **Step 4: Commit cleanup if needed**

```bash
git add -A
git commit -m "chore: clippy fixes for context window management"
```
