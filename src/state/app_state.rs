use crate::types::permissions::ToolPermissionContext;
use crate::utils::abort_signal::AbortSignal;
use crate::utils::settings::SettingsJson;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Agent configuration (user-settable).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default)]
    pub builtin_models: crate::services::agents::BuiltinModels,
}

/// Background task (shell, agent) — runtime only, not serialized
#[derive(Debug)]
pub struct BackgroundTask {
    pub id: String,
    pub task_type: String,
    pub command: Option<String>,
    pub status: BackgroundTaskStatus,
    pub cancel_token: AbortSignal,
    pub output: Arc<Mutex<String>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Clone for BackgroundTask {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            task_type: self.task_type.clone(),
            command: self.command.clone(),
            status: self.status.clone(),
            cancel_token: self.cancel_token.clone(),
            output: self.output.clone(),
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
}

/// A completed background agent result pending injection into next query
#[derive(Debug, Clone)]
pub struct PendingAgentResult {
    pub agent_id: String,
    pub description: String,
    pub result: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    // Session
    pub settings: SettingsJson,
    pub session_id: String,
    #[serde(default)]
    pub verbose: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_loop_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_loop_model_for_session: Option<String>,

    // Messages (core — the entire query engine depends on this)
    #[serde(default)]
    pub messages: Vec<crate::types::message::Message>,

    // UI state
    #[serde(default = "default_expanded_view")]
    pub expanded_view: ExpandedView,
    #[serde(default)]
    pub is_brief_only: bool,
    #[serde(default)]
    pub selected_ip_agent_index: i32,
    #[serde(default)]
    pub coordinator_task_index: i32,
    #[serde(default)]
    pub view_selection_mode: ViewSelectionMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer_selection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_line_text: Option<String>,

    // Permissions
    #[serde(default)]
    pub tool_permission_context: ToolPermissionContext,

    // Feature flags
    #[serde(default)]
    pub kairos_enabled: bool,
    #[serde(default)]
    pub fast_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_enabled: Option<bool>,
    #[serde(default)]
    pub prompt_suggestion_enabled: bool,

    // Remote/Bridge
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_session_url: Option<String>,
    #[serde(default = "default_connection_status")]
    pub remote_connection_status: ConnectionStatus,
    #[serde(default)]
    pub remote_background_task_count: u32,
    #[serde(default)]
    pub repl_bridge_enabled: bool,
    #[serde(default)]
    pub repl_bridge_explicit: bool,
    #[serde(default)]
    pub repl_bridge_outbound_only: bool,
    #[serde(default)]
    pub repl_bridge_connected: bool,
    #[serde(default)]
    pub repl_bridge_session_active: bool,
    #[serde(default)]
    pub repl_bridge_reconnecting: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repl_bridge_error: Option<String>,
    #[serde(default)]
    pub show_remote_callout: bool,

    // Tasks & Agents
    #[serde(default)]
    pub tasks: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub agent_name_registry: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground_task_id: Option<String>,
    /// Background tasks — runtime only, not serialized
    #[serde(skip)]
    #[serde(default)]
    pub background_tasks: HashMap<String, BackgroundTask>,
    /// Completed background agent results waiting to be injected into next query
    #[serde(skip)]
    #[serde(default)]
    pub pending_agent_results: Vec<PendingAgentResult>,
    /// V1 legacy todo list
    #[serde(default)]
    pub todos: Vec<TodoItem>,
    /// Agent configuration (builtin model overrides, etc.)
    #[serde(default)]
    pub agent_config: AgentConfig,
    /// Agent registry — runtime only, not serialized
    #[serde(skip)]
    pub agent_registry: crate::services::agents::AgentRegistry,

    // Speculation
    #[serde(default)]
    pub speculation_session_time_saved_ms: u64,

    // Auth
    #[serde(default)]
    pub auth_version: u32,

    // Cost tracking
    #[serde(default)]
    pub session_cost: f64,
    #[serde(default)]
    pub token_usage: TokenUsage,

