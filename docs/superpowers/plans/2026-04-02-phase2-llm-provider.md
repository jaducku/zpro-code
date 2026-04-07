# Phase 2: LLM Provider Implementation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the OpenAI-compatible LLM provider with SSE streaming, tool call support, and retry logic.

**Architecture:** `OpenAiCompatibleProvider` implementing the `LlmProvider` trait from Phase 1. Uses `reqwest` + `reqwest-eventsource` for SSE streaming. Covers OpenRouter, vLLM, and local models via the OpenAI chat/completions API format.

**Tech Stack:** reqwest, reqwest-eventsource, serde, tokio, futures

**Spec:** `docs/superpowers/specs/2026-04-02-ts-to-rust-migration-design.md` (Section 4)

---

### Task 1: OpenAI API request/response types

**Files:**
- Create: `src/services/api/openai_types.rs`
- Modify: `src/services/api/mod.rs`

- [ ] **Step 1: Define OpenAI chat/completions request/response types**

```rust
// src/services/api/openai_types.rs
use serde::{Deserialize, Serialize};

/// OpenAI chat/completions request body
#[derive(Debug, Clone, Serialize)]
pub struct OpenAiRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

/// OpenAI message format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<OpenAiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Content can be a string or array of content parts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAiContent {
    Text(String),
    Parts(Vec<OpenAiContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OpenAiContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrlDetail },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlDetail {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// OpenAI tool definition
#[derive(Debug, Clone, Serialize)]
pub struct OpenAiTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAiFunction,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// OpenAI tool call in assistant message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: OpenAiFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiFunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Non-streaming response
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiResponse {
    pub id: String,
    pub choices: Vec<OpenAiChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChoice {
    pub index: usize,
    pub message: OpenAiMessage,
    pub finish_reason: Option<String>,
}

/// Streaming chunk
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiStreamChunk {
    pub id: String,
    pub choices: Vec<OpenAiStreamChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiStreamChoice {
    pub index: usize,
    pub delta: OpenAiDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCallDelta>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiToolCallDelta {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<OpenAiFunctionCallDelta>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiFunctionCallDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = OpenAiRequest {
            model: "gpt-4".into(),
            messages: vec![OpenAiMessage {
                role: "user".into(),
                content: Some(OpenAiContent::Text("hello".into())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            tools: None,
            max_tokens: Some(1024),
            temperature: None,
            stream: Some(true),
            stream_options: Some(StreamOptions { include_usage: true }),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "gpt-4");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["stream"], true);
        assert!(json.get("temperature").is_none());
    }

    #[test]
    fn test_stream_chunk_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "delta": {"content": "Hello"},
                "finish_reason": null
            }]
        }"#;
        let chunk: OpenAiStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

    #[test]
    fn test_tool_call_delta_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_abc",
                        "type": "function",
                        "function": {"name": "get_weather", "arguments": "{\"loc"}
                    }]
                },
                "finish_reason": null
            }]
        }"#;
        let chunk: OpenAiStreamChunk = serde_json::from_str(json).unwrap();
        let tc = &chunk.choices[0].delta.tool_calls.as_ref().unwrap()[0];
        assert_eq!(tc.id.as_deref(), Some("call_abc"));
        assert_eq!(tc.function.as_ref().unwrap().name.as_deref(), Some("get_weather"));
    }

    #[test]
    fn test_usage_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150}
        }"#;
        let chunk: OpenAiStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.usage.as_ref().unwrap().total_tokens, 150);
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
    }
}
```

- [ ] **Step 2: Update mod.rs**

Add `pub mod openai_types;` to `src/services/api/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code services::api::openai_types`
Expected: 4 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/services/api/openai_types.rs src/services/api/mod.rs
git commit -m "feat: define OpenAI-compatible API request/response types"
```

---

### Task 2: Format conversion (internal <-> OpenAI)

**Files:**
- Create: `src/services/api/convert.rs`

- [ ] **Step 1: Implement format converters with tests**

```rust
// src/services/api/convert.rs
//!
//! Convert between internal message/tool types and OpenAI API format.

