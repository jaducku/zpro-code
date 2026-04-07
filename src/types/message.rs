use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::content::ContentBlock;

/// API-level message (for LLM communication)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMessage {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// Internal message types used throughout the application
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "user")]
    User(UserMessage),

    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),

    #[serde(rename = "system")]
    System(SystemMessage),

    #[serde(rename = "attachment")]
    Attachment(AttachmentMessage),

    #[serde(rename = "progress")]
    Progress(ProgressMessage),

    #[serde(rename = "grouped_tool_use")]
    GroupedToolUse(GroupedToolUseMessage),

    #[serde(rename = "collapsed_read_search")]
    CollapsedReadSearch(CollapsedReadSearchMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub uuid: Uuid,
    pub message: ApiMessage,
    #[serde(default)]
    pub is_meta: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub uuid: Uuid,
    pub message: ApiMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_agent_id: Option<String>,
    #[serde(default)]
    pub is_compact_summary: bool,
}

/// System messages with subtypes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype")]
pub enum SystemMessage {
    #[serde(rename = "local_command")]
    LocalCommand {
        uuid: Uuid,
        #[serde(flatten)]
        data: serde_json::Value,
    },
    #[serde(rename = "compact_boundary")]
    CompactBoundary { uuid: Uuid },
    #[serde(rename = "microcompact_boundary")]
    MicrocompactBoundary { uuid: Uuid },
    #[serde(rename = "api_error")]
    ApiError {
        uuid: Uuid,
        error_message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        status_code: Option<u16>,
    },
    #[serde(rename = "file_snapshot")]
    FileSnapshot {
        uuid: Uuid,
        #[serde(flatten)]
        data: serde_json::Value,
    },
    #[serde(rename = "agents_killed")]
    AgentsKilled { uuid: Uuid },
    #[serde(rename = "api_metrics")]
    ApiMetrics {
        uuid: Uuid,
        #[serde(flatten)]
        metrics: serde_json::Value,
    },
    #[serde(rename = "away_summary")]
    AwaySummary { uuid: Uuid, summary: String },
    #[serde(rename = "bridge_status")]
    BridgeStatus { uuid: Uuid, status: String },
    #[serde(rename = "informational")]
    Informational { uuid: Uuid, text: String },
    #[serde(rename = "memory_saved")]
    MemorySaved { uuid: Uuid, memory_path: String },
    #[serde(rename = "permission_retry")]
    PermissionRetry { uuid: Uuid },
    #[serde(rename = "scheduled_task_fire")]
    ScheduledTaskFire { uuid: Uuid, task_id: String },
    #[serde(rename = "stop_hook_summary")]
    StopHookSummary {
        uuid: Uuid,
        #[serde(flatten)]
        data: serde_json::Value,
    },
    #[serde(rename = "turn_duration")]
    TurnDuration { uuid: Uuid, duration_ms: u64 },
    #[serde(rename = "thinking")]
    Thinking { uuid: Uuid, thinking: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMessage {
    pub uuid: Uuid,
    pub attachment: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressMessage {
    pub uuid: Uuid,
    pub tool_use_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedToolUseMessage {
    pub uuid: Uuid,
    pub tool_name: String,
    pub messages: Vec<Message>,
    pub results: Vec<Message>,
    pub display_message: Box<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollapsedReadSearchMessage {
    pub uuid: Uuid,
    #[serde(default)]
    pub search_count: u32,
    #[serde(default)]
    pub read_count: u32,
    #[serde(default)]
    pub list_count: u32,
    #[serde(default)]
    pub repl_count: u32,
    #[serde(default)]
    pub memory_search_count: u32,
    #[serde(default)]
    pub memory_read_count: u32,
    #[serde(default)]
    pub memory_write_count: u32,
    pub read_file_paths: Vec<String>,
    pub search_args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_display_hint: Option<String>,
    pub messages: Vec<Message>,
    pub display_message: Box<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_call_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_server_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bash_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_total_ms: Option<u32>,
}

/// Stop reason from LLM provider (unified across providers)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_message_creation() {
        let msg = UserMessage {
            uuid: Uuid::new_v4(),
            message: ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "hello".into(),
                }],
            },
            is_meta: false,
            tool_use_result: None,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["message"]["role"], "user");
        assert_eq!(json["message"]["content"][0]["text"], "hello");
    }

    #[test]
    fn test_assistant_message_creation() {
        let msg = AssistantMessage {
            uuid: Uuid::new_v4(),
            message: ApiMessage {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Text {
                        text: "Let me help".into(),
                    },
                    ContentBlock::ToolUse {
                        id: "tu_1".into(),
                        name: "BashTool".into(),
                        input: serde_json::json!({"command": "ls"}),
                    },
                ],
            },
            source_agent_id: None,
            is_compact_summary: false,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["message"]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_role_serde() {
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
        assert_eq!(
            serde_json::to_string(&Role::Assistant).unwrap(),
            "\"assistant\""
        );
        assert_eq!(serde_json::to_string(&Role::Tool).unwrap(), "\"tool\"");
    }

    #[test]
    fn test_system_message_serde() {
        let sys_msg = SystemMessage::ApiError {
            uuid: Uuid::new_v4(),
            error_message: "rate limited".into(),
            status_code: Some(429),
        };
        // Wrap in Message enum to test nested tag serialization
        let msg = Message::System(sys_msg);
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "system");
        assert_eq!(json["subtype"], "api_error");
        assert_eq!(json["error_message"], "rate limited");
        assert_eq!(json["status_code"], 429);

        // Roundtrip
        let parsed: Message = serde_json::from_value(json).unwrap();
        match parsed {
            Message::System(SystemMessage::ApiError {
                error_message,
                status_code,
                ..
            }) => {
                assert_eq!(error_message, "rate limited");
                assert_eq!(status_code, Some(429));
            }
            _ => panic!("Expected System::ApiError"),
        }
    }

    #[test]
    fn test_system_informational_serde() {
        let msg = Message::System(SystemMessage::Informational {
            uuid: Uuid::new_v4(),
            text: "hello".into(),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "system");
        assert_eq!(json["subtype"], "informational");

        let parsed: Message = serde_json::from_value(json).unwrap();
        assert!(matches!(
            parsed,
            Message::System(SystemMessage::Informational { .. })
        ));
    }

    #[test]
    fn test_stop_reason() {
        let reasons = vec![
            StopReason::EndTurn,
            StopReason::ToolUse,
            StopReason::MaxTokens,
            StopReason::StopSequence,
        ];
        for reason in reasons {
            let json = serde_json::to_string(&reason).unwrap();
            let parsed: StopReason = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, reason);
        }
    }
}
