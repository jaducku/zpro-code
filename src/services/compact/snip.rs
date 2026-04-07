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

    // Find turn boundaries (indices where Role::User messages start).
    // Tool result messages also have Role::User but are continuations
    // of the current turn, not new turns.
    let turn_starts: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m.role == Role::User
                && !m.content.iter().all(|c| matches!(c, ContentBlock::ToolResult { .. }))
        })
        .map(|(i, _)| i)
        .collect();

    if turn_starts.len() <= keep_turns {
        return 0;
    }

    let keep_from = turn_starts[turn_starts.len() - keep_turns];
    if keep_from == 0 {
        return 0;
    }

    let removed_count = keep_from;
    let kept = messages.split_off(keep_from);
    let tokens_removed = estimate_message_tokens(messages);

    let boundary = ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: format!(
                "{}: {} earlier messages removed. ~{}K tokens freed]",
                COMPACT_BOUNDARY_PREFIX,
                removed_count,
                tokens_removed / 1000
            ),
        }],
    };

    *messages = vec![boundary];
    messages.extend(kept);

    let tokens_after = estimate_message_tokens(messages);
    tokens_before.saturating_sub(tokens_after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::{ContentBlock, ToolResultContent};
    use crate::types::message::{ApiMessage, Role};

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage {
            role,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        }
    }

    fn tool_use_msg(id: &str, name: &str) -> ApiMessage {
        ApiMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: id.into(),
                name: name.into(),
                input: serde_json::json!({}),
            }],
        }
    }

    fn tool_result_msg(id: &str, text: &str) -> ApiMessage {
        ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.into(),
                content: ToolResultContent::Text(text.into()),
                is_error: false,
            }],
        }
    }

    #[test]
    fn test_snip_no_op_few_messages() {
        let mut msgs = vec![
            text_msg(Role::User, "hello"),
            text_msg(Role::Assistant, "hi"),
        ];
        let freed = snip(&mut msgs, 3);
        assert_eq!(freed, 0);
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn test_snip_removes_old() {
        let mut msgs = vec![
            text_msg(Role::User, "old1"),
            text_msg(Role::Assistant, "old1r"),
            text_msg(Role::User, "old2"),
            text_msg(Role::Assistant, "old2r"),
            text_msg(Role::User, "recent"),
            text_msg(Role::Assistant, "recentr"),
        ];
        let freed = snip(&mut msgs, 1);
        assert!(freed > 0);
        assert!(msgs.len() <= 3); // boundary + 1 turn
        assert!(msgs[0].content.iter().any(|c| matches!(
            c,
            ContentBlock::Text { text } if text.starts_with(COMPACT_BOUNDARY_PREFIX)
        )));
    }

    #[test]
    fn test_snip_preserves_tool_pairs() {
        let long_text = "x".repeat(500);
        let mut msgs = vec![
            text_msg(Role::User, &long_text),
            text_msg(Role::Assistant, &long_text),
            text_msg(Role::User, "do"),
            tool_use_msg("t1", "bash"),
            tool_result_msg("t1", "output"),
            text_msg(Role::Assistant, "done"),
            text_msg(Role::User, "recent"),
            text_msg(Role::Assistant, "reply"),
        ];
        let freed = snip(&mut msgs, 2);
        assert!(freed > 0);
        let has_tu = msgs
            .iter()
            .any(|m| m.content.iter().any(|c| matches!(c, ContentBlock::ToolUse { .. })));
        let has_tr = msgs
            .iter()
            .any(|m| m.content.iter().any(|c| matches!(c, ContentBlock::ToolResult { .. })));
        assert_eq!(has_tu, has_tr); // both present or both absent
    }

    #[test]
    fn test_snip_empty() {
        let mut msgs = vec![];
        assert_eq!(snip(&mut msgs, 3), 0);
    }
}
