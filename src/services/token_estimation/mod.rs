use crate::types::content::{ContentBlock, ToolResultContent};
use crate::types::message::ApiMessage;

/// Estimate token count for a text string (~4 chars per token heuristic).
pub fn estimate_tokens(text: &str) -> u64 {
    let len = text.len() as u64;
    if len == 0 {
        return 0;
    }
    (len + 3) / 4
}

/// Estimate total tokens for API messages.
pub fn estimate_message_tokens(messages: &[ApiMessage]) -> u64 {
    let content_tokens: u64 = messages
        .iter()
        .map(|m| {
            m.content
                .iter()
                .map(|c| match c {
                    ContentBlock::Text { text } => estimate_tokens(text),
                    ContentBlock::ToolUse { input, .. } => {
                        estimate_tokens(&input.to_string())
                    }
                    ContentBlock::ToolResult { content, .. } => match content {
                        ToolResultContent::Text(text) => estimate_tokens(text),
                        ToolResultContent::Blocks(blocks) => blocks
                            .iter()
                            .map(|b| match b {
                                ContentBlock::Text { text } => estimate_tokens(text),
                                _ => estimate_tokens(&format!("{:?}", b)),
                            })
                            .sum(),
                    },
                    ContentBlock::Image { .. } => 1000,
                    ContentBlock::Thinking { thinking, .. } => estimate_tokens(thinking),
                })
                .sum::<u64>()
        })
        .sum();
    content_tokens + (messages.len() as u64 * 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_short() {
        let t = estimate_tokens("hello world");
        assert!(t > 0 && t < 10);
    }

    #[test]
    fn test_estimate_tokens_long() {
        let t = estimate_tokens(&"a".repeat(400));
        assert_eq!(t, 100);
    }
}
