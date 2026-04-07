use async_trait::async_trait;
use chrono::Utc;

use crate::services::coordinator::is_agent_swarms_enabled;
use crate::services::mailbox::types::MailboxMessage;
use crate::services::mailbox::Mailbox;
use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::AtomError;

pub struct SendMessageTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "SendMessage".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "SendMessage"
    }

    fn description(&self) -> &str {
        "Send a message to an agent teammate, broadcast to all, or route via UDS/bridge."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Agent name, '*' for broadcast, 'uds:<path>', or 'bridge:<session-id>'"
                },
                "message": {
                    "description": "Plain text string or structured message object"
                },
                "summary": {
                    "type": "string",
                    "description": "5-10 word preview (required for plain text messages)"
                }
            },
            "required": ["to", "message"]
        })
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        if is_agent_swarms_enabled() {
            return ValidationResult::Ok;
        }
        // Check swarm_agents via try_read (sync context)
        if let Ok(state) = ctx.app_state.try_read() {
            if !state.swarm_agents.is_empty() {
                return ValidationResult::Ok;
            }
        }
        ValidationResult::Disabled("Agent swarms are not enabled".into())
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        // --- Parse & validate inputs ---
        let to = ctx
            .input
            .get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required parameter: 'to'"))?;

        if to.is_empty() {
            return Err(tool_err("'to' must not be empty"));
        }
        if to.contains('@') {
            return Err(tool_err("'to' must not contain '@'"));
        }

        let message_val = ctx
            .input
            .get("message")
            .ok_or_else(|| tool_err("Missing required parameter: 'message'"))?;

        let summary = ctx.input.get("summary").and_then(|v| v.as_str());

        // Determine if message is plain text or structured
        let is_plain = message_val.is_string();
        let is_structured = message_val.is_object()
            && message_val.get("type").and_then(|v| v.as_str()).is_some();

        if is_plain && summary.is_none() {
            return Err(tool_err(
                "'summary' is required when 'message' is a plain string",
            ));
        }

        if is_structured && to == "*" {
            return Err(tool_err("Structured messages cannot be broadcast"));
        }

        if is_structured && (to.starts_with("uds:") || to.starts_with("bridge:")) {
            return Err(tool_err("Structured messages cannot cross sessions"));
        }

        // Get sender name from app state
        let state = ctx.app_state.read().await;
        let sender = state.session_id.clone();
        let team_name = state.team_name.clone().unwrap_or_else(|| "default".into());
        drop(state);

        // --- Route the message ---

        // Path 1: Broadcast
        if to == "*" {
            let plain_text = message_val.as_str().unwrap().to_string();
            let state = ctx.app_state.read().await;
            let agents: Vec<String> = state.agent_name_registry.keys().cloned().collect();
            drop(state);

            let mut recipients = Vec::new();

            for agent in &agents {
                let msg = MailboxMessage {
                    from: sender.clone(),
                    text: plain_text.clone(),
                    summary: summary.map(|s| s.to_string()),
                    timestamp: Utc::now(),
                };
                let agent_owned = agent.clone();
                let mb_team = team_name.clone();
                let msg_clone = msg;
                tokio::task::spawn_blocking(move || {
                    let mb = Mailbox::new(&mb_team);
                    mb.write(&agent_owned, msg_clone)
                })
                .await
                .map_err(|e| tool_err(format!("Join error: {}", e)))?
                .map_err(|e| tool_err(format!("Mailbox write error: {}", e)))?;
                recipients.push(agent.clone());
            }

            let output = serde_json::json!({
                "success": true,
                "message": format!("Broadcast sent to {} agent(s)", recipients.len()),
                "recipients": recipients,
            });
            return Ok(ToolResult {
                content: serde_json::to_string(&output).unwrap(),
                is_error: false,
                content_blocks: None,
            });
        }

        // Path 2: UDS
        if let Some(path) = to.strip_prefix("uds:") {
            let plain_text = message_val
                .as_str()
                .ok_or_else(|| tool_err("UDS routing requires a plain text message"))?;

            let path_owned = path.to_string();
            let payload = serde_json::json!({
                "from": sender,
                "text": plain_text,
                "summary": summary,
            })
            .to_string();

            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                use std::io::Write;
                let stream = std::os::unix::net::UnixStream::connect(&path_owned)?;
                let mut writer = std::io::BufWriter::new(stream);
                writer.write_all(payload.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
                Ok(())
            })
            .await
            .map_err(|e| tool_err(format!("Join error: {}", e)))?
            .map_err(|e| tool_err(format!("UDS send error: {}", e)))?;

            let output = serde_json::json!({
                "success": true,
                "message": format!("Message sent via UDS to {}", to),
                "routing": {
                    "sender": sender,
                    "target": to,
                    "summary": summary,
                    "content": plain_text,
                }
            });
            return Ok(ToolResult {
                content: serde_json::to_string(&output).unwrap(),
                is_error: false,
                content_blocks: None,
            });
        }

        // Path 3: Bridge
        if let Some(session_id) = to.strip_prefix("bridge:") {
            let plain_text = message_val
                .as_str()
                .ok_or_else(|| tool_err("Bridge routing requires a plain text message"))?;

            let url = format!(
                "http://localhost:3000/bridge/sessions/{}/messages",
                session_id
            );
            let payload = serde_json::json!({
                "from": sender,
                "text": plain_text,
                "summary": summary,
            });

            let client = reqwest::Client::new();
            client
                .post(&url)
                .json(&payload)
                .send()
                .await
                .map_err(|e| tool_err(format!("Bridge HTTP error: {}", e)))?;

            let output = serde_json::json!({
                "success": true,
                "message": format!("Message sent via bridge to {}", to),
                "routing": {
                    "sender": sender,
                    "target": to,
                    "summary": summary,
                    "content": plain_text,
                }
            });
            return Ok(ToolResult {
                content: serde_json::to_string(&output).unwrap(),
                is_error: false,
                content_blocks: None,
            });
        }

        // Path 4: Structured message
        if is_structured {
            let msg_type = message_val
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            match msg_type {
                "shutdown_request" => {
                    let request_id = uuid::Uuid::new_v4().to_string();
                    let output = serde_json::json!({
                        "success": true,
                        "message": "Shutdown request sent",
                        "request_id": request_id,
                        "target": to,
                    });
                    return Ok(ToolResult {
                        content: serde_json::to_string(&output).unwrap(),
                        is_error: false,
                        content_blocks: None,
                    });
                }
                "shutdown_response" => {
                    let request_id = message_val
                        .get("request_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let approve = message_val
                        .get("approve")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    if approve {
                        // Cancel the target agent via background_tasks
                        let state = ctx.app_state.read().await;
                        if let Some(task) = state.background_tasks.get(to) {
                            task.cancel_token.cancel(crate::utils::abort_signal::AbortReason::UserCancel);
                        }
                    }

                    let output = serde_json::json!({
                        "success": true,
                        "message": if approve { "Shutdown approved" } else { "Shutdown rejected" },
                        "request_id": request_id,
                    });
                    return Ok(ToolResult {
                        content: serde_json::to_string(&output).unwrap(),
                        is_error: false,
                        content_blocks: None,
                    });
                }
                "plan_approval_response" => {
                    let request_id = message_val
                        .get("request_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let approve = message_val
                        .get("approve")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let output = serde_json::json!({
                        "success": true,
                        "message": if approve { "Plan approved" } else { "Plan rejected" },
                        "request_id": request_id,
                    });
                    return Ok(ToolResult {
                        content: serde_json::to_string(&output).unwrap(),
                        is_error: false,
                        content_blocks: None,
                    });
                }
                _ => {
                    return Err(tool_err(format!(
                        "Unknown structured message type: '{}'",
                        msg_type
                    )));
                }
            }
        }

        // Path 5: Agent message (default) — queue in memory, fall back to file Mailbox
        let plain_text = message_val
            .as_str()
            .ok_or_else(|| tool_err("Default agent routing requires a plain text message"))?
            .to_string();

        let state = ctx.app_state.read().await;
        let agent_known = state.agent_name_registry.contains_key(to);
        drop(state);

        if agent_known {
            // Queue in agent_message_queues
            let mut state = ctx.app_state.write().await;
            let queue = state
                .agent_message_queues
                .entry(to.to_string())
                .or_default();
            queue.push(serde_json::json!({
                "from": sender,
                "text": plain_text,
                "summary": summary,
                "timestamp": Utc::now().to_rfc3339(),
            }));
        } else {
            // Fall back to file-based Mailbox
            let msg = MailboxMessage {
                from: sender.clone(),
                text: plain_text.clone(),
                summary: summary.map(|s| s.to_string()),
                timestamp: Utc::now(),
            };
            let to_owned = to.to_string();
            let team = team_name.clone();
            tokio::task::spawn_blocking(move || {
                let mb = Mailbox::new(&team);
                mb.write(&to_owned, msg)
            })
            .await
            .map_err(|e| tool_err(format!("Join error: {}", e)))?
            .map_err(|e| tool_err(format!("Mailbox write error: {}", e)))?;
        }

        let output = serde_json::json!({
            "success": true,
            "message": format!("Message sent to {}", to),
            "routing": {
                "sender": sender,
                "target": to,
                "summary": summary,
                "content": plain_text,
            }
        });
        Ok(ToolResult {
            content: serde_json::to_string(&output).unwrap(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn should_defer(&self) -> bool {
        true
    }

    fn search_hint(&self) -> Option<&str> {
        Some("send messages to agent teammates (swarm protocol)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;
    use std::sync::Arc;

    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        ToolUseContext::test(input)
    }

    #[test]
    fn test_name() {
        assert_eq!(SendMessageTool.name(), "SendMessage");
    }

    #[test]
    fn test_schema() {
        let schema = SendMessageTool.input_schema();
        let props = schema.get("properties").unwrap();
        assert!(props.get("to").is_some());
        assert!(props.get("message").is_some());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("to")));
        assert!(required.contains(&serde_json::json!("message")));
    }

    #[test]
    fn test_should_defer() {
        assert!(SendMessageTool.should_defer());
        assert!(!SendMessageTool.is_read_only());
    }

    #[tokio::test]
    async fn test_missing_to() {
        let ctx = make_ctx(serde_json::json!({"message": "hello", "summary": "test"}));
        let result = SendMessageTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("to"), "Error should mention 'to': {}", err);
    }

    #[tokio::test]
    async fn test_missing_message() {
        let ctx = make_ctx(serde_json::json!({"to": "agent1"}));
        let result = SendMessageTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("message"),
            "Error should mention 'message': {}",
            err
        );
    }

    #[tokio::test]
    async fn test_empty_to_rejected() {
        let ctx = make_ctx(serde_json::json!({"to": "", "message": "hello", "summary": "test"}));
        let result = SendMessageTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("empty"), "Error should mention 'empty': {}", err);
    }

    #[tokio::test]
    async fn test_plain_message_to_agent_in_registry() {
        // Set up AppState with an agent in the registry
        let app_state = crate::state::AppState {
            agent_name_registry: {
                let mut m = std::collections::HashMap::new();
                m.insert("worker-1".into(), "task-abc".into());
                m
            },
            ..Default::default()
        };
        let app_state = Arc::new(tokio::sync::RwLock::new(app_state));

        let mut ctx = make_ctx(serde_json::json!({
            "to": "worker-1",
            "message": "Please review the PR",
            "summary": "review PR request"
        }));
        ctx.app_state = app_state.clone();

        let result = SendMessageTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        // Verify the message was queued
        let state = app_state.read().await;
        let queue = state.agent_message_queues.get("worker-1").unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0]["text"], "Please review the PR");
    }
}
