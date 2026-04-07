use futures::StreamExt;

use crate::services::api::provider::LlmProvider;
use crate::services::api::types::{ContentDelta, CreateMessageRequest, StreamEvent};
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};

/// Replace `$ARGUMENTS` in a prompt template with the actual JSON arguments.
pub fn build_prompt_input(prompt_template: &str, arguments_json: &str) -> String {
    prompt_template.replace("$ARGUMENTS", arguments_json)
}

/// Send a single prompt to the LLM and collect the full response text.
pub async fn query_without_streaming(
    provider: &dyn LlmProvider,
    prompt: &str,
    model: Option<&str>,
    timeout_ms: u64,
) -> Result<String, String> {
    let model_id = model
        .map(|m| m.to_string())
        .unwrap_or_else(|| provider.model_info().id.clone());

    let request = CreateMessageRequest {
        model: model_id,
        messages: vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: prompt.to_string(),
            }],
        }],
        max_tokens: 4096,
        system: None,
        tools: None,
        temperature: None,
        stream: true,
    };

    let stream_result = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), async {
        let mut stream = provider
            .create_message(request)
            .await
            .map_err(|e| format!("LLM call failed: {e}"))?;

        let mut text = String::new();
        while let Some(event) = stream.next().await {
            match event {
                Ok(StreamEvent::ContentBlockDelta {
                    delta: ContentDelta::TextDelta { text: t },
                    ..
                }) => {
                    text.push_str(&t);
                }
                Ok(_) => {}
                Err(e) => return Err(format!("Stream error: {e}")),
            }
        }
        Ok(text)
    })
    .await;

    match stream_result {
        Ok(inner) => inner,
        Err(_) => Err("Prompt hook timed out".to_string()),
    }
}

/// Check the "ok" field in a parsed JSON value.
fn check_ok_field(v: &serde_json::Value) -> Result<(), String> {
    match v.get("ok").and_then(|v| v.as_bool()) {
        Some(true) => Ok(()),
        _ => {
            let reason = v
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("Blocked by prompt hook");
            Err(reason.to_string())
        }
    }
}

/// Parse a prompt hook response JSON. Expects `{"ok": true/false, "reason": "..."}`.
///
/// Handles LLM responses that include preamble text before the JSON:
/// 1. Tries parsing the full response as JSON
/// 2. Falls back to scanning each line for a JSON object starting with `{`
pub fn parse_prompt_response(response: &str) -> Result<(), String> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
        return check_ok_field(&v);
    }

    for line in response.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return check_ok_field(&v);
            }
        }
    }

    Err(format!(
        "Could not parse JSON from response: {}",
        &response[..response.len().min(200)]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_hook_input() {
        let template = "Review this tool call: $ARGUMENTS";
        let args = r#"{"tool":"Bash","command":"rm -rf /"}"#;
        let result = build_prompt_input(template, args);
        assert_eq!(
            result,
            r#"Review this tool call: {"tool":"Bash","command":"rm -rf /"}"#
        );
    }

    #[test]
    fn test_parse_prompt_hook_response_ok() {
        assert!(parse_prompt_response(r#"{"ok": true}"#).is_ok());
    }

    #[test]
    fn test_parse_prompt_hook_response_blocked() {
        let err = parse_prompt_response(r#"{"ok": false, "reason": "dangerous"}"#).unwrap_err();
        assert_eq!(err, "dangerous");
    }

    #[test]
    fn test_parse_prompt_response_with_preamble() {
        let response = "Here is my analysis:\n\n{\"ok\": false, \"reason\": \"unsafe command\"}\n";
        let err = parse_prompt_response(response).unwrap_err();
        assert_eq!(err, "unsafe command");
    }

    #[test]
    fn test_parse_prompt_response_preamble_ok() {
        let response = "I've reviewed the tool call.\n{\"ok\": true}\nDone.";
        assert!(parse_prompt_response(response).is_ok());
    }

    #[test]
    fn test_parse_prompt_response_no_json() {
        let err = parse_prompt_response("I'm not sure what to do here.").unwrap_err();
        assert!(err.starts_with("Could not parse JSON"));
    }

    #[test]
    fn test_parse_prompt_response_default_reason() {
        let err = parse_prompt_response(r#"{"ok": false}"#).unwrap_err();
        assert_eq!(err, "Blocked by prompt hook");
    }
}