use crate::services::api::openai_types::*;
use crate::services::api::types::*;
use crate::tool::ToolDefinition;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role, StopReason};

/// Convert internal messages to OpenAI format
pub fn to_openai_messages(messages: &[ApiMessage], system: Option<&[SystemBlock]>) -> Vec<OpenAiMessage> {
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
                    tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                    tool_call_id: None,
                    name: None,
                });
            }
            Role::Tool => {
                // Tool results: each tool_result block becomes a separate message
                for block in &msg.content {
                    if let ContentBlock::ToolResult { tool_use_id, content, .. } = block {
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
                let text: String = msg.content.iter().filter_map(|b| {
                    if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
                }).collect::<Vec<_>>().join("\n");
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
    let has_images = blocks.iter().any(|b| matches!(b, ContentBlock::Image { .. }));

    if has_images {
        let parts: Vec<OpenAiContentPart> = blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(OpenAiContentPart::Text { text: text.clone() }),
                ContentBlock::Image { source } => {
                    let url = match source {
                        crate::types::content::ImageSource::Base64 { media_type, data } => {
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
                if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
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
            content: vec![ContentBlock::Text { text: "hello".into() }],
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
            content: vec![ContentBlock::Text { text: "hi".into() }],
        }];
        let system = vec![SystemBlock::Text { text: "You are helpful".into() }];
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
                ContentBlock::Text { text: "Let me check".into() },
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
```

- [ ] **Step 2: Update mod.rs**

Add `pub mod convert;` to `src/services/api/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code services::api::convert`
Expected: 6 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/services/api/convert.rs src/services/api/mod.rs
git commit -m "feat: implement internal<->OpenAI format conversion"
```

---

### Task 3: OpenAI-compatible provider with SSE streaming

**Files:**
- Create: `src/services/api/openai_provider.rs`

- [ ] **Step 1: Implement the provider**

```rust
// src/services/api/openai_provider.rs
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
use crate::types::Result;
use crate::types::AtomError;

/// OpenAI-compatible provider (OpenRouter, vLLM, local models)
pub struct OpenAiCompatibleProvider {
    base_url: String,
    api_key: Option<String>,
    model: String,
    client: Client,
    model_info: ModelInfo,
}

impl OpenAiCompatibleProvider {
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
            model,
            client: Client::new(),
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
        let tools = request.tools.as_ref().map(|t| to_openai_tools(t));

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

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    async fn create_message(&self, request: CreateMessageRequest) -> Result<MessageStream> {
        let openai_req = self.build_request(&request);
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let mut http_req = self.client.post(&url)
            .header("Content-Type", "application/json");

        if let Some(ref key) = self.api_key {
            http_req = http_req.header("Authorization", format!("Bearer {}", key));
        }

        let body = serde_json::to_string(&openai_req)
            .map_err(|e| AtomError::Api { message: e.to_string(), status: None })?;

        if !request.stream {
            // Non-streaming: single request/response
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

        // Streaming: SSE
        let req = self.client.post(&url)
            .header("Content-Type", "application/json");
        let req = if let Some(ref key) = self.api_key {
            req.header("Authorization", format!("Bearer {}", key))
        } else {
            req
        };

        let event_source = EventSource::new(req.body(body))
            .map_err(|e| AtomError::Api { message: e.to_string(), status: None })?;

        let stream = sse_to_stream_events(event_source);
        Ok(Box::pin(stream))
    }

    fn model_info(&self) -> &ModelInfo {
        &self.model_info
    }

    fn capabilities(&self) -> &Capabilities {
        &self.model_info.capabilities
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
                    content_block: ContentBlock::Text { text: String::new() },
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
        let stop_reason = choice.finish_reason.as_deref()
            .map(from_openai_finish_reason)
            .unwrap_or(StopReason::EndTurn);
        events.push(StreamEvent::MessageStop { stop_reason });
    }

    // Usage
    if let Some(usage) = resp.usage {
        events.push(StreamEvent::Usage(from_openai_usage(&usage)));
    }

    events
}

/// Convert SSE event source to StreamEvent stream
fn sse_to_stream_events(event_source: EventSource) -> BoxStream<'static, Result<StreamEvent>> {
    // State for accumulating tool call deltas
    struct StreamState {
        message_id: Option<String>,
        tool_call_index_map: std::collections::HashMap<usize, usize>, // openai index -> our content block index
        next_content_index: usize,
        text_started: bool,
        tool_arguments: std::collections::HashMap<usize, String>, // openai index -> accumulated args
        tool_names: std::collections::HashMap<usize, String>,
        tool_ids: std::collections::HashMap<usize, String>,
    }

    let state = std::sync::Arc::new(tokio::sync::Mutex::new(StreamState {
        message_id: None,
        tool_call_index_map: std::collections::HashMap::new(),
        next_content_index: 0,
        text_started: false,
        tool_arguments: std::collections::HashMap::new(),
        tool_names: std::collections::HashMap::new(),
        tool_ids: std::collections::HashMap::new(),
    }));

    let stream = event_source.filter_map(move |event| {
        let state = state.clone();
        async move {
            match event {
                Ok(Event::Message(msg)) => {
                    if msg.data == "[DONE]" {
                        return None;
                    }
                    let chunk: OpenAiStreamChunk = match serde_json::from_str(&msg.data) {
                        Ok(c) => c,
                        Err(e) => return Some(Err(AtomError::Api {
                            message: format!("Failed to parse SSE chunk: {}", e),
                            status: None,
                        })),
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
                                    content_block: ContentBlock::Text { text: String::new() },
                                });
                            }
                            events.push(StreamEvent::ContentBlockDelta {
                                index: state.next_content_index,
                                delta: ContentDelta::TextDelta { text: text.clone() },
                            });
                        }

                        // Tool call deltas
                        if let Some(tool_calls) = &choice.delta.tool_calls {
                            // Close text block if transitioning to tools
                            if state.text_started && !state.tool_call_index_map.is_empty() == false {
                                // First tool call after text
                            }

                            for tc_delta in tool_calls {
                                let oi = tc_delta.index;
                                if !state.tool_call_index_map.contains_key(&oi) {
                                    // Close text block before first tool
                                    if state.text_started && state.tool_call_index_map.is_empty() {
                                        events.push(StreamEvent::ContentBlockStop {
                                            index: state.next_content_index,
                                        });
                                        state.next_content_index += 1;
                                        state.text_started = false;
                                    }

                                    let ci = state.next_content_index;
                                    state.tool_call_index_map.insert(oi, ci);
                                    state.tool_arguments.insert(oi, String::new());
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
                                            id: state.tool_ids.get(&oi).cloned().unwrap_or_default(),
                                            name: state.tool_names.get(&oi).cloned().unwrap_or_default(),
                                            input: serde_json::Value::Object(serde_json::Map::new()),
                                        },
                                    });
                                }

                                // Accumulate arguments
                                if let Some(func) = &tc_delta.function {
                                    if let Some(args) = &func.arguments {
                                        if let Some(accumulated) = state.tool_arguments.get_mut(&oi) {
                                            accumulated.push_str(args);
                                        }
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
                            for (&oi, &ci) in &state.tool_call_index_map {
                                let _ = oi;
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
    .flat_map(|result| {
        match result {
            Ok(events) => stream::iter(events.into_iter().map(Ok)).boxed(),
            Err(e) => stream::once(async move { Err(e) }).boxed(),
        }
    });

    stream.boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = OpenAiCompatibleProvider::new(
            "https://openrouter.ai/api/v1".into(),
            Some("sk-test".into()),
            "anthropic/claude-sonnet-4-20250514".into(),
        );
        assert_eq!(provider.model_info().id, "anthropic/claude-sonnet-4-20250514");
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
        assert!(matches!(events[4], StreamEvent::MessageStop { stop_reason: StopReason::EndTurn }));
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
        assert!(matches!(events.last(), Some(StreamEvent::MessageStop { stop_reason: StopReason::ToolUse })));
    }
}
```

- [ ] **Step 2: Update mod.rs**

Add `pub mod openai_provider;` and re-export:
```rust
pub use openai_provider::OpenAiCompatibleProvider;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code services::api::openai_provider`
Expected: 3 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/services/api/openai_provider.rs src/services/api/mod.rs
git commit -m "feat: implement OpenAI-compatible provider with SSE streaming"
```

---

### Task 4: Retry logic

**Files:**
- Create: `src/services/api/retry.rs`

- [ ] **Step 1: Implement retry with exponential backoff**

```rust
// src/services/api/retry.rs
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

use crate::types::{AtomError, Result};

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub retryable_status_codes: Vec<u16>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 30_000,
            retryable_status_codes: vec![429, 500, 502, 503, 529],
        }
    }
}

/// Execute an async operation with retry logic
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt == config.max_retries || !is_retryable(&e, config) {
                    return Err(e);
                }

                let delay = calculate_delay(attempt, config);
                warn!(
                    attempt = attempt + 1,
                    max_retries = config.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    error = %e,
                    "Retrying after error"
                );
                sleep(delay).await;
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or(AtomError::Api {
        message: "Retry exhausted".into(),
        status: None,
    }))
}

