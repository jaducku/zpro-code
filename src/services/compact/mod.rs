pub mod full_compact;
pub mod microcompact;
pub mod prompt;
pub mod snip;
pub mod types;

pub use types::*;

use crate::services::token_estimation::estimate_message_tokens;
use crate::types::message::ApiMessage;

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
    use crate::types::content::ContentBlock;
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
}
