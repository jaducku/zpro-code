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
- Error states or blockers encountered\n\
\n\
Be concise but complete. Output a structured summary.";

pub async fn full_compact(
    messages: &[ApiMessage],
    provider: &dyn LlmProvider,
    _context_window: u64,
    keep_turns: usize,
) -> Result<CompactResult, AtomError> {
    let keep_from = find_keep_boundary(messages, keep_turns);
    if keep_from == 0 {
        return Err(AtomError::Config("Nothing to compact".into()));
    }

    let to_summarize = &messages[..keep_from];
    let to_keep = &messages[keep_from..];
    let tokens_before = estimate_message_tokens(messages);

    // Build summary request
    let model = provider.model_info().id.clone();
    let request = build_summary_request(to_summarize, &model);

    // Call LLM and collect summary text from stream
    let mut stream = provider.create_message(request).await?;
    let mut summary_text = String::new();

    while let Some(event) = stream.next().await {
        if let StreamEvent::ContentBlockDelta {
            delta: ContentDelta::TextDelta { text },
            ..
        } = event?
        {
            summary_text.push_str(&text);
        }
    }

    if summary_text.is_empty() {
        return Err(AtomError::Api {
            message: "LLM returned empty summary".into(),
            status: None,
        });
    }

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
        role: Role::Assistant,
        content: vec![ContentBlock::Text {
            text: summary_text,
        }],
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

/// Build a CreateMessageRequest that asks the LLM to summarize the given messages.
pub fn build_summary_request(to_summarize: &[ApiMessage], model: &str) -> CreateMessageRequest {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn text_msg(role: Role, text: &str) -> ApiMessage {
        ApiMessage {
            role,
            content: vec![ContentBlock::Text {
                text: text.into(),
            }],
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
        let msgs = vec![
            text_msg(Role::User, "q"),
            text_msg(Role::Assistant, "a"),
        ];
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
        let req = build_summary_request(&msgs, "test-model");
        assert!(req.system.is_some());
        assert!(!req.messages.is_empty());
        assert_eq!(req.max_tokens, 4096);
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
        let req = build_summary_request(&msgs, "m");
        let text = match &req.messages[0].content[0] {
            ContentBlock::Text { text } => text,
            _ => panic!("expected text block"),
        };
        assert!(text.contains("User: fix the bug"));
        assert!(text.contains("Assistant: I'll look into it"));
    }
}
