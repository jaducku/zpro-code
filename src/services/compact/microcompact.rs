use crate::services::token_estimation::estimate_tokens;
use crate::types::content::{ContentBlock, ToolResultContent};
use crate::types::message::ApiMessage;

const MAX_CHARS: usize = 2000;
const KEEP_HEAD: usize = 500;
const KEEP_TAIL: usize = 500;
const IMAGE_TOKEN_ESTIMATE: u64 = 1000;

pub fn microcompact(messages: &mut [ApiMessage], preserve_last: usize) -> u64 {
    // 1. Find indices of messages that contain ToolResult blocks (counting from end)
    let tool_result_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        })
        .map(|(i, _)| i)
        .collect();

    // Determine which message indices to preserve (last N tool-result messages)
    let preserved_count = preserve_last.min(tool_result_indices.len());
    let preserved_start = tool_result_indices.len() - preserved_count;
    let preserved_indices: std::collections::HashSet<usize> = tool_result_indices
        [preserved_start..]
        .iter()
        .copied()
        .collect();

    let mut tokens_freed: u64 = 0;

    for (msg_idx, msg) in messages.iter_mut().enumerate() {
        if preserved_indices.contains(&msg_idx) {
            continue;
        }

        // Remove image blocks and truncate tool results
        let mut new_content = Vec::with_capacity(msg.content.len());
        for block in msg.content.drain(..) {
            match block {
                ContentBlock::Image { .. } => {
                    tokens_freed += IMAGE_TOKEN_ESTIMATE;
                    // Drop the image block
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    let (new_content_inner, freed) = truncate_tool_result_content(content);
                    tokens_freed += freed;
                    new_content.push(ContentBlock::ToolResult {
                        tool_use_id,
                        content: new_content_inner,
                        is_error,
                    });
                }
                other => {
                    new_content.push(other);
                }
            }
        }
        msg.content = new_content;
    }

    tokens_freed
}

fn truncate_tool_result_content(content: ToolResultContent) -> (ToolResultContent, u64) {
    match content {
        ToolResultContent::Text(text) => {
            let (new_text, freed) = truncate_text(text);
            (ToolResultContent::Text(new_text), freed)
        }
        ToolResultContent::Blocks(blocks) => {
            let mut freed = 0u64;
            let mut new_blocks = Vec::with_capacity(blocks.len());
            for block in blocks {
                match block {
                    ContentBlock::Text { text } => {
                        let (new_text, f) = truncate_text(text);
                        freed += f;
                        new_blocks.push(ContentBlock::Text { text: new_text });
                    }
                    ContentBlock::Image { .. } => {
                        freed += IMAGE_TOKEN_ESTIMATE;
                        // Drop image blocks inside tool results too
                    }
                    other => {
                        new_blocks.push(other);
                    }
                }
            }
            (ToolResultContent::Blocks(new_blocks), freed)
        }
    }
}

fn truncate_text(text: String) -> (String, u64) {
    if text.len() <= MAX_CHARS {
        return (text, 0);
    }

    let original_tokens = estimate_tokens(&text);
    let removed = text.len() - KEEP_HEAD - KEEP_TAIL;
    let truncated = format!(
        "{}...[truncated {} chars]...{}",
        &text[..KEEP_HEAD],
        removed,
        &text[text.len() - KEEP_TAIL..]
    );
    let new_tokens = estimate_tokens(&truncated);
    let freed = original_tokens.saturating_sub(new_tokens);
    (truncated, freed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::{ContentBlock, ImageSource, ToolResultContent};
    use crate::types::message::{ApiMessage, Role};

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

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage {
            role,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    #[test]
    fn test_truncates_long_tool_result() {
        let long_text = "x".repeat(5000);
        let mut msgs = vec![
            tool_result_msg("t1", &long_text),
            text_msg(Role::User, "r"),
            text_msg(Role::Assistant, "a"),
        ];
        let freed = microcompact(&mut msgs, 0);
        assert!(freed > 0);
        if let ContentBlock::ToolResult {
            content: ToolResultContent::Text(text),
            ..
        } = &msgs[0].content[0]
        {
            assert!(text.len() < 5000);
            assert!(text.contains("truncated"));
        } else {
            panic!("Expected ToolResult with Text content");
        }
    }

    #[test]
    fn test_preserves_recent() {
        let long_text = "x".repeat(5000);
        let mut msgs = vec![tool_result_msg("t1", &long_text)];
        let freed = microcompact(&mut msgs, 1);
        assert_eq!(freed, 0);
    }

    #[test]
    fn test_short_unchanged() {
        let mut msgs = vec![tool_result_msg("t1", "short")];
        assert_eq!(microcompact(&mut msgs, 0), 0);
    }

    #[test]
    fn test_blocks_variant() {
        let long_text = "y".repeat(5000);
        let mut msgs = vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t1".into(),
                content: ToolResultContent::Blocks(vec![ContentBlock::Text { text: long_text }]),
                is_error: false,
            }],
        }];
        let freed = microcompact(&mut msgs, 0);
        assert!(freed > 0);
    }

    #[test]
    fn test_empty() {
        let mut msgs = vec![];
        assert_eq!(microcompact(&mut msgs, 0), 0);
    }

    #[test]
    fn test_removes_old_images() {
        let mut msgs = vec![
            ApiMessage {
                role: Role::User,
                content: vec![
                    ContentBlock::Text {
                        text: "look at this".into(),
                    },
                    ContentBlock::Image {
                        source: ImageSource::Base64 {
                            media_type: "image/png".into(),
                            data: "abc".into(),
                        },
                    },
                ],
            },
            text_msg(Role::User, "recent"),
        ];
        let freed = microcompact(&mut msgs, 0);
        assert!(freed > 0);
        // Image should be removed from first message
        assert!(!msgs[0]
            .content
            .iter()
            .any(|c| matches!(c, ContentBlock::Image { .. })));
    }

    #[test]
    fn test_preserve_last_skips_most_recent_tool_results() {
        let long_text = "z".repeat(5000);
        let mut msgs = vec![
            tool_result_msg("t1", &long_text),
            tool_result_msg("t2", &long_text),
            tool_result_msg("t3", &long_text),
        ];
        let freed = microcompact(&mut msgs, 2);
        assert!(freed > 0);
        // t1 should be truncated
        if let ContentBlock::ToolResult {
            content: ToolResultContent::Text(text),
            ..
        } = &msgs[0].content[0]
        {
            assert!(text.contains("truncated"));
        }
        // t2 and t3 should be preserved
        if let ContentBlock::ToolResult {
            content: ToolResultContent::Text(text),
            ..
        } = &msgs[1].content[0]
        {
            assert!(!text.contains("truncated"));
        }
        if let ContentBlock::ToolResult {
            content: ToolResultContent::Text(text),
            ..
        } = &msgs[2].content[0]
        {
            assert!(!text.contains("truncated"));
        }
    }

    #[test]
    fn test_truncation_format() {
        let text = "A".repeat(3000);
        let (truncated, _) = truncate_text(text.clone());
        assert!(truncated.starts_with(&"A".repeat(500)));
        assert!(truncated.ends_with(&"A".repeat(500)));
        assert!(truncated.contains("...[truncated 2000 chars]..."));
    }
}