    // Agents
    #[serde(default)]
    pub coordinator_mode: bool,
    #[serde(default)]
    pub swarm_agents: Vec<serde_json::Value>,
    #[serde(default)]
    pub agent_message_queues: HashMap<String, Vec<serde_json::Value>>,
    /// Active worktree session (set by EnterWorktree, cleared by ExitWorktree)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_worktree_session: Option<crate::services::worktree::WorktreeSession>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_json_schema: Option<serde_json::Value>,

    // Advisor
    // Cron
    #[serde(skip)]
    pub cron_store: Option<crate::services::cron::CronStore>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisor_model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_message: Option<serde_json::Value>,

    #[serde(default)]
    pub active_overlays: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExpandedView {
    #[default]
    None,
    Tasks,
    Teammates,
}

fn default_expanded_view() -> ExpandedView {
    ExpandedView::None
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ViewSelectionMode {
    #[default]
    None,
    SelectingAgent,
    ViewingAgent,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Reconnecting,
    #[default]
    Disconnected,
}

fn default_connection_status() -> ConnectionStatus {
    ConnectionStatus::Disconnected
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            settings: SettingsJson::default(),
            session_id: uuid::Uuid::new_v4().to_string(),
            verbose: false,
            main_loop_model: None,
            main_loop_model_for_session: None,
            messages: Vec::new(),
            expanded_view: ExpandedView::None,
            is_brief_only: false,
            selected_ip_agent_index: 0,
            coordinator_task_index: 0,
            view_selection_mode: ViewSelectionMode::None,
            footer_selection: None,
            status_line_text: None,
            tool_permission_context: ToolPermissionContext::default(),
            kairos_enabled: false,
            fast_mode: false,
            thinking_enabled: None,
            prompt_suggestion_enabled: false,
            remote_session_url: None,
            remote_connection_status: ConnectionStatus::Disconnected,
            remote_background_task_count: 0,
            repl_bridge_enabled: false,
            repl_bridge_explicit: false,
            repl_bridge_outbound_only: false,
            repl_bridge_connected: false,
            repl_bridge_session_active: false,
            repl_bridge_reconnecting: false,
            repl_bridge_error: None,
            show_remote_callout: false,
            tasks: HashMap::new(),
            agent_name_registry: HashMap::new(),
            foreground_task_id: None,
            background_tasks: HashMap::new(),
            pending_agent_results: Vec::new(),
            todos: Vec::new(),
            speculation_session_time_saved_ms: 0,
            auth_version: 0,
            session_cost: 0.0,
            token_usage: TokenUsage::default(),
            coordinator_mode: false,
            swarm_agents: Vec::new(),
            agent_message_queues: HashMap::new(),
            current_worktree_session: None,
            team_name: None,
            output_json_schema: None,
            cron_store: None,
            advisor_model: None,
            initial_message: None,
            active_overlays: Vec::new(),
            agent_config: AgentConfig::default(),
            agent_registry: {
                let builtin_models = &AgentConfig::default().builtin_models;
                crate::services::agents::AgentRegistry::with_builtins(
                    crate::services::agents::builtin_agents(builtin_models),
                )
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert_eq!(state.expanded_view, ExpandedView::None);
        assert!(!state.kairos_enabled);
        assert!(!state.fast_mode);
        assert!(state.tasks.is_empty());
    }

    #[test]
    fn test_app_state_serde_roundtrip() {
        let state = AppState {
            main_loop_model: Some("test-model".into()),
            fast_mode: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: AppState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.main_loop_model, state.main_loop_model);
        assert_eq!(parsed.fast_mode, state.fast_mode);
    }

    #[test]
    fn test_expanded_view_serde() {
        assert_eq!(
            serde_json::to_string(&ExpandedView::Tasks).unwrap(),
            "\"tasks\""
        );
        assert_eq!(
            serde_json::to_string(&ExpandedView::None).unwrap(),
            "\"none\""
        );
    }

    #[test]
    fn test_connection_status_serde() {
        assert_eq!(
            serde_json::to_string(&ConnectionStatus::Connected).unwrap(),
            "\"connected\""
        );
    }
}
