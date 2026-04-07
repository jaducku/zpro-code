use std::collections::HashMap;

use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};

use super::convert::*;
use super::openai_types::*;
use super::provider::{LlmProvider, MessageStream};
use super::types::*;
use crate::types::content::ContentBlock;
use crate::types::message::StopReason;
use crate::types::AtomError;
use crate::types::Result;

/// OpenAI-compatible provider (OpenRouter, vLLM, local models)
pub struct ChatCompletionsProvider {
    base_url: String,
    api_key: Option<String>,
    _model: String,
    client: Client,
    model_info: ModelInfo,
}

impl ChatCompletionsProvider {
    pub fn new(base_url: String, api_key: Option<String>, model: String) -> Self {
        let model_info = ModelInfo {
            id: model.clone(),
            name: model.clone(),
            context_window: 128_000,
            max_output_tokens: 4096,
            capabilities: Capabilities {
                tool_use: true,
                vision: true,
                streaming: true,
                extended_thinking: false,
            },
        };

        Self {
            base_url,
            api_key,
            _model: model,
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                // Note: no .timeout() — streaming responses can take minutes.
                // Per-chunk timeouts are handled by tokio::time::timeout in the stream.
                .build()
                .unwrap_or_else(|_| Client::new()),
            model_info,
        }
    }

    /// Configure model info (context window, max tokens, capabilities)
    pub fn with_model_info(mut self, info: ModelInfo) -> Self {
        self.model_info = info;
        self
    }

    fn build_request(&self, request: &CreateMessageRequest) -> OpenAiRequest {
        let messages = to_openai_messages(&request.messages, request.system.as_deref());
        let tools = request.tools.as_ref()
            .map(|t| to_openai_tools(t))
            .filter(|t| !t.is_empty());

        OpenAiRequest {
            model: request.model.clone(),
            messages,
            tools,
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
            stream: Some(request.stream),
            stream_options: if request.stream {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
        }
    }
}

#[async_trait]
impl LlmProvider for ChatCompletionsProvider {
    async fn create_message(&self, request: CreateMessageRequest) -> Result<MessageStream> {
        let url = format!(
            "{}/chat/completions",
            self.base_url.trim_end_matches('/')
        );

        // Try with full features, auto-retry with fallbacks on errors
        let result = self.send_request(&url, &request, false).await;

        match result {
            Err(AtomError::Api { status: Some(s), ref message })
                if (s == 400 || s == 404)
                    && (message.contains("instruction") || message.contains("system")) =>
            {
                // Model doesn't support system role — retry with system-as-user
                self.send_request(&url, &request, true).await
            }
            Err(AtomError::Api { status: Some(s), ref message })
                if (s == 400 || s == 404)
                    && (message.contains("tool") || message.contains("function")) =>
            {
                // Model doesn't support tool use — retry without tools
                let mut request_no_tools = request.clone();
                request_no_tools.tools = None;
                self.send_request(&url, &request_no_tools, false).await
            }
            other => other,
        }
    }

    fn model_info(&self) -> &ModelInfo {
        &self.model_info
    }

    fn capabilities(&self) -> &Capabilities {
        &self.model_info.capabilities
    }
}

impl ChatCompletionsProvider {
    async fn send_request(
        &self,
        url: &str,
        request: &CreateMessageRequest,
        system_as_user: bool,
    ) -> Result<MessageStream> {
        let openai_req = if system_as_user {
            self.build_request_system_as_user(request)
        } else {
            self.build_request(request)
        };

        let body = serde_json::to_string(&openai_req)
            .map_err(|e| AtomError::Api {
                message: e.to_string(),
                status: None,
            })?;

        let mut http_req = self
            .client
            .post(url)
            .header("Content-Type", "application/json");

        if let Some(ref key) = self.api_key {
            http_req = http_req.header("Authorization", format!("Bearer {}", key));
        }

        if !request.stream {
            let resp = http_req.body(body).send().await?;
            let status = resp.status();
            if !status.is_success() {
                let error_text = resp.text().await.unwrap_or_default();
                return Err(AtomError::Api {
                    message: format!("API error {}: {}", status, error_text),
                    status: Some(status.as_u16()),
                });
            }
            let openai_resp: OpenAiResponse = resp.json().await?;
            let events = non_streaming_to_events(openai_resp);
            return Ok(Box::pin(stream::iter(events.into_iter().map(Ok))));
        }

        let resp = http_req.body(body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(AtomError::Api {
                message: format!("API error {}: {}", status, error_text),
                status: Some(status.as_u16()),
            });
        }

        let stream = parse_sse_response(resp);
        Ok(Box::pin(stream))
    }

