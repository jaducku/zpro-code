use crate::services::api::provider::LlmProvider;
use crate::services::session_memory::SessionMemoryState;
use crate::state::AppState;
use crate::types::message::{ApiMessage, Message};
use crate::utils::abort_signal::AbortSignal;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Runtime context passed to every command when it executes.
pub struct CommandContext {
    pub app_state: Arc<RwLock<AppState>>,
    pub abort_signal: AbortSignal,
    pub cwd: PathBuf,
    pub message_tx: mpsc::UnboundedSender<MessageUpdate>,
    pub ui_event_tx: mpsc::UnboundedSender<UiEvent>,
    pub options: CommandOptions,
    pub provider: Option<Arc<dyn LlmProvider>>,
    pub messages: Arc<Vec<ApiMessage>>,
    pub session_memory: Option<Arc<SessionMemoryState>>,
}

/// Options that affect command behaviour (e.g. theming).
pub struct CommandOptions {
    pub theme: String,
}

/// A message-list mutation sent over the `message_tx` channel.
pub enum MessageUpdate {
    /// Apply a function that transforms the current message list.
    Set(Box<dyn FnOnce(Vec<Message>) -> Vec<Message> + Send>),
}

/// Events a command can send to the UI layer.
pub enum UiEvent {
    /// Prompt the user to change their API key.
    ChangeApiKey,
    /// Resume a previous session by id.
    ResumeSession { session_id: String },
}
