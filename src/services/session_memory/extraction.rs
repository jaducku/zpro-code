use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;

use crate::services::api::provider::LlmProvider;
use crate::services::api::types::{ContentDelta, CreateMessageRequest, StreamEvent, SystemBlock};
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};
use crate::types::AtomError;

use super::types::SessionMemoryState;

/// Decide whether to trigger a memory extraction pass.
pub fn should_extract_memory(
    state: &SessionMemoryState,
    current_token_count: u64,
    tool_calls_since_last: usize,
    no_tool_calls_in_last_turn: bool,
) -> bool {
    let cfg = &state.config;

    // Initialization gate
    if !state.initialized.load(Ordering::SeqCst) {
        if current_token_count >= cfg.minimum_message_tokens_to_init {
            state.initialized.store(true, Ordering::SeqCst);
        } else {
            return false;
        }
    }

    let last = state.tokens_at_last_extraction.load(Ordering::SeqCst);
    let growth = current_token_count.saturating_sub(last);
    let token_growth_met = growth >= cfg.minimum_tokens_between_update;
    let tool_calls_met = tool_calls_since_last >= cfg.tool_calls_between_updates;

    if token_growth_met && tool_calls_met {
        return true;
    }
    if token_growth_met && no_tool_calls_in_last_turn {
        return true;
    }
    false
}

/// Count ToolUse blocks in messages after `state.last_memory_message_index`.
fn count_tool_calls_since_last(messages: &[ApiMessage], state: &SessionMemoryState) -> usize {
    // last_memory_message_index is an async RwLock; use try_read to avoid blocking in sync context.
    // If we can't acquire the lock we conservatively return 0.
    let start_index = state
        .last_memory_message_index
        .try_read()
        .ok()
        .and_then(|g| *g)
        .map(|i| i + 1)
        .unwrap_or(0);

    messages[start_index.min(messages.len())..]
        .iter()
        .flat_map(|m| m.content.iter())
        .filter(|c| matches!(c, ContentBlock::ToolUse { .. }))
        .count()
}

/// Return true if the last assistant message in `messages` contains a ToolUse block.
fn has_tool_calls_in_last_turn(messages: &[ApiMessage]) -> bool {
    messages
        .iter()
        .rev()
        .find(|m| m.role == Role::Assistant)
        .map(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolUse { .. }))
        })
        .unwrap_or(false)
}

/// Run a memory extraction if thresholds are met.  Fire-and-forget errors are swallowed.
pub async fn extract_session_memory(
    state: &SessionMemoryState,
    messages: &[ApiMessage],
    provider: &Arc<dyn LlmProvider>,
) -> Result<(), AtomError> {
    // Only one extraction at a time; skip if another is running.
    let _guard = match state.extraction_lock.try_lock() {
        Ok(g) => g,
        Err(_) => return Ok(()),
    };

    // Record start time.
    *state.extraction_started_at.write().await = Some(Instant::now());

    // Ensure the session memory file exists with the template.
    let dir = super::utils::get_session_memory_dir();
    tokio::fs::create_dir_all(&dir).await.ok();
    let path = super::utils::get_session_memory_path(&state.session_id);
    if !path.exists() {
        tokio::fs::write(&path, super::utils::DEFAULT_SESSION_MEMORY_TEMPLATE).await?;
    }

    // Read current content.
    let content = tokio::fs::read_to_string(&path).await?;
    let path_str = path.to_string_lossy().into_owned();
    let update_prompt = super::prompts::build_update_prompt(&content, &path_str);

    // Build a single-turn LLM request and stream the response.
    let request = CreateMessageRequest {
        model: provider.model_info().id.clone(),
        messages: vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: update_prompt,
            }],
        }],
        tools: None,
        system: Some(vec![SystemBlock::Text {
            text: "You are updating session notes. Output the updated notes content.".into(),
        }]),
        max_tokens: 4096,
        temperature: None,
        stream: true,
    };

    let mut stream = provider.create_message(request).await?;
    let mut response_text = String::new();
    while let Some(event) = stream.next().await {
        if let StreamEvent::ContentBlockDelta {
            delta: ContentDelta::TextDelta { text },
            ..
        } = event?
        {
            response_text.push_str(&text);
        }
    }

    // Write updated content if we got a non-empty response.
    if !response_text.is_empty() {
        tokio::fs::write(&path, &response_text).await?;
    }

    // Record extraction token snapshot.
    let current_tokens = crate::services::token_estimation::estimate_message_tokens(messages);
    state
        .tokens_at_last_extraction
        .store(current_tokens, Ordering::SeqCst);

    // Update last_memory_message_index to the current end of messages.
    if !messages.is_empty() {
        *state.last_memory_message_index.write().await = Some(messages.len() - 1);
    }

    // Clear extraction timestamp.
    *state.extraction_started_at.write().await = None;

    Ok(())
}

