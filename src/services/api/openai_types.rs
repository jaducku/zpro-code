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
    /// OpenRouter reasoning config. When set, the provider parses model-specific
    /// thinking tags (Gemma4 <channel>, DeepSeek <think>, etc.) and returns
    /// normalized reasoning/reasoning_content fields instead of raw tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
}

/// OpenRouter reasoning configuration.
/// Empty object `{}` enables reasoning with defaults.
#[derive(Debug, Clone, Serialize)]
pub struct ReasoningConfig {
    /// Reasoning effort: "high", "medium", "low", "none"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
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
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub choices: Vec<OpenAiStreamChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAiUsage>,
    /// Error object returned by some providers (e.g., OpenRouter rate limits)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<OpenAiStreamError>,
}

/// Error object embedded in SSE stream chunks
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiStreamError {
    pub message: String,
    #[serde(default)]
    pub code: Option<serde_json::Value>,
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
    /// Reasoning/thinking content (used by Qwen 3, DeepSeek, etc.)
    /// Field name varies by provider: "reasoning_content" (DeepSeek/Qwen),
    /// "reasoning" (some OpenRouter models like Gemma4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    /// Alternate reasoning field used by some providers (e.g., OpenRouter for Gemma4).
    /// Merged with reasoning_content during processing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
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
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
            reasoning: None,
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
        assert_eq!(
            tc.function.as_ref().unwrap().name.as_deref(),
            Some("get_weather")
        );
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

    #[test]
    fn test_chunk_missing_id_uses_default() {
        let json = r#"{
            "choices": [{"index": 0, "delta": {"content": "hi"}, "finish_reason": null}]
        }"#;
        let chunk: OpenAiStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.id, "");
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("hi"));
    }

    #[test]
    fn test_chunk_with_error_object() {
        let json = r#"{
            "error": {"message": "Rate limit exceeded", "code": 429}
        }"#;
        let chunk: OpenAiStreamChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.error.is_some());
        assert_eq!(chunk.error.unwrap().message, "Rate limit exceeded");
    }

    #[test]
    fn test_chunk_with_reasoning_content() {
        let json = r#"{
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "delta": {"reasoning_content": "Let me think...", "content": null},
                "finish_reason": null
            }]
        }"#;
        let chunk: OpenAiStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(
            chunk.choices[0].delta.reasoning_content.as_deref(),
            Some("Let me think...")
        );
        assert!(chunk.choices[0].delta.content.is_none());
    }

    #[test]
    fn test_chunk_with_extra_fields_ignored() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1234567890,
            "model": "qwen-3",
            "system_fingerprint": "fp_abc",
            "choices": [{
                "index": 0,
                "delta": {"content": "works"},
                "finish_reason": null,
                "logprobs": null
            }]
        }"#;
        let chunk: OpenAiStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("works"));
    }
}