fn is_retryable(error: &AtomError, config: &RetryConfig) -> bool {
    match error {
        AtomError::Api { status: Some(code), .. } => {
            config.retryable_status_codes.contains(code)
        }
        AtomError::Http(_) => true,
        AtomError::Io(_) => true,
        _ => false,
    }
}

fn calculate_delay(attempt: u32, config: &RetryConfig) -> Duration {
    let delay_ms = config.base_delay_ms * 2u64.pow(attempt);
    let delay_ms = delay_ms.min(config.max_delay_ms);
    Duration::from_millis(delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_default_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 500);
        assert!(config.retryable_status_codes.contains(&429));
        assert!(config.retryable_status_codes.contains(&529));
    }

    #[test]
    fn test_delay_calculation() {
        let config = RetryConfig::default();
        assert_eq!(calculate_delay(0, &config), Duration::from_millis(500));
        assert_eq!(calculate_delay(1, &config), Duration::from_millis(1000));
        assert_eq!(calculate_delay(2, &config), Duration::from_millis(2000));
    }

    #[test]
    fn test_delay_cap() {
        let config = RetryConfig {
            max_delay_ms: 1000,
            ..Default::default()
        };
        assert_eq!(calculate_delay(10, &config), Duration::from_millis(1000));
    }

    #[test]
    fn test_retryable_errors() {
        let config = RetryConfig::default();
        assert!(is_retryable(&AtomError::Api { message: "".into(), status: Some(429) }, &config));
        assert!(is_retryable(&AtomError::Api { message: "".into(), status: Some(529) }, &config));
        assert!(!is_retryable(&AtomError::Api { message: "".into(), status: Some(400) }, &config));
        assert!(!is_retryable(&AtomError::PermissionDenied { tool_name: "".into() }, &config));
    }

    #[tokio::test]
    async fn test_with_retry_success() {
        let config = RetryConfig { max_retries: 3, base_delay_ms: 1, ..Default::default() };
        let count = Arc::new(AtomicU32::new(0));
        let count_clone = count.clone();

        let result = with_retry(&config, || {
            let c = count_clone.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(AtomError::Api { message: "fail".into(), status: Some(429) })
                } else {
                    Ok(42)
                }
            }
        }).await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_exhausted() {
        let config = RetryConfig { max_retries: 2, base_delay_ms: 1, ..Default::default() };

        let result: Result<i32> = with_retry(&config, || async {
            Err(AtomError::Api { message: "always fail".into(), status: Some(429) })
        }).await;

        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Update mod.rs**

Add `pub mod retry;` to `src/services/api/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code services::api::retry`
Expected: 6 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/services/api/retry.rs src/services/api/mod.rs
git commit -m "feat: implement retry logic with exponential backoff"
```

---

### Task 5: Full test suite verification

- [ ] **Step 1: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass (~50+ tests total)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 3: Commit if needed**

```bash
git add -A
git commit -m "chore: phase 2 complete — LLM provider with OpenAI-compatible streaming"
```