impl SessionMemoryState {
    /// Check thresholds and, if met, run a background extraction.
    pub async fn maybe_extract(&self, messages: &[ApiMessage], provider: &Arc<dyn LlmProvider>) {
        let token_count = crate::services::token_estimation::estimate_message_tokens(messages);
        let tool_calls = count_tool_calls_since_last(messages, self);
        let no_tools_last = !has_tool_calls_in_last_turn(messages);
        if should_extract_memory(self, token_count, tool_calls, no_tools_last) {
            let _ = extract_session_memory(self, messages, provider).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_should_extract_below_init_threshold() {
        let state = SessionMemoryState::new("test".into());
        assert!(!should_extract_memory(&state, 5_000, 5, false));
    }

    #[test]
    fn test_should_extract_initializes_at_threshold() {
        let state = SessionMemoryState::new("test".into());
        assert!(!state.initialized.load(Ordering::SeqCst));
        // At exactly the threshold, should initialize but not extract yet
        // (no tool calls met)
        let result = should_extract_memory(&state, 10_000, 0, false);
        assert!(state.initialized.load(Ordering::SeqCst));
        // Growth = 10000 - 0 = 10000 >= 5000 ✓, but tool_calls = 0 < 3 ✗
        // no_tool_calls_in_last_turn = false, so natural break not triggered
        assert!(!result);
    }

    #[test]
    fn test_should_extract_both_thresholds() {
        let state = SessionMemoryState::new("test".into());
        state.initialized.store(true, Ordering::SeqCst);
        state
            .tokens_at_last_extraction
            .store(10_000, Ordering::SeqCst);
        // Growth = 15000 - 10000 = 5000 >= 5000 ✓, tool_calls = 3 >= 3 ✓
        assert!(should_extract_memory(&state, 15_000, 3, false));
    }

    #[test]
    fn test_should_extract_natural_break() {
        let state = SessionMemoryState::new("test".into());
        state.initialized.store(true, Ordering::SeqCst);
        state
            .tokens_at_last_extraction
            .store(10_000, Ordering::SeqCst);
        // Growth met, tool calls NOT met, but natural break
        assert!(should_extract_memory(&state, 15_000, 1, true));
    }

    #[test]
    fn test_should_extract_insufficient_growth() {
        let state = SessionMemoryState::new("test".into());
        state.initialized.store(true, Ordering::SeqCst);
        state
            .tokens_at_last_extraction
            .store(10_000, Ordering::SeqCst);
        // Growth = 13000 - 10000 = 3000 < 5000
        assert!(!should_extract_memory(&state, 13_000, 5, true));
    }

    #[test]
    fn test_has_tool_calls_in_last_turn() {
        let msgs_with_tools = vec![ApiMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "t1".into(),
                name: "Read".into(),
                input: serde_json::json!({}),
            }],
        }];
        assert!(has_tool_calls_in_last_turn(&msgs_with_tools));

        let msgs_no_tools = vec![ApiMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::Text {
                text: "done".into(),
            }],
        }];
        assert!(!has_tool_calls_in_last_turn(&msgs_no_tools));
    }
}
