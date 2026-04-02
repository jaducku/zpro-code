# Phase 4: Query Engine

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the core query loop that connects user input → LLM provider → tool execution → response. This is the heart of the agent.

**Architecture:** `query()` function that loops: send messages to LLM → receive response → extract tool calls → execute tools → loop until end_turn. Uses `LlmProvider` trait from Phase 2 and `Tool` trait from Phase 3.

**Tech Stack:** tokio, futures, serde_json

**Spec:** `docs/superpowers/specs/2026-04-02-ts-to-rust-migration-design.md` (Section 7)

---

### Task 1: Query types and context

**Files:**
- Create: `src/query/mod.rs`
- Create: `src/query/types.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Define query types**

```rust
// src/query/types.rs
use crate::services::api::provider::LlmProvider;
use crate::services::api::types::SystemBlock;
use crate::tool::{Tool, ToolCall, ToolResultBlock};
use crate::types::message::{ApiMessage, Message, StopReason};

/// Context for a query execution
pub struct QueryContext {
    pub messages: Vec<ApiMessage>,
    pub tools: Vec<Box<dyn Tool>>,
    pub system_prompt: Option<Vec<SystemBlock>>,
    pub model: String,
    pub max_tokens: u32,
}

/// Result of a query execution
pub struct QueryResult {
    pub messages: Vec<ApiMessage>,
    pub stop_reason: StopReason,
    pub tool_results: Vec<ToolResultBlock>,
}

/// Events emitted during query execution (for UI updates)
#[derive(Debug, Clone)]
pub enum QueryEvent {
    /// Streaming text from LLM
    TextDelta(String),
    /// Tool call started
    ToolCallStart { id: String, name: String },
    /// Tool call completed
    ToolCallComplete { id: String, result: ToolResultBlock },
    /// LLM response complete
    ResponseComplete { stop_reason: StopReason },
    /// Error occurred
    Error(String),
}
```

```rust
// src/query/mod.rs
pub mod types;
pub mod engine;

pub use types::{QueryContext, QueryResult, QueryEvent};
pub use engine::query;
```

- [ ] **Step 2: Add `pub mod query;` to `src/lib.rs`**

Note: `src/query/` is a directory module — different from the existing empty `src/query/` dir.

- [ ] **Step 3: Commit**

```bash
git add src/query/ src/lib.rs
git commit -m "feat: define query types (QueryContext, QueryResult, QueryEvent)"
```

---

### Task 2: Core query loop

**Files:**
- Create: `src/query/engine.rs`

This is the core agent loop: send to LLM → get response → execute tools → loop.

- [ ] **Step 1: Implement query engine**

```rust
// src/query/engine.rs
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::services::api::provider::LlmProvider;
use crate::services::api::types::*;
use crate::tool::{Tool, ToolCall, ToolResultBlock, ToolUseContext, ToolProgress};
use crate::tools::find_tool_by_name;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role, StopReason};

use super::types::{QueryContext, QueryResult, QueryEvent};

/// Maximum number of tool-use loops before forcing stop
const MAX_TOOL_LOOPS: usize = 50;

