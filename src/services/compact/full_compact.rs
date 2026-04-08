use futures::StreamExt;

use crate::services::api::provider::LlmProvider;
use crate::services::api::types::*;
use crate::services::token_estimation::estimate_message_tokens;
use crate::types::content::ContentBlock;
use crate::types::error::AtomError;
use crate::types::message::{ApiMessage, Role};

use super::prompt::{format_compact_summary, get_compact_prompt, get_compact_user_summary_message};
use super::types::{
    CompactResult, CompactStage, COMPACT_BOUNDARY_PREFIX, COMPACT_MAX_OUTPUT_TOKENS,
    ERROR_MESSAGE_INCOMPLETE_RESPONSE, MAX_PTL_RETRIES,
};

pub async fn full_compact(
    messages: &[ApiMessage],
    provider: &dyn LlmProvider,
    _context_window: u64,
    keep_turns: usize,
    custom_instructions: Option<&str>,
    abort_signal: &crate::utils::abort_signal::AbortSignal,
) -> Result<CompactResult, AtomError> {
    let keep_from = find_keep_boundary(messages, keep_turns);
    if keep_from == 0 {
        return Err(AtomError::Config("Nothing to compact".into()));
    }

    let to_summarize = &messages[..keep_from];
    let to_keep = &messages[keep_from..];
    let tokens_before = estimate_message_tokens(messages);

    let model = provider.model_info().id.clone();
    let stripped = super::strip_images_from_messages(to_summarize);

    // PTL retry loop
    let mut attempt: u32 = 0;
    let mut working_messages = stripped.to_vec();
    let summary_text = loop {
        let req = build_summary_request(&working_messages, &model, custom_instructions);
        match try_stream_summary(provider, &req, abort_signal).await {
            Ok(text) if !text.is_empty() => break text,
            Ok(_) => {
                return Err(AtomError::Api {
                    message: ERROR_MESSAGE_INCOMPLETE_RESPONSE.into(),
                    status: None,
                })
            }
            Err(e) if is_prompt_too_long(&e) && attempt < MAX_PTL_RETRIES => {
                let drop_count = (working_messages.len() / 5).max(1);
                working_messages = working_messages[drop_count..].to_vec();
                working_messages.insert(
                    0,
                    ApiMessage {
                        role: Role::User,
                        content: vec![ContentBlock::Text {
                            text: "[earlier conversation truncated for compaction retry]".into(),
                        }],
                    },
                );
                attempt += 1;
                continue;
            }
            Err(e) => return Err(e),
        }
    };

    let formatted = format_compact_summary(&summary_text);
    let summary_msg_text = get_compact_user_summary_message(&formatted, false, None, false);

    let boundary = ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: format!(
                "{}: {} messages summarized by LLM]",
                COMPACT_BOUNDARY_PREFIX,
                to_summarize.len()
            ),
        }],
    };
    let summary = ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: summary_msg_text,
        }],
    };

    let mut result_messages = vec![boundary, summary];
    result_messages.extend_from_slice(to_keep);

    Ok(CompactResult {
        tokens_freed: tokens_before.saturating_sub(estimate_message_tokens(&result_messages)),
        stage: CompactStage::FullCompact,
        pre_compact_token_count: Some(tokens_before),
        post_compact_token_count: Some(estimate_message_tokens(&result_messages)),
        messages: result_messages,
    })
}

/// Stream the summary request and collect text output.
async fn try_stream_summary(
    provider: &dyn LlmProvider,
    req: &CreateMessageRequest,
    abort_signal: &crate::utils::abort_signal::AbortSignal,
) -> Result<String, AtomError> {
    let mut stream = provider.create_message(req.clone()).await?;
    let mut summary_text = String::new();

    while let Some(event) = stream.next().await {
        if abort_signal.is_cancelled() {
            return Err(AtomError::Cancelled);
        }
        if let StreamEvent::ContentBlockDelta {
            delta: ContentDelta::TextDelta { text },
            ..
        } = event?
        {
            summary_text.push_str(&text);
        }
    }

    Ok(summary_text)
}

