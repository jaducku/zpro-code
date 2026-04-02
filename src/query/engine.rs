use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::services::api::provider::LlmProvider;
use crate::services::api::types::*;
use crate::tool::{ToolCall, ToolProgress, ToolResultBlock, ToolUseContext};
use crate::tools::find_tool_by_name;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role, StopReason};

use super::types::{QueryContext, QueryEvent, QueryResult};

/// Maximum number of tool-use loops before forcing stop
const MAX_TOOL_LOOPS: usize = 50;

/// Execute the query loop: LLM call -> tool execution -> repeat until done
pub async fn query(
    mut ctx: QueryContext,
    provider: &dyn LlmProvider,
    event_tx: mpsc::UnboundedSender<QueryEvent>,
    abort_signal: CancellationToken,
) -> crate::types::Result<QueryResult> {
    let mut loop_count = 0;

    loop {
        if loop_count >= MAX_TOOL_LOOPS {
            let _ = event_tx.send(QueryEvent::Error(
                "Maximum tool-use loop count reached".into(),
            ));
            break;
        }
        loop_count += 1;

        // 1. Build request
        let request = CreateMessageRequest {
            model: ctx.model.clone(),
            messages: ctx.messages.clone(),
            tools: Some(
                ctx.tools
                    .iter()
                    .map(|t| crate::tool::ToolDefinition::from(t.as_ref()))
                    .collect(),
            ),
            max_tokens: ctx.max_tokens,
            system: ctx.system_prompt.clone(),
            temperature: None,
            stream: true,
        };

        // 2. Stream response
        let mut stream = provider.create_message(request).await?;

        let mut text_content = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut stop_reason = StopReason::EndTurn;
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_input = String::new();

        while let Some(event) = stream.next().await {
            if abort_signal.is_cancelled() {
                return Err(crate::types::AtomError::Cancelled);
            }

            let event = event?;

            match event {
                StreamEvent::ContentBlockStart {
                    content_block: ContentBlock::ToolUse { id, name, .. },
                    ..
                } => {
                    current_tool_id = id.clone();
                    current_tool_name = name.clone();
                    current_tool_input.clear();
                    let _ = event_tx.send(QueryEvent::ToolCallStart {
                        id: id.clone(),
                        name: name.clone(),
                    });
                }
                StreamEvent::ContentBlockStart { .. } => {}
                StreamEvent::ContentBlockDelta { delta, .. } => match delta {
                    ContentDelta::TextDelta { text } => {
                        text_content.push_str(&text);
                        let _ = event_tx.send(QueryEvent::TextDelta(text));
                    }
                    ContentDelta::InputJsonDelta { partial_json } => {
                        current_tool_input.push_str(&partial_json);
                    }
                    ContentDelta::ThinkingDelta { .. } => {}
                },
                StreamEvent::ContentBlockStop { .. } => {
                    if !current_tool_id.is_empty() {
                        let input: serde_json::Value =
                            serde_json::from_str(&current_tool_input).unwrap_or_default();
                        tool_calls.push(ToolCall {
                            id: current_tool_id.clone(),
                            name: current_tool_name.clone(),
                            input,
                        });
                        current_tool_id.clear();
                        current_tool_name.clear();
                        current_tool_input.clear();
                    }
                }
                StreamEvent::MessageStop { stop_reason: sr } => {
                    stop_reason = sr;
                }
                _ => {}
            }
        }

        // 3. Build assistant message content
        let mut assistant_content: Vec<ContentBlock> = Vec::new();
        if !text_content.is_empty() {
            assistant_content.push(ContentBlock::Text {
                text: text_content.clone(),
            });
        }
        for tc in &tool_calls {
            assistant_content.push(ContentBlock::ToolUse {
                id: tc.id.clone(),
                name: tc.name.clone(),
                input: tc.input.clone(),
            });
        }

        // Add assistant message to conversation
        ctx.messages.push(ApiMessage {
            role: Role::Assistant,
            content: assistant_content,
        });

        // 4. If no tool calls, we're done
        if tool_calls.is_empty() {
            let _ = event_tx.send(QueryEvent::ResponseComplete {
                stop_reason: stop_reason.clone(),
            });
            return Ok(QueryResult {
                messages: ctx.messages,
                stop_reason,
                tool_results: Vec::new(),
            });
        }

        // 5. Execute tools
        let mut tool_result_blocks = Vec::new();
        for tc in &tool_calls {
            let result = execute_tool(tc, &ctx.tools, &abort_signal).await;
            let block = match result {
                Ok(tr) => {
                    let block = ToolResultBlock::success(tc.id.clone(), tr.content.clone());
                    let _ = event_tx.send(QueryEvent::ToolCallComplete {
                        id: tc.id.clone(),
                        result: block.clone(),
                    });
                    if tr.is_error {
                        ToolResultBlock::error(tc.id.clone(), tr.content)
                    } else {
                        block
                    }
                }
                Err(e) => {
                    let block = ToolResultBlock::error(tc.id.clone(), e.to_string());
                    let _ = event_tx.send(QueryEvent::ToolCallComplete {
                        id: tc.id.clone(),
                        result: block.clone(),
                    });
                    block
                }
            };
            tool_result_blocks.push(block);
        }

        // 6. Add tool results as user message
        let tool_result_content: Vec<ContentBlock> = tool_result_blocks
            .iter()
            .map(|b| ContentBlock::ToolResult {
                tool_use_id: b.tool_use_id.clone(),
                content: crate::types::content::ToolResultContent::Text(b.content.clone()),
                is_error: b.is_error,
            })
            .collect();

        ctx.messages.push(ApiMessage {
            role: Role::Tool,
            content: tool_result_content,
        });

        // Loop back for next LLM call with tool results
    }

    Ok(QueryResult {
        messages: ctx.messages,
        stop_reason: StopReason::EndTurn,
        tool_results: Vec::new(),
    })
}

async fn execute_tool(
    tc: &ToolCall,
    tools: &[Box<dyn crate::tool::Tool>],
    abort_signal: &CancellationToken,
) -> crate::types::Result<crate::tool::ToolResult> {
    let tool = find_tool_by_name(tools, &tc.name).ok_or_else(|| crate::types::AtomError::Tool {
        tool_name: tc.name.clone(),
        message: format!("Tool '{}' not found", tc.name),
    })?;

    let (progress_tx, _progress_rx) = mpsc::unbounded_channel::<ToolProgress>();

    let ctx = ToolUseContext {
        input: tc.input.clone(),
        abort_signal: abort_signal.clone(),
        progress_tx,
    };

    tool.execute(ctx).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crate::types::permissions::PermissionMeta;

    /// Mock provider that returns a fixed response
    struct MockProvider {
        response_events: Vec<StreamEvent>,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn create_message(
            &self,
            _request: CreateMessageRequest,
        ) -> crate::types::Result<crate::services::api::provider::MessageStream> {
            let events = self.response_events.clone();
            Ok(Box::pin(futures::stream::iter(events.into_iter().map(Ok))))
        }
        fn model_info(&self) -> &ModelInfo {
            // SAFETY: leaking is fine in tests — small, bounded, and only runs once per test
            Box::leak(Box::new(ModelInfo {
                id: "test".into(),
                name: "test".into(),
                context_window: 128_000,
                max_output_tokens: 4096,
                capabilities: Capabilities::default(),
            }))
        }
        fn capabilities(&self) -> &Capabilities {
            // SAFETY: leaking is fine in tests
            Box::leak(Box::new(Capabilities::default()))
        }
    }

    /// Mock tool for testing
    struct EchoTool;

    #[async_trait]
    impl crate::tool::Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echo input"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(
            &self,
            ctx: ToolUseContext,
        ) -> crate::types::Result<crate::tool::ToolResult> {
            let msg = ctx
                .input
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("no message");
            Ok(crate::tool::ToolResult {
                content: format!("Echo: {}", msg),
                is_error: false,
            })
        }
        fn permission_metadata(&self, _input: &serde_json::Value) -> PermissionMeta {
            PermissionMeta {
                file_path: None,
                command: None,
                url: None,
            }
        }
    }

    #[tokio::test]
    async fn test_simple_text_response() {
        let provider = MockProvider {
            response_events: vec![
                StreamEvent::MessageStart {
                    id: "msg_1".into(),
                },
                StreamEvent::ContentBlockStart {
                    index: 0,
                    content_block: ContentBlock::Text {
                        text: String::new(),
                    },
                },
                StreamEvent::ContentBlockDelta {
                    index: 0,
                    delta: ContentDelta::TextDelta {
                        text: "Hello!".into(),
                    },
                },
                StreamEvent::ContentBlockStop { index: 0 },
                StreamEvent::MessageStop {
                    stop_reason: StopReason::EndTurn,
                },
            ],
        };

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let ctx = QueryContext {
            messages: vec![ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "Hi".into(),
                }],
            }],
            tools: vec![],
            system_prompt: None,
            model: "test".into(),
            max_tokens: 4096,
        };

        let result = query(ctx, &provider, event_tx, CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.stop_reason, StopReason::EndTurn);
        assert_eq!(result.messages.len(), 2); // user + assistant

        // Check events
        let mut got_text = false;
        while let Ok(event) = event_rx.try_recv() {
            if let QueryEvent::TextDelta(t) = event {
                assert_eq!(t, "Hello!");
                got_text = true;
            }
        }
        assert!(got_text);
    }

    #[tokio::test]
    async fn test_tool_use_loop() {
        // First call: LLM requests tool use
        // Second call: LLM responds with text after getting tool result
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let cc = call_count.clone();

        struct TwoStepProvider {
            call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }

        #[async_trait]
        impl LlmProvider for TwoStepProvider {
            async fn create_message(
                &self,
                _request: CreateMessageRequest,
            ) -> crate::types::Result<crate::services::api::provider::MessageStream> {
                let n = self
                    .call_count
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let events = if n == 0 {
                    // First call: tool use
                    vec![
                        StreamEvent::MessageStart {
                            id: "msg_1".into(),
                        },
                        StreamEvent::ContentBlockStart {
                            index: 0,
                            content_block: ContentBlock::ToolUse {
                                id: "call_1".into(),
                                name: "echo".into(),
                                input: serde_json::Value::Object(serde_json::Map::new()),
                            },
                        },
                        StreamEvent::ContentBlockDelta {
                            index: 0,
                            delta: ContentDelta::InputJsonDelta {
                                partial_json: r#"{"message":"test"}"#.into(),
                            },
                        },
                        StreamEvent::ContentBlockStop { index: 0 },
                        StreamEvent::MessageStop {
                            stop_reason: StopReason::ToolUse,
                        },
                    ]
                } else {
                    // Second call: text response
                    vec![
                        StreamEvent::MessageStart {
                            id: "msg_2".into(),
                        },
                        StreamEvent::ContentBlockStart {
                            index: 0,
                            content_block: ContentBlock::Text {
                                text: String::new(),
                            },
                        },
                        StreamEvent::ContentBlockDelta {
                            index: 0,
                            delta: ContentDelta::TextDelta {
                                text: "Done!".into(),
                            },
                        },
                        StreamEvent::ContentBlockStop { index: 0 },
                        StreamEvent::MessageStop {
                            stop_reason: StopReason::EndTurn,
                        },
                    ]
                };
                Ok(Box::pin(futures::stream::iter(events.into_iter().map(Ok))))
            }
            fn model_info(&self) -> &ModelInfo {
                Box::leak(Box::new(ModelInfo {
                    id: "test".into(),
                    name: "test".into(),
                    context_window: 128_000,
                    max_output_tokens: 4096,
                    capabilities: Capabilities::default(),
                }))
            }
            fn capabilities(&self) -> &Capabilities {
                Box::leak(Box::new(Capabilities::default()))
            }
        }

        let provider = TwoStepProvider { call_count: cc };
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let ctx = QueryContext {
            messages: vec![ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "test".into(),
                }],
            }],
            tools: vec![Box::new(EchoTool)],
            system_prompt: None,
            model: "test".into(),
            max_tokens: 4096,
        };

        let result = query(ctx, &provider, event_tx, CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.stop_reason, StopReason::EndTurn);
        // user + assistant(tool_use) + tool_result + assistant(text) = 4
        assert_eq!(result.messages.len(), 4);
        assert_eq!(
            call_count.load(std::sync::atomic::Ordering::SeqCst),
            2
        );
    }
}