/// Execute the query loop: LLM call → tool execution → repeat until done
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
                StreamEvent::ContentBlockStart { content_block, .. } => {
                    match &content_block {
                        ContentBlock::ToolUse { id, name, .. } => {
                            current_tool_id = id.clone();
                            current_tool_name = name.clone();
                            current_tool_input.clear();
                            let _ = event_tx.send(QueryEvent::ToolCallStart {
                                id: id.clone(),
                                name: name.clone(),
                            });
                        }
                        _ => {}
                    }
                }
                StreamEvent::ContentBlockDelta { delta, .. } => {
                    match delta {
                        ContentDelta::TextDelta { text } => {
                            text_content.push_str(&text);
                            let _ = event_tx.send(QueryEvent::TextDelta(text));
                        }
                        ContentDelta::InputJsonDelta { partial_json } => {
                            current_tool_input.push_str(&partial_json);
                        }
                        ContentDelta::ThinkingDelta { .. } => {}
                    }
                }
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
            let _ = event_tx.send(QueryEvent::ResponseComplete { stop_reason: stop_reason.clone() });
            return Ok(QueryResult {
                messages: ctx.messages,
                stop_reason,
                tool_results: Vec::new(),
            });
        }

        // 5. Execute tools
        let mut tool_result_blocks = Vec::new();
        for tc in &tool_calls {
            let result = execute_tool(tc, &ctx.tools, &event_tx, &abort_signal).await;
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
    tools: &[Box<dyn Tool>],
    _event_tx: &mpsc::UnboundedSender<QueryEvent>,
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
            &ModelInfo {
                id: "test".into(),
                name: "test".into(),
                context_window: 128_000,
                max_output_tokens: 4096,
                capabilities: Capabilities::default(),
            }
        }
        fn capabilities(&self) -> &Capabilities {
            &Capabilities::default()
        }
    }

    /// Mock tool for testing
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str { "echo" }
        fn description(&self) -> &str { "Echo input" }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<crate::tool::ToolResult> {
            let msg = ctx.input.get("message").and_then(|v| v.as_str()).unwrap_or("no message");
            Ok(crate::tool::ToolResult {
                content: format!("Echo: {}", msg),
                is_error: false,
            })
        }
        fn permission_metadata(&self, _input: &serde_json::Value) -> PermissionMeta {
            PermissionMeta { file_path: None, command: None, url: None }
        }
    }

    #[tokio::test]
    async fn test_simple_text_response() {
        let provider = MockProvider {
            response_events: vec![
                StreamEvent::MessageStart { id: "msg_1".into() },
                StreamEvent::ContentBlockStart {
                    index: 0,
                    content_block: ContentBlock::Text { text: String::new() },
                },
                StreamEvent::ContentBlockDelta {
                    index: 0,
                    delta: ContentDelta::TextDelta { text: "Hello!".into() },
                },
                StreamEvent::ContentBlockStop { index: 0 },
                StreamEvent::MessageStop { stop_reason: StopReason::EndTurn },
            ],
        };

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let ctx = QueryContext {
            messages: vec![ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text { text: "Hi".into() }],
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
                let n = self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let events = if n == 0 {
                    // First call: tool use
                    vec![
                        StreamEvent::MessageStart { id: "msg_1".into() },
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
                        StreamEvent::MessageStop { stop_reason: StopReason::ToolUse },
                    ]
                } else {
                    // Second call: text response
                    vec![
                        StreamEvent::MessageStart { id: "msg_2".into() },
                        StreamEvent::ContentBlockStart {
                            index: 0,
                            content_block: ContentBlock::Text { text: String::new() },
                        },
                        StreamEvent::ContentBlockDelta {
                            index: 0,
                            delta: ContentDelta::TextDelta { text: "Done!".into() },
                        },
                        StreamEvent::ContentBlockStop { index: 0 },
                        StreamEvent::MessageStop { stop_reason: StopReason::EndTurn },
                    ]
                };
                Ok(Box::pin(futures::stream::iter(events.into_iter().map(Ok))))
            }
            fn model_info(&self) -> &ModelInfo {
                &ModelInfo {
                    id: "test".into(), name: "test".into(),
                    context_window: 128_000, max_output_tokens: 4096,
                    capabilities: Capabilities::default(),
                }
            }
            fn capabilities(&self) -> &Capabilities { &Capabilities::default() }
        }

        let provider = TwoStepProvider { call_count: cc };
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let ctx = QueryContext {
            messages: vec![ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text { text: "test".into() }],
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
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p atom-code query::engine`
Expected: 2 tests pass

- [ ] **Step 3: Commit**

```bash
git add src/query/
git commit -m "feat: implement core query engine with tool-use loop"
```

---

### Task 3: Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass (~80+)

- [ ] **Step 2: clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings
