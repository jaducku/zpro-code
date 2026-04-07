use crate::services::compact::prompt::get_compact_user_summary_message;
use crate::services::compact::types::{CompactResult, CompactStage, COMPACT_BOUNDARY_PREFIX};
use crate::services::token_estimation::estimate_message_tokens;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};

use super::types::{SessionMemoryCompactConfig, SessionMemoryState};
use super::utils::{
    get_session_memory_path, is_session_memory_empty, wait_for_extraction,
    DEFAULT_SESSION_MEMORY_TEMPLATE,
};

/// Returns true if the message contains at least one Text block
/// (not just ToolUse/ToolResult blocks).
pub fn has_text_blocks(msg: &ApiMessage) -> bool {
    msg.content
        .iter()
        .any(|b| matches!(b, ContentBlock::Text { .. }))
}

/// Adjusts `start_index` backward to avoid splitting tool_use/tool_result pairs.
///
/// Any ToolResult in the kept range (`start_index..`) whose tool_use_id does
/// not have a matching ToolUse in that same range is "orphaned". We scan
/// backward from `start_index - 1` to find the message(s) containing the
/// matching ToolUse and include them.
pub fn adjust_index_to_preserve_api_invariants(
    messages: &[ApiMessage],
    start_index: usize,
) -> usize {
    if start_index == 0 {
        return 0;
    }

    let kept = &messages[start_index..];

    // Collect tool_result IDs referenced in the kept range.
    let result_ids: Vec<&str> = kept
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| {
            if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                Some(tool_use_id.as_str())
            } else {
                None
            }
        })
        .collect();

    if result_ids.is_empty() {
        return start_index;
    }

    // Collect tool_use IDs already present in the kept range.
    let use_ids_in_kept: Vec<&str> = kept
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| {
            if let ContentBlock::ToolUse { id, .. } = b {
                Some(id.as_str())
            } else {
                None
            }
        })
        .collect();

    // Find orphaned result IDs (referenced but no matching use in kept range).
    let orphaned_ids: Vec<&str> = result_ids
        .iter()
        .copied()
        .filter(|id| !use_ids_in_kept.contains(id))
        .collect();

    if orphaned_ids.is_empty() {
        return start_index;
    }

    // Scan backward from start_index - 1 to find messages with matching ToolUse.
    let mut adjusted = start_index;
    let mut remaining_orphans: Vec<&str> = orphaned_ids;

    for i in (0..start_index).rev() {
        if remaining_orphans.is_empty() {
            break;
        }
        let msg = &messages[i];
        let has_matching_use = msg.content.iter().any(|b| {
            if let ContentBlock::ToolUse { id, .. } = b {
                remaining_orphans.contains(&id.as_str())
            } else {
                false
            }
        });
        if has_matching_use {
            adjusted = i;
            // Remove resolved IDs from remaining orphans.
            remaining_orphans.retain(|&orphan_id| {
                !msg.content.iter().any(|b| {
                    if let ContentBlock::ToolUse { id, .. } = b {
                        id.as_str() == orphan_id
                    } else {
                        false
                    }
                })
            });
        }
    }

    adjusted
}

/// Calculate the index from which messages should be kept for compaction.
///
/// Tries to keep `min_text_block_messages` text-bearing messages and
/// `min_tokens` worth of tokens, while not exceeding `max_tokens`.
/// Always starts from `last_summarized_index + 1`.
pub fn calculate_messages_to_keep_index(
    messages: &[ApiMessage],
    last_summarized_index: usize,
    config: &SessionMemoryCompactConfig,
) -> usize {
    if messages.is_empty() {
        return 0;
    }

    // Start from the message after the last summarized one.
    let base = (last_summarized_index + 1).min(messages.len());

    let total_tokens = estimate_message_tokens(&messages[base..]);
    let total_text_msgs = messages[base..]
        .iter()
        .filter(|m| has_text_blocks(m))
        .count();

    // Already at/over max_tokens — take only from base.
    if total_tokens >= config.max_tokens {
        return adjust_index_to_preserve_api_invariants(messages, base);
    }

    // Both minimums already satisfied by keeping from base.
    if total_tokens >= config.min_tokens && total_text_msgs >= config.min_text_block_messages {
        return adjust_index_to_preserve_api_invariants(messages, base);
    }

    // Expand backward one message at a time.
    let mut current_index = base;
    let mut accumulated_tokens = total_tokens;
    let mut accumulated_text_msgs = total_text_msgs;

    while current_index > 0 {
        // Check stop condition before including the previous message.
        if accumulated_tokens >= config.max_tokens {
            break;
        }
        if accumulated_tokens >= config.min_tokens
            && accumulated_text_msgs >= config.min_text_block_messages
        {
            break;
        }

        current_index -= 1;
        let msg = &messages[current_index];
        let msg_tokens = estimate_message_tokens(std::slice::from_ref(msg));
        accumulated_tokens += msg_tokens;
        if has_text_blocks(msg) {
            accumulated_text_msgs += 1;
        }
    }

    adjust_index_to_preserve_api_invariants(messages, current_index)
}

/// Check if a message is a compact boundary marker.
fn is_compact_boundary(msg: &ApiMessage) -> bool {
    if let Some(ContentBlock::Text { text }) = msg.content.first() {
        text.starts_with(COMPACT_BOUNDARY_PREFIX)
    } else {
        false
    }
}

