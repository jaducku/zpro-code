pub mod full_compact;
pub mod microcompact;
pub mod prompt;
pub mod snip;
pub mod types;

pub use types::*;

use std::sync::Arc;

use crate::services::api::provider::LlmProvider;
use crate::services::session_memory::SessionMemoryState;
use crate::services::token_estimation::estimate_message_tokens;
use crate::types::content::ContentBlock;
use crate::types::message::ApiMessage;
use crate::utils::abort_signal::AbortSignal;

/// Compute the auto-compact threshold: context window minus max summary size minus buffer.
pub fn get_auto_compact_threshold(context_window: u64, max_output_tokens: u32) -> u64 {
    let max_summary = std::cmp::min(max_output_tokens as u64, 20_000);
    let effective = context_window.saturating_sub(max_summary);
    effective.saturating_sub(AUTOCOMPACT_BUFFER_TOKENS)
}

/// Run auto-compaction if the token count exceeds the threshold.
/// Returns a CompactResult if compaction was performed, or None if not needed or skipped.
pub async fn auto_compact_if_needed(
    messages: &mut Vec<ApiMessage>,
    provider: &Arc<dyn LlmProvider>,
    session_memory: Option<&Arc<SessionMemoryState>>,
    consecutive_failures: &mut u32,
    snip_tokens_freed: u64,
    query_source: Option<&str>,
    abort_signal: &AbortSignal,
) -> Option<CompactResult> {
    // Recursion guards
    if query_source == Some("session_memory") || query_source == Some("compact") {
        return None;
    }
    // Env var check
    if std::env::var("DISABLE_COMPACT").is_ok() || std::env::var("DISABLE_AUTO_COMPACT").is_ok() {
        return None;
    }
    // Circuit breaker
    if *consecutive_failures >= types::MAX_CONSECUTIVE_COMPACT_FAILURES {
        return None;
    }
    // Threshold check
    let model_info = provider.model_info();
    let threshold =
        get_auto_compact_threshold(model_info.context_window, model_info.max_output_tokens);
    let token_count = estimate_message_tokens(messages);
    if token_count.saturating_sub(snip_tokens_freed) < threshold {
        return None;
    }

    // 1. Try session memory compaction
    if let Some(sm) = session_memory {
        if let Some(result) =
            crate::services::session_memory::compaction::try_session_memory_compaction(
                messages,
                sm,
                Some(threshold),
            )
            .await
        {
            *consecutive_failures = 0;
            return Some(result);
        }
    }

    // 2. Fallback: microcompact + full compact
    let mut work = messages.clone();
    microcompact::microcompact(&mut work, 3);
    match full_compact::full_compact(
        &work,
        provider.as_ref(),
        model_info.context_window,
        3,
        None,
        abort_signal,
    )
    .await
    {
        Ok(result) => {
            *consecutive_failures = 0;
            Some(result)
        }
        Err(e) => {
            *consecutive_failures += 1;
            tracing::warn!(
                "Auto-compact failed ({}/{}): {}",
                consecutive_failures,
                types::MAX_CONSECUTIVE_COMPACT_FAILURES,
                e
            );
            None
        }
    }
}

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

    #[test]
    fn test_get_auto_compact_threshold() {
        let t = get_auto_compact_threshold(200_000, 8192);
        // effective = 200000 - 8192 = 191808, threshold = 191808 - 13000 = 178808
        assert_eq!(t, 178_808);
    }

    #[test]
    fn test_get_auto_compact_threshold_caps_at_20k() {
        let t = get_auto_compact_threshold(200_000, 32_000);
        // effective = 200000 - 20000 = 180000, threshold = 180000 - 13000 = 167000
        assert_eq!(t, 167_000);
    }

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
