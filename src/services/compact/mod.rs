pub mod full_compact;
pub mod microcompact;
pub mod prompt;
pub mod snip;
pub mod types;

pub use types::*;

use crate::services::token_estimation::estimate_message_tokens;
use crate::types::content::ContentBlock;
use crate::types::message::ApiMessage;

/// Compute the token ratio (estimated_tokens / context_window).
pub fn token_ratio(messages: &[ApiMessage], context_window: u64) -> f64 {
    if context_window == 0 {
        return 0.0;
    }
    estimate_message_tokens(messages) as f64 / context_window as f64
}

/// Scan backwards for the last message whose text starts with COMPACT_BOUNDARY_PREFIX.
/// Return messages from that boundary index onward (inclusive).
/// If no boundary found, return all messages.
pub fn get_messages_after_compact_boundary(messages: &[ApiMessage]) -> &[ApiMessage] {
    for (idx, msg) in messages.iter().enumerate().rev() {
        if let Some(ContentBlock::Text { text }) = msg.content.first() {
            if text.starts_with(COMPACT_BOUNDARY_PREFIX) {
                return &messages[idx..];
            }
        }
    }
    messages
}

/// Clone each message, replacing ContentBlock::Image with ContentBlock::Text.
/// All other blocks pass through unchanged.
pub fn strip_images_from_messages(messages: &[ApiMessage]) -> Vec<ApiMessage> {
    messages
        .iter()
        .map(|msg| ApiMessage {
            role: msg.role.clone(),
            content: msg
                .content
                .iter()
                .map(|block| match block {
                    ContentBlock::Image { .. } => ContentBlock::Text {
                        text: "[image was shared]".to_string(),
                    },
                    other => other.clone(),
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::{ContentBlock, ImageSource};
    use crate::types::message::Role;

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage {
            role,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
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
        let msgs = vec![text_msg(Role::User, "hello")];
        let r = token_ratio(&msgs, 100);
        assert!(r > 0.0 && r < 0.1);
    }

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
            text_msg(
                Role::User,
                &format!("{}: 5 messages summarized]", COMPACT_BOUNDARY_PREFIX),
            ),
            text_msg(Role::Assistant, "summary"),
            text_msg(Role::User, "new"),
        ];
        let result = get_messages_after_compact_boundary(&msgs);
        assert_eq!(result.len(), 3); // boundary + summary + new
    }

    #[test]
    fn test_strip_images_from_messages() {
        let msgs = vec![ApiMessage {
            role: Role::User,
            content: vec![
                ContentBlock::Text {
                    text: "look".into(),
                },
                ContentBlock::Image {
                    source: ImageSource::Base64 {
                        media_type: "image/png".into(),
                        data: "abc".into(),
                    },
                },
            ],
        }];
        let stripped = strip_images_from_messages(&msgs);
        assert_eq!(stripped[0].content.len(), 2);
        assert!(
            matches!(&stripped[0].content[1], ContentBlock::Text { text } if text.contains("[image"))
        );
    }

    #[test]
    fn test_strip_images_no_images() {
        let msgs = vec![text_msg(Role::User, "hello")];
        let stripped = strip_images_from_messages(&msgs);
        assert_eq!(stripped.len(), 1);
        assert!(matches!(&stripped[0].content[0], ContentBlock::Text { text } if text == "hello"));
    }
}