/// Session-memory-based compaction: uses the pre-built session memory file as
/// the summary instead of calling an LLM. Returns `None` when compaction is
/// not possible or would not reduce tokens below `auto_compact_threshold`.
pub async fn try_session_memory_compaction(
    messages: &[ApiMessage],
    state: &SessionMemoryState,
    auto_compact_threshold: Option<u64>,
) -> Option<CompactResult> {
    // Wait for any in-progress extraction.
    wait_for_extraction(state).await;

    // Read the session memory file.
    let path = get_session_memory_path(&state.session_id);
    let content = super::utils::get_session_memory_content(&path).await?;

    // Bail out if content is empty / just the template.
    if content.trim().is_empty()
        || is_session_memory_empty(&content, DEFAULT_SESSION_MEMORY_TEMPLATE)
    {
        return None;
    }

    // Determine the last summarized index.
    let last_summarized_index = {
        let guard = state.last_summarized_index.read().await;
        guard.unwrap_or(messages.len().saturating_sub(1))
    };

    let keep_index =
        calculate_messages_to_keep_index(messages, last_summarized_index, &state.compact_config);

    // Collect kept messages, filtering old compact boundary markers.
    let kept_messages: Vec<ApiMessage> = messages[keep_index..]
        .iter()
        .filter(|m| !is_compact_boundary(m))
        .cloned()
        .collect();

    // Truncate session memory content if it is too long.
    let (truncated_content, _) = super::prompts::truncate_session_memory_for_compact(&content);

    // Build the boundary marker.
    let boundary = ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: format!("{}: session memory compaction]", COMPACT_BOUNDARY_PREFIX),
        }],
    };

    // Build the summary message from session memory content.
    let summary_text = get_compact_user_summary_message(&truncated_content, true, None, true);
    let summary_msg = ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text { text: summary_text }],
    };

    // Assemble final message list.
    let mut new_messages = Vec::with_capacity(2 + kept_messages.len());
    new_messages.push(boundary);
    new_messages.push(summary_msg);
    new_messages.extend(kept_messages);

    let pre_tokens = estimate_message_tokens(messages);
    let post_tokens = estimate_message_tokens(&new_messages);
    let tokens_freed = pre_tokens.saturating_sub(post_tokens);

    // If auto_compact_threshold is set, only return a result if we actually
    // brought token count below the threshold.
    if let Some(threshold) = auto_compact_threshold {
        if post_tokens >= threshold {
            return None;
        }
    }

    Some(CompactResult {
        messages: new_messages,
        tokens_freed,
        stage: CompactStage::SessionMemory,
        pre_compact_token_count: Some(pre_tokens),
        post_compact_token_count: Some(post_tokens),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::{ContentBlock, ToolResultContent};
    use crate::types::message::{ApiMessage, Role};

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage {
            role,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    #[test]
    fn test_has_text_blocks_true() {
        assert!(has_text_blocks(&text_msg(Role::User, "hello")));
    }

    #[test]
    fn test_has_text_blocks_false_tool_only() {
        let msg = ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t".into(),
                content: ToolResultContent::Text("ok".into()),
                is_error: false,
            }],
        };
        assert!(!has_text_blocks(&msg));
    }

    #[test]
    fn test_adjust_index_preserves_tool_pairs() {
        let msgs = vec![
            text_msg(Role::User, "old"),
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "Read".into(),
                    input: serde_json::json!({}),
                }],
            },
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: ToolResultContent::Text("ok".into()),
                    is_error: false,
                }],
            },
            text_msg(Role::User, "new"),
        ];
        let adjusted = adjust_index_to_preserve_api_invariants(&msgs, 2);
        assert_eq!(adjusted, 1);
    }

    #[test]
    fn test_adjust_index_no_orphans() {
        let msgs = vec![
            text_msg(Role::User, "a"),
            text_msg(Role::Assistant, "b"),
            text_msg(Role::User, "c"),
        ];
        let adjusted = adjust_index_to_preserve_api_invariants(&msgs, 1);
        assert_eq!(adjusted, 1);
    }

    #[test]
    fn test_calculate_messages_respects_min_messages() {
        use crate::services::session_memory::types::SessionMemoryCompactConfig;
        let config = SessionMemoryCompactConfig {
            min_tokens: 0,
            min_text_block_messages: 3,
            max_tokens: 100_000,
        };
        let msgs: Vec<_> = (0..10)
            .map(|i| text_msg(Role::User, &format!("msg {}", i)))
            .collect();
        let idx = calculate_messages_to_keep_index(&msgs, 5, &config);
        let kept = &msgs[idx..];
        let text_count = kept.iter().filter(|m| has_text_blocks(m)).count();
        assert!(text_count >= 3);
    }

    #[test]
    fn test_calculate_messages_respects_max_tokens() {
        use crate::services::session_memory::types::SessionMemoryCompactConfig;
        // Very low max_tokens — should not expand much
        let config = SessionMemoryCompactConfig {
            min_tokens: 0,
            min_text_block_messages: 0,
            max_tokens: 50,
        };
        let msgs: Vec<_> = (0..10)
            .map(|_| text_msg(Role::User, &"x".repeat(200)))
            .collect();
        let idx = calculate_messages_to_keep_index(&msgs, 0, &config);
        assert!(idx > 0); // Should not keep everything
    }
}
