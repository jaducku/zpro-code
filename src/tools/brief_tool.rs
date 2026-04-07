use std::path::PathBuf;

use async_trait::async_trait;
use chrono::Utc;

use crate::query::types::{BriefAttachment, BriefStatus, QueryEvent};
use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::AtomError;

pub struct BriefTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "SendUserMessage".into(),
        message: msg.into(),
    }
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg"];

fn is_image_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[async_trait]
impl Tool for BriefTool {
    fn name(&self) -> &str {
        "SendUserMessage"
    }

    fn description(&self) -> &str {
        "Send a visible message to the user with optional file attachments."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Markdown message to display to the user"
                },
                "attachments": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional file paths to attach"
                },
                "status": {
                    "type": "string",
                    "enum": ["normal", "proactive"],
                    "description": "Message status: 'normal' or 'proactive'"
                }
            },
            "required": ["message", "status"]
        })
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        if let Ok(state) = ctx.app_state.try_read() {
            if state.is_brief_only || state.kairos_enabled {
                return ValidationResult::Ok;
            }
        }
        ValidationResult::Disabled("Brief mode or Kairos not enabled".into())
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        // Extract required fields
        let message = ctx
            .input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required parameter: 'message'"))?
            .to_string();

        let status_str = ctx
            .input
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required parameter: 'status'"))?;

        let status = match status_str {
            "normal" => BriefStatus::Normal,
            "proactive" => BriefStatus::Proactive,
            other => return Err(tool_err(format!(
                "Invalid status '{}': must be 'normal' or 'proactive'",
                other
            ))),
        };

        // Resolve attachments
        let raw_attachments: Vec<String> = ctx
            .input
            .get("attachments")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let mut resolved_attachments = Vec::new();
        let mut attachment_output = Vec::new();

        for raw_path in &raw_attachments {
            let path = PathBuf::from(raw_path);
            let abs_path = if path.is_absolute() {
                path
            } else {
                ctx.cwd.join(&path)
            };

            match tokio::fs::metadata(&abs_path).await {
                Ok(meta) if meta.is_file() => {
                    let is_image = is_image_extension(&abs_path);
                    resolved_attachments.push(BriefAttachment {
                        path: abs_path.clone(),
                        size: meta.len(),
                        is_image,
                    });
                    attachment_output.push(serde_json::json!({
                        "path": abs_path.to_string_lossy(),
                        "size": meta.len(),
                        "isImage": is_image,
                    }));
                }
                Ok(_) => {
                    // exists but not a file (directory, etc.)
                    attachment_output.push(serde_json::json!({
                        "path": abs_path.to_string_lossy(),
                        "error": "not a regular file",
                    }));
                }
                Err(e) => {
                    attachment_output.push(serde_json::json!({
                        "path": abs_path.to_string_lossy(),
                        "error": format!("{}", e),
                    }));
                }
            }
        }

        // Emit QueryEvent::BriefMessage
        let _ = ctx.event_tx.send(QueryEvent::BriefMessage {
            message: message.clone(),
            attachments: resolved_attachments,
            status,
        });

        // Return JSON output
        let output = serde_json::json!({
            "message": message,
            "attachments": attachment_output,
            "sentAt": Utc::now().to_rfc3339(),
        });

        Ok(ToolResult {
            content: serde_json::to_string(&output).unwrap(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn aliases(&self) -> &[&str] {
        &["Brief"]
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }

    fn always_load(&self) -> bool {
        true
    }

    fn search_hint(&self) -> Option<&str> {
        Some("send a visible message to the user")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;

    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        ToolUseContext::test(input)
    }

    #[test]
    fn test_name_and_alias() {
        assert_eq!(BriefTool.name(), "SendUserMessage");
        assert_eq!(BriefTool.aliases(), &["Brief"]);
    }

    #[test]
    fn test_schema() {
        let schema = BriefTool.input_schema();
        let props = schema.get("properties").unwrap();
        assert!(props.get("message").is_some());
        assert!(props.get("status").is_some());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("message")));
        assert!(required.contains(&serde_json::json!("status")));
    }

    #[test]
    fn test_always_load() {
        assert!(BriefTool.always_load());
        assert!(BriefTool.is_read_only());
        assert!(BriefTool.is_concurrent_safe());
    }

    #[tokio::test]
    async fn test_basic_message() {
        let ctx = make_ctx(serde_json::json!({
            "message": "Hello user!",
            "status": "normal"
        }));

        let result = BriefTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["message"], "Hello user!");
        assert!(parsed.get("sentAt").is_some());
    }

    #[tokio::test]
    async fn test_missing_message() {
        let ctx = make_ctx(serde_json::json!({
            "status": "normal"
        }));
        let result = BriefTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("message"), "Error should mention 'message': {}", err);
    }

    #[tokio::test]
    async fn test_missing_status() {
        let ctx = make_ctx(serde_json::json!({
            "message": "Hello"
        }));
        let result = BriefTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("status"), "Error should mention 'status': {}", err);
    }

    #[tokio::test]
    async fn test_invalid_status() {
        let ctx = make_ctx(serde_json::json!({
            "message": "Hello",
            "status": "invalid"
        }));
        let result = BriefTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid"), "Error should mention invalid status: {}", err);
    }

    #[tokio::test]
    async fn test_with_nonexistent_attachment() {
        let ctx = make_ctx(serde_json::json!({
            "message": "Check this file",
            "status": "normal",
            "attachments": ["/tmp/nonexistent_file_abc123.txt"]
        }));

        let result = BriefTool.execute(ctx).await.unwrap();
        assert!(!result.is_error, "Tool should succeed even with missing attachment");

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        let attachments = parsed["attachments"].as_array().unwrap();
        assert_eq!(attachments.len(), 1);
        assert!(attachments[0].get("error").is_some(), "Missing file should have error info");
    }
}
