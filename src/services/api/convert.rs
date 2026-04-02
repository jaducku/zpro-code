//!
//! Convert between internal message/tool types and OpenAI API format.

use crate::services::api::openai_types::*;
use crate::services::api::types::*;
use crate::tool::ToolDefinition;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role, StopReason};

/// Convert internal messages to OpenAI format
pub fn to_openai_messages(
    messages: &[ApiMessage],
    system: Option<&[SystemBlock]>,
) -> Vec<OpenAiMessage> {
    let mut result = Vec::new();

    // System prompt as first message
    if let Some(blocks) = system {
        let text: String = blocks
            .iter()
            .map(|b| match b {
                SystemBlock::Text { text } => text.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        if !text.is_empty() {
            result.push(OpenAiMessage {
                role: "system".into(),
                content: Some(OpenAiContent::Text(text)),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
    }

    for msg in messages {
        match msg.role {
            Role::User => {
                let content = content_blocks_to_openai(&msg.content);
                result.push(OpenAiMessage {
                    role: "user".into(),
                    content: Some(content),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
            Role::Assistant => {
                let mut text_parts = Vec::new();
                let mut tool_calls = Vec::new();

                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } => text_parts.push(text.clone()),
                        ContentBlock::ToolUse { id, name, input } => {
                            tool_calls.push(OpenAiToolCall {
                                id: id.clone(),
                                call_type: "function".into(),
                                function: OpenAiFunctionCall {
                                    name: name.clone(),
                                    arguments: serde_json::to_string(input).unwrap_or_default(),
                                },
                            });
                        }
                        _ => {}
                    }
                }

                let content = if text_parts.is_empty() {
                    None
                } else {
                    Some(OpenAiContent::Text(text_parts.join("")))
                };

                result.push(OpenAiMessage {
                    role: "assistant".into(),
                    content,
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
                    tool_call_id: None,
                    name: None,
                });
            }
            Role::Tool => {
                // Tool results: each tool_result block becomes a separate message
                for block in &msg.content {
                    if let ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } = block
                    {
                        let text = match content {
                            crate::types::content::ToolResultContent::Text(t) => t.clone(),
                            crate::types::content::ToolResultContent::Blocks(_) => {
                                serde_json::to_string(content).unwrap_or_default()
                            }
                        };
                        result.push(OpenAiMessage {
                            role: "tool".into(),
                            content: Some(OpenAiContent::Text(text)),
                            tool_calls: None,
                            tool_call_id: Some(tool_use_id.clone()),
                            name: None,
                        });
                    }
                }
            }
            Role::System => {
                // Additional system messages (rare)
                let text: String = msg
                    .content
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                result.push(OpenAiMessage {
                    role: "system".into(),
                    content: Some(OpenAiContent::Text(text)),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
        }
    }

    result
}

/// Convert internal tool definitions to OpenAI format
pub fn to_openai_tools(tools: &[ToolDefinition]) -> Vec<OpenAiTool> {
    tools
        .iter()
        .map(|t| OpenAiTool {
            tool_type: "function".into(),
            function: OpenAiFunction {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            },
        })
        .collect()
}

/// Convert OpenAI finish_reason to internal StopReason
pub fn from_openai_finish_reason(reason: &str) -> StopReason {
    match reason {
        "stop" => StopReason::EndTurn,
        "tool_calls" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        _ => StopReason::EndTurn,
    }
}

/// Convert OpenAI usage to internal UsageInfo
pub fn from_openai_usage(usage: &OpenAiUsage) -> UsageInfo {
    UsageInfo {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
    }
}

fn content_blocks_to_openai(blocks: &[ContentBlock]) -> OpenAiContent {
    let has_images = blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::Image { .. }));

    if has_images {
        let parts: Vec<OpenAiContentPart> = blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => {
                    Some(OpenAiContentPart::Text { text: text.clone() })
                }
                ContentBlock::Image { source } => {
                    let url = match source {
                        crate::types::content::ImageSource::Base64 {
                            media_type,
                            data,
                        } => {
                            format!("data:{};base64,{}", media_type, data)
                        }
                        crate::types::content::ImageSource::Url { url } => url.clone(),
                    };
                    Some(OpenAiContentPart::ImageUrl {
                        image_url: ImageUrlDetail { url, detail: None },
                    })
                }
                _ => None,
            })
            .collect();
        OpenAiContent::Parts(parts)
    } else {
        let text: String = blocks
            .iter()
            .filter_map(|b| {
                if let ContentBlock::Text { text } = b {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");
        OpenAiContent::Text(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_message_conversion() {
        let messages = vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
        }];
        let result = to_openai_messages(&messages, None);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        match &result[0].content {
            Some(OpenAiContent::Text(t)) => assert_eq!(t, "hello"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_system_prompt_prepended() {
        let messages = vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "hi".into(),
            }],
        }];
        let system = vec![SystemBlock::Text {
            text: "You are helpful".into(),
        }];
        let result = to_openai_messages(&messages, Some(&system));
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "system");
        assert_eq!(result[1].role, "user");
    }

    #[test]
    fn test_assistant_with_tool_calls() {
        let messages = vec![ApiMessage {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "Let me check".into(),
                },
                ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "BashTool".into(),
                    input: serde_json::json!({"command": "ls"}),
                },
            ],
        }];
        let result = to_openai_messages(&messages, None);
        assert_eq!(result.len(), 1);
        let msg = &result[0];
        assert_eq!(msg.role, "assistant");
        assert!(msg.content.is_some());
        let tcs = msg.tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].function.name, "BashTool");
    }

    #[test]
    fn test_tool_result_conversion() {
        let messages = vec![ApiMessage {
            role: Role::Tool,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "call_1".into(),
                content: crate::types::content::ToolResultContent::Text("file.txt".into()),
                is_error: false,
            }],
        }];
        let result = to_openai_messages(&messages, None);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "tool");
        assert_eq!(result[0].tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn test_tool_definition_conversion() {
        let tools = vec![ToolDefinition {
            name: "BashTool".into(),
            description: "Run bash".into(),
            input_schema: serde_json::json!({"type": "object", "properties": {"command": {"type": "string"}}}),
        }];
        let result = to_openai_tools(&tools);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].tool_type, "function");
        assert_eq!(result[0].function.name, "BashTool");
    }

    #[test]
    fn test_finish_reason_conversion() {
        assert_eq!(from_openai_finish_reason("stop"), StopReason::EndTurn);
        assert_eq!(from_openai_finish_reason("tool_calls"), StopReason::ToolUse);
        assert_eq!(from_openai_finish_reason("length"), StopReason::MaxTokens);
        assert_eq!(from_openai_finish_reason("unknown"), StopReason::EndTurn);
    }
}