    /// Build request with system prompt as user message (for models that don't support system role)
    fn build_request_system_as_user(&self, request: &CreateMessageRequest) -> OpenAiRequest {
        let mut messages = Vec::new();

        // Inject system prompt as first user message
        if let Some(system) = &request.system {
            let text: String = system
                .iter()
                .map(|b| match b { SystemBlock::Text { text } => text.as_str() })
                .collect::<Vec<_>>()
                .join("\n\n");
            if !text.is_empty() {
                messages.push(OpenAiMessage {
                    role: "user".into(),
                    content: Some(OpenAiContent::Text(format!("[System Instructions]\n{}", text))),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
                messages.push(OpenAiMessage {
                    role: "assistant".into(),
                    content: Some(OpenAiContent::Text("Understood. I'll follow these instructions.".into())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
        }

        // Add regular messages (without system)
        let regular = to_openai_messages(&request.messages, None);
        messages.extend(regular);

        let tools = request.tools.as_ref()
            .map(|t| to_openai_tools(t))
            .filter(|t| !t.is_empty());

        OpenAiRequest {
            model: request.model.clone(),
            messages,
            tools,
            max_tokens: Some(request.max_tokens),
            temperature: request.temperature,
            stream: Some(request.stream),
            stream_options: if request.stream {
                Some(StreamOptions { include_usage: true })
            } else {
                None
            },
        }
    }

}

/// Convert non-streaming response to stream events
fn non_streaming_to_events(resp: OpenAiResponse) -> Vec<StreamEvent> {
    let mut events = vec![StreamEvent::MessageStart { id: resp.id }];

    if let Some(choice) = resp.choices.first() {
        let mut index = 0;

        // Text content
        if let Some(OpenAiContent::Text(text)) = &choice.message.content {
            if !text.is_empty() {
                events.push(StreamEvent::ContentBlockStart {
                    index,
                    content_block: ContentBlock::Text {
                        text: String::new(),
                    },
                });
                events.push(StreamEvent::ContentBlockDelta {
                    index,
                    delta: ContentDelta::TextDelta { text: text.clone() },
                });
                events.push(StreamEvent::ContentBlockStop { index });
                index += 1;
            }
        }

        // Tool calls
        if let Some(tool_calls) = &choice.message.tool_calls {
            for tc in tool_calls {
                let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                events.push(StreamEvent::ContentBlockStart {
                    index,
                    content_block: ContentBlock::ToolUse {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        input,
                    },
                });
                events.push(StreamEvent::ContentBlockStop { index });
                index += 1;
            }
        }

        // Stop reason
        let stop_reason = choice
            .finish_reason
            .as_deref()
            .map(from_openai_finish_reason)
            .unwrap_or(StopReason::EndTurn);
        events.push(StreamEvent::MessageStop { stop_reason });
    } else {
        // Empty choices — still emit MessageStop so TUI doesn't hang
        events.push(StreamEvent::MessageStop { stop_reason: StopReason::EndTurn });
    }

    // Usage
    if let Some(usage) = resp.usage {
        events.push(StreamEvent::Usage(from_openai_usage(&usage)));
    }

    events
}

/// Parse SSE from a raw reqwest Response using byte streaming
fn parse_sse_response(resp: reqwest::Response) -> BoxStream<'static, Result<StreamEvent>> {
    struct StreamState {
        message_id: Option<String>,
        tool_call_index_map: HashMap<usize, usize>,
        next_content_index: usize,
        text_started: bool,
        tool_ids: HashMap<usize, String>,
        tool_names: HashMap<usize, String>,
        buffer: String,
    }

    let state = std::sync::Arc::new(tokio::sync::Mutex::new(StreamState {
        message_id: None,
        tool_call_index_map: HashMap::new(),
        next_content_index: 0,
        text_started: false,
        tool_ids: HashMap::new(),
        tool_names: HashMap::new(),
        buffer: String::new(),
    }));

    // Map byte_stream to Vec<u8> so we can chain a flush sentinel
    let byte_stream = resp.bytes_stream()
        .map(|r| r.map(|b| b.to_vec()).map_err(|e| e.to_string()));
    // Chain a trailing newline to flush any remaining buffered SSE data
    // when the server closes the connection without a final \n
    let flush = stream::once(async { Ok::<Vec<u8>, String>(b"\n".to_vec()) });
    let byte_stream = byte_stream.chain(flush);

    let stream = byte_stream
        .filter_map(move |chunk_result| {
            let state = state.clone();
            async move {
                let chunk = match chunk_result {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        return Some(vec![Err(AtomError::Api {
                            message: format!("Stream error: {}", e),
                            status: None,
                        })]);
                    }
                };

                let text = String::from_utf8_lossy(&chunk);
                let mut state = state.lock().await;
                state.buffer.push_str(&text);

                let mut all_events: Vec<Result<StreamEvent>> = Vec::new();

                // Process complete lines from buffer
                while let Some(line_end) = state.buffer.find('\n') {
                    let line = state.buffer[..line_end].trim_end_matches('\r').to_string();
                    state.buffer = state.buffer[line_end + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    let data = if let Some(d) = line.strip_prefix("data: ") {
                        d
                    } else if let Some(d) = line.strip_prefix("data:") {
                        d
                    } else {
                        continue;
                    };

                    if data == "[DONE]" {
                        continue;
                    }

                    let chunk: OpenAiStreamChunk = match serde_json::from_str(data) {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!("SSE parse error: {} — raw data: {}", e, data);
                            // Try to extract error from raw JSON
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                                if let Some(err_msg) = val.get("error")
                                    .and_then(|e| e.get("message"))
                                    .and_then(|m| m.as_str())
                                {
                                    all_events.push(Err(AtomError::Api {
                                        message: format!("Provider error: {}", err_msg),
                                        status: None,
                                    }));
                                }
                            }
                            continue;
                        }
                    };

                    // Handle error objects in SSE stream (e.g., OpenRouter rate limits)
                    if let Some(ref err) = chunk.error {
                        all_events.push(Err(AtomError::Api {
                            message: format!("Provider error: {}", err.message),
                            status: err.code.as_ref().and_then(|c| c.as_u64()).map(|c| c as u16),
                        }));
                        continue;
                    }

                    // Message start
                    if state.message_id.is_none() {
                        state.message_id = Some(chunk.id.clone());
                        all_events.push(Ok(StreamEvent::MessageStart { id: chunk.id.clone() }));
                    }

                    for choice in &chunk.choices {
                        // Reasoning/thinking delta (Qwen 3, DeepSeek, etc.)
                        if let Some(reasoning) = &choice.delta.reasoning_content {
                            if !reasoning.is_empty() {
                                all_events.push(Ok(StreamEvent::ContentBlockDelta {
                                    index: 0,
                                    delta: ContentDelta::ThinkingDelta { thinking: reasoning.clone() },
                                }));
                            }
                        }

                        // Text delta
                        if let Some(text) = &choice.delta.content {
                            if !state.text_started {
                                state.text_started = true;
                                all_events.push(Ok(StreamEvent::ContentBlockStart {
                                    index: state.next_content_index,
                                    content_block: ContentBlock::Text { text: String::new() },
                                }));
                            }
                            if !text.is_empty() {
                                all_events.push(Ok(StreamEvent::ContentBlockDelta {
                                    index: state.next_content_index,
                                    delta: ContentDelta::TextDelta { text: text.clone() },
                                }));
                            }
                        }

                        // Tool call deltas
                        if let Some(tool_calls) = &choice.delta.tool_calls {
                            for tc_delta in tool_calls {
                                let oi = tc_delta.index;
                                if !state.tool_call_index_map.contains_key(&oi) {
                                    if state.text_started && state.tool_call_index_map.is_empty() {
                                        all_events.push(Ok(StreamEvent::ContentBlockStop {
                                            index: state.next_content_index,
                                        }));
                                        state.next_content_index += 1;
                                        state.text_started = false;
                                    }
                                    let ci = state.next_content_index;
                                    state.tool_call_index_map.insert(oi, ci);
                                    state.next_content_index += 1;
                                    if let Some(id) = &tc_delta.id {
                                        state.tool_ids.insert(oi, id.clone());
                                    }
                                    if let Some(func) = &tc_delta.function {
                                        if let Some(name) = &func.name {
                                            state.tool_names.insert(oi, name.clone());
                                        }
                                    }
                                    all_events.push(Ok(StreamEvent::ContentBlockStart {
                                        index: ci,
                                        content_block: ContentBlock::ToolUse {
                                            id: state.tool_ids.get(&oi).cloned().unwrap_or_default(),
                                            name: state.tool_names.get(&oi).cloned().unwrap_or_default(),
                                            input: serde_json::Value::Object(serde_json::Map::new()),
                                        },
                                    }));
                                }
                                if let Some(func) = &tc_delta.function {
                                    if let Some(args) = &func.arguments {
                                        if let Some(&ci) = state.tool_call_index_map.get(&oi) {
                                            all_events.push(Ok(StreamEvent::ContentBlockDelta {
                                                index: ci,
                                                delta: ContentDelta::InputJsonDelta { partial_json: args.clone() },
                                            }));
                                        }
                                    }
                                }
                            }
                        }

                        // Finish reason
                        if let Some(reason) = &choice.finish_reason {
                            if state.text_started {
                                all_events.push(Ok(StreamEvent::ContentBlockStop {
                                    index: state.next_content_index,
                                }));
                            }
                            for &ci in state.tool_call_index_map.values() {
                                all_events.push(Ok(StreamEvent::ContentBlockStop { index: ci }));
                            }
                            all_events.push(Ok(StreamEvent::MessageStop {
                                stop_reason: from_openai_finish_reason(reason),
                            }));
                        }
                    }

                    if let Some(usage) = &chunk.usage {
                        all_events.push(Ok(StreamEvent::Usage(from_openai_usage(usage))));
                    }
                }

                if all_events.is_empty() {
                    None
                } else {
                    Some(all_events)
                }
            }
        })
        .flat_map(stream::iter);

    stream.boxed()
}

/// Convert SSE event source to StreamEvent stream (legacy, kept for reference)
#[allow(dead_code)]
fn sse_to_stream_events(event_source: EventSource) -> BoxStream<'static, Result<StreamEvent>> {
    // State for accumulating tool call deltas
    struct StreamState {
        message_id: Option<String>,
        tool_call_index_map: HashMap<usize, usize>, // openai index -> our content block index
        next_content_index: usize,
        text_started: bool,
        tool_ids: HashMap<usize, String>,
        tool_names: HashMap<usize, String>,
    }

    let state = std::sync::Arc::new(tokio::sync::Mutex::new(StreamState {
        message_id: None,
        tool_call_index_map: HashMap::new(),
        next_content_index: 0,
        text_started: false,
        tool_ids: HashMap::new(),
        tool_names: HashMap::new(),
    }));

    let stream = event_source
        .filter_map(move |event| {
            let state = state.clone();
            async move {
                match event {
                    Ok(Event::Message(msg)) => {
                        if msg.data == "[DONE]" {
                            return None;
                        }
                        let chunk: OpenAiStreamChunk = match serde_json::from_str(&msg.data) {
                            Ok(c) => c,
                            Err(e) => {
                                return Some(Err(AtomError::Api {
                                    message: format!("Failed to parse SSE chunk: {}", e),
                                    status: None,
                                }))
                            }
                        };

                        let mut state = state.lock().await;
                        let mut events = Vec::new();

                        // Message start
                        if state.message_id.is_none() {
                            state.message_id = Some(chunk.id.clone());
                            events.push(StreamEvent::MessageStart { id: chunk.id });
                        }

                        for choice in &chunk.choices {
                            // Text delta
                            if let Some(text) = &choice.delta.content {
                                if !state.text_started {
                                    state.text_started = true;
                                    events.push(StreamEvent::ContentBlockStart {
                                        index: state.next_content_index,
                                        content_block: ContentBlock::Text {
                                            text: String::new(),
                                        },
                                    });
                                }
                                events.push(StreamEvent::ContentBlockDelta {
                                    index: state.next_content_index,
                                    delta: ContentDelta::TextDelta { text: text.clone() },
                                });
                            }

                            // Tool call deltas
                            if let Some(tool_calls) = &choice.delta.tool_calls {
                                for tc_delta in tool_calls {
                                    let oi = tc_delta.index;
                                    if !state.tool_call_index_map.contains_key(&oi) {
                                        // Close text block before first tool
                                        if state.text_started
                                            && state.tool_call_index_map.is_empty()
                                        {
                                            events.push(StreamEvent::ContentBlockStop {
                                                index: state.next_content_index,
                                            });
                                            state.next_content_index += 1;
                                            state.text_started = false;
                                        }

                                        let ci = state.next_content_index;
                                        state.tool_call_index_map.insert(oi, ci);
                                        state.next_content_index += 1;

                                        if let Some(id) = &tc_delta.id {
                                            state.tool_ids.insert(oi, id.clone());
                                        }
                                        if let Some(func) = &tc_delta.function {
                                            if let Some(name) = &func.name {
                                                state.tool_names.insert(oi, name.clone());
                                            }
                                        }

                                        events.push(StreamEvent::ContentBlockStart {
                                            index: ci,
                                            content_block: ContentBlock::ToolUse {
                                                id: state
                                                    .tool_ids
                                                    .get(&oi)
                                                    .cloned()
                                                    .unwrap_or_default(),
                                                name: state
                                                    .tool_names
                                                    .get(&oi)
                                                    .cloned()
                                                    .unwrap_or_default(),
                                                input: serde_json::Value::Object(
                                                    serde_json::Map::new(),
                                                ),
                                            },
                                        });
                                    }

                                    // Accumulate arguments
                                    if let Some(func) = &tc_delta.function {
                                        if let Some(args) = &func.arguments {
                                            let ci = state.tool_call_index_map[&oi];
                                            events.push(StreamEvent::ContentBlockDelta {
                                                index: ci,
                                                delta: ContentDelta::InputJsonDelta {
                                                    partial_json: args.clone(),
                                                },
                                            });
                                        }
                                    }
                                }
                            }

                            // Finish reason
                            if let Some(reason) = &choice.finish_reason {
                                // Close any open blocks
                                if state.text_started {
                                    events.push(StreamEvent::ContentBlockStop {
                                        index: state.next_content_index,
                                    });
                                }
                                for &ci in state.tool_call_index_map.values() {
                                    events.push(StreamEvent::ContentBlockStop { index: ci });
                                }

                                events.push(StreamEvent::MessageStop {
                                    stop_reason: from_openai_finish_reason(reason),
                                });
                            }
                        }

                        // Usage
                        if let Some(usage) = &chunk.usage {
                            events.push(StreamEvent::Usage(from_openai_usage(usage)));
                        }

                        if events.is_empty() {
                            None
                        } else {
                            Some(Ok(events))
                        }
                    }
                    Ok(Event::Open) => None,
                    Err(e) => Some(Err(AtomError::Api {
                        message: format!("SSE error: {}", e),
                        status: None,
                    })),
                }
            }
        })
        .flat_map(|result| match result {
            Ok(events) => stream::iter(events.into_iter().map(Ok)).boxed(),
            Err(e) => stream::once(async move { Err(e) }).boxed(),
        });

    stream.boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = ChatCompletionsProvider::new(
            "https://openrouter.ai/api/v1".into(),
            Some("sk-test".into()),
            "anthropic/claude-sonnet-4-20250514".into(),
        );
        assert_eq!(
            provider.model_info().id,
            "anthropic/claude-sonnet-4-20250514"
        );
        assert!(provider.capabilities().tool_use);
        assert!(provider.capabilities().streaming);
    }

    #[test]
    fn test_non_streaming_to_events() {
        let resp = OpenAiResponse {
            id: "chatcmpl-123".into(),
            choices: vec![OpenAiChoice {
                index: 0,
                message: OpenAiMessage {
                    role: "assistant".into(),
                    content: Some(OpenAiContent::Text("Hello!".into())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
                finish_reason: Some("stop".into()),
            }],
            usage: Some(OpenAiUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
        };
        let events = non_streaming_to_events(resp);
        assert!(matches!(events[0], StreamEvent::MessageStart { .. }));
        assert!(matches!(events[1], StreamEvent::ContentBlockStart { .. }));
        assert!(matches!(events[2], StreamEvent::ContentBlockDelta { .. }));
        assert!(matches!(events[3], StreamEvent::ContentBlockStop { .. }));
        assert!(matches!(
            events[4],
            StreamEvent::MessageStop {
                stop_reason: StopReason::EndTurn
            }
        ));
        assert!(matches!(events[5], StreamEvent::Usage(_)));
    }

    #[test]
    fn test_non_streaming_with_tool_calls() {
        let resp = OpenAiResponse {
            id: "chatcmpl-456".into(),
            choices: vec![OpenAiChoice {
                index: 0,
                message: OpenAiMessage {
                    role: "assistant".into(),
                    content: None,
                    tool_calls: Some(vec![OpenAiToolCall {
                        id: "call_1".into(),
                        call_type: "function".into(),
                        function: OpenAiFunctionCall {
                            name: "BashTool".into(),
                            arguments: r#"{"command":"ls"}"#.into(),
                        },
                    }]),
                    tool_call_id: None,
                    name: None,
                },
                finish_reason: Some("tool_calls".into()),
            }],
            usage: None,
        };
        let events = non_streaming_to_events(resp);
        // MessageStart, ToolUse ContentBlockStart, ContentBlockStop, MessageStop(ToolUse)
        assert!(matches!(
            events.last(),
            Some(StreamEvent::MessageStop {
                stop_reason: StopReason::ToolUse
            })
        ));
    }
}
