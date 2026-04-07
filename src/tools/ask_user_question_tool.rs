use async_trait::async_trait;

use crate::query::types::{QuestionOption, UserQuestionAsk, UserQuestionReply};
use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct AskUserQuestionTool;

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &str {
        "AskUserQuestion"
    }

    fn description(&self) -> &str {
        "Ask the user a question. Can present multiple-choice options or accept free-text input."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "The question to ask" },
                "options": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": { "type": "string" },
                            "description": { "type": "string" }
                        },
                        "required": ["label"]
                    }
                }
            },
            "required": ["question"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let question = ctx
            .input
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "AskUserQuestion".into(),
                message: "Missing 'question'".into(),
            })?;

        let options: Vec<QuestionOption> = ctx
            .input
            .get("options")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        Some(QuestionOption {
                            label: item.get("label")?.as_str()?.to_string(),
                            description: item
                                .get("description")
                                .and_then(|d| d.as_str())
                                .map(String::from),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        ctx.question_tx
            .send(UserQuestionAsk {
                tool_use_id: ctx.tool_use_id.clone(),
                question: question.to_string(),
                options,
                reply_tx,
            })
            .map_err(|_| crate::types::AtomError::Tool {
                tool_name: "AskUserQuestion".into(),
                message: "UI channel closed".into(),
            })?;

        let reply = tokio::select! {
            reply = reply_rx => reply.unwrap_or(UserQuestionReply::Dismissed),
            _ = ctx.abort_signal.cancelled() => UserQuestionReply::Dismissed,
        };
        match reply {
            UserQuestionReply::Answer(a) => Ok(ToolResult {
                content: a,
                is_error: false,
                content_blocks: None,
            }),
            UserQuestionReply::Dismissed => Ok(ToolResult {
                content: "User dismissed".into(),
                is_error: true,
                content_blocks: None,
            }),
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(AskUserQuestionTool.name(), "AskUserQuestion");
    }

    #[test]
    fn test_schema() {
        let s = AskUserQuestionTool.input_schema();
        assert!(s["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("question")));
    }

    #[tokio::test]
    async fn test_missing_question() {
        let ctx = ToolUseContext::test(serde_json::json!({}));
        assert!(AskUserQuestionTool.execute(ctx).await.is_err());
    }
}