/// Build a CreateMessageRequest that asks the LLM to summarize the given messages.
pub fn build_summary_request(
    to_summarize: &[ApiMessage],
    model: &str,
    custom_instructions: Option<&str>,
) -> CreateMessageRequest {
    let conversation_text: String = to_summarize
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
                Role::System => "System",
            };
            let content: String = m
                .content
                .iter()
                .map(|c| match c {
                    ContentBlock::Text { text } => text.clone(),
                    ContentBlock::ToolUse { name, .. } => format!("[Tool call: {}]", name),
                    ContentBlock::ToolResult { content, .. } => {
                        use crate::types::content::ToolResultContent;
                        match content {
                            ToolResultContent::Text(t) => {
                                if t.len() > 200 {
                                    format!("{}...", &t[..200])
                                } else {
                                    t.clone()
                                }
                            }
                            ToolResultContent::Blocks(_) => "[tool result blocks]".into(),
                        }
                    }
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("{}: {}", role, content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let compact_prompt = get_compact_prompt(custom_instructions);
    let user_text = format!("{}\n\n{}", conversation_text, compact_prompt);

    CreateMessageRequest {
        model: model.to_string(),
        messages: vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text { text: user_text }],
        }],
        tools: None,
        system: Some(vec![SystemBlock::Text {
            text: "You are a helpful AI assistant tasked with summarizing conversations.".into(),
        }]),
        max_tokens: COMPACT_MAX_OUTPUT_TOKENS,
        temperature: None,
        stream: true,
    }
}

fn is_prompt_too_long(error: &AtomError) -> bool {
    let msg = error.to_string().to_lowercase();
    msg.contains("prompt_too_long")
        || msg.contains("too many tokens")
        || msg.contains("context_length")
}

/// Find the index where kept messages start. Messages before this index will be summarized.
/// A "turn" is a User message that isn't purely ToolResult blocks.
pub fn find_keep_boundary(messages: &[ApiMessage], keep_turns: usize) -> usize {
    let turn_starts: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m.role == Role::User
                && !m
                    .content
                    .iter()
                    .all(|c| matches!(c, ContentBlock::ToolResult { .. }))
        })
        .map(|(i, _)| i)
        .collect();

    if turn_starts.len() <= keep_turns {
        return 0;
    }
    turn_starts[turn_starts.len() - keep_turns]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage {
            role,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    #[test]
    fn test_find_keep_boundary() {
        let msgs = vec![
            text_msg(Role::User, "old"),
            text_msg(Role::Assistant, "old reply"),
            text_msg(Role::User, "recent"),
            text_msg(Role::Assistant, "recent reply"),
        ];
        assert_eq!(find_keep_boundary(&msgs, 1), 2);
    }

    #[test]
    fn test_find_keep_boundary_all_kept() {
        let msgs = vec![text_msg(Role::User, "q"), text_msg(Role::Assistant, "a")];
        assert_eq!(find_keep_boundary(&msgs, 3), 0);
    }

    #[test]
    fn test_find_keep_boundary_tool_result_only_not_counted() {
        let msgs = vec![
            text_msg(Role::User, "first"),
            text_msg(Role::Assistant, "reply"),
            // ToolResult-only user message should not count as a turn
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: crate::types::content::ToolResultContent::Text("ok".into()),
                    is_error: false,
                }],
            },
            text_msg(Role::Assistant, "continued"),
            text_msg(Role::User, "second"),
            text_msg(Role::Assistant, "reply2"),
        ];
        // Only 2 real turns: "first" at 0, "second" at 4
        assert_eq!(find_keep_boundary(&msgs, 1), 4);
    }

    #[test]
    fn test_build_summary_request() {
        let msgs = vec![
            text_msg(Role::User, "hello"),
            text_msg(Role::Assistant, "world"),
        ];
        let req = build_summary_request(&msgs, "test-model", None);
        assert!(req.system.is_some());
        assert!(!req.messages.is_empty());
        assert_eq!(req.max_tokens, COMPACT_MAX_OUTPUT_TOKENS);
        assert!(req.tools.is_none());
        assert!(req.stream);
        assert_eq!(req.model, "test-model");
    }

    #[test]
    fn test_build_summary_request_content() {
        let msgs = vec![
            text_msg(Role::User, "fix the bug"),
            text_msg(Role::Assistant, "I'll look into it"),
        ];
        let req = build_summary_request(&msgs, "m", None);
        let text = match &req.messages[0].content[0] {
            ContentBlock::Text { text } => text,
            _ => panic!("expected text block"),
        };
        assert!(text.contains("User: fix the bug"));
        assert!(text.contains("Assistant: I'll look into it"));
    }

    #[test]
    fn test_build_summary_request_uses_structured_prompt() {
        let msgs = vec![text_msg(Role::User, "fix the bug")];
        let req = build_summary_request(&msgs, "m", None);
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
}
