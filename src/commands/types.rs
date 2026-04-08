use crate::services::compact::types::CompactResult;
use serde::{Deserialize, Serialize};

/// Auth/provider environments a command is available in.
///
/// Separate from `is_enabled`: availability is a static auth requirement,
/// while `is_enabled` reflects runtime feature flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandAvailability {
    /// claude.ai OAuth subscriber (Pro/Max/Team/Enterprise)
    ClaudeAi,
    /// Console API key user (direct api.anthropic.com)
    Console,
}

/// Where the command definition originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandSource {
    Builtin,
    Mcp,
    Plugin,
    Bundled,
    UserSettings,
    ProjectSettings,
    LocalSettings,
    FlagSettings,
    PolicySettings,
}

/// How the command was loaded at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandLoadedFrom {
    #[serde(rename = "commands_DEPRECATED")]
    CommandsDeprecated,
    #[serde(rename = "skills")]
    Skills,
    #[serde(rename = "plugin")]
    Plugin,
    #[serde(rename = "managed")]
    Managed,
    #[serde(rename = "bundled")]
    Bundled,
    #[serde(rename = "mcp")]
    Mcp,
}

/// Distinguishes workflow-backed commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandKind {
    Workflow,
}

/// Execution context for prompt commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextMode {
    /// Skill content expands into the current conversation (default).
    Inline,
    /// Skill runs in a sub-agent with separate context and token budget.
    Fork,
}

impl Default for ContextMode {
    fn default() -> Self {
        Self::Inline
    }
}

/// How to display a command result in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandResultDisplay {
    /// Don't display the result.
    Skip,
    /// Display as a system message.
    System,
    /// Display as a user-visible message (default).
    User,
}

impl Default for CommandResultDisplay {
    fn default() -> Self {
        Self::User
    }
}

/// Result of a local command execution.
#[derive(Debug)]
pub enum LocalCommandResult {
    /// Text output to display.
    Text { value: String },
    /// Compact output with full result data.
    Compact { result: CompactResult },
    /// Conversation cleared with no summary.
    Clear,
    /// No output to display.
    Skip,
}

/// Result data when a command completes via `Done`.
#[derive(Debug, Clone, Default)]
pub struct CommandDoneResult {
    /// Optional user-visible text to display.
    pub text: Option<String>,
    /// How to display the result.
    pub display: CommandResultDisplay,
    /// If true, send messages to the model after command completes.
    pub should_query: bool,
    /// Additional messages to insert as meta (model-visible but hidden from UI).
    pub meta_messages: Vec<String>,
    /// Input to set in the prompt after command completes.
    pub next_input: Option<String>,
    /// Whether to auto-submit the next input.
    pub submit_next_input: bool,
}

/// What the command handler wants the REPL to do next.
#[derive(Debug, Clone)]
pub enum CommandAction {
    /// Continue the REPL loop normally.
    Continue,
    /// Command is done, with optional result data.
    Done(Option<CommandDoneResult>),
    /// Set the input buffer to this value.
    SetInput(String),
}

/// Plugin metadata for plugin-sourced commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub plugin_name: String,
    pub repository: String,
}

/// Static metadata describing a command (slash command or skill).
///
/// Uses `&'static str` for fields that are compile-time constants in builtin
/// commands. Dynamic commands (plugins, MCP) construct owned `String` values
/// via the builder or by converting at registration time.
#[derive(Debug, Clone)]
pub struct CommandMeta {
    /// The primary command name (without leading `/`).
    pub name: &'static str,
    /// Human-readable description.
    pub description: &'static str,
    /// Alternative names the command can be invoked with.
    pub aliases: &'static [&'static str],
    /// Whether to hide from typeahead/help.
    pub is_hidden: bool,
    /// If true, command executes immediately without waiting for a stop point.
    pub immediate: bool,
    /// Hint text for command arguments (displayed in gray after command name).
    pub argument_hint: Option<&'static str>,
    /// Auth/provider requirements. Empty means available everywhere.
    pub availability: &'static [CommandAvailability],
    /// Where the command definition originated.
    pub source: CommandSource,
    /// How the command was loaded at runtime.
    pub loaded_from: Option<CommandLoadedFrom>,
    /// Detailed usage scenarios for when to use this command (for model context).
    pub when_to_use: Option<&'static str>,
    /// If true, arguments are redacted from conversation history.
    pub is_sensitive: bool,
    /// Runtime enablement check result. Defaults to true.
    pub is_enabled: bool,
    /// Whether users can invoke this skill by typing `/skill-name`.
    pub user_invocable: Option<bool>,
    /// Whether to disable this command from being invoked by models.
    pub disable_model_invocation: bool,
    /// Distinguishes workflow-backed commands.
    pub kind: Option<CommandKind>,
    /// Version of the command/skill.
    pub version: Option<&'static str>,
    /// Whether the description was explicitly set (vs auto-derived).
    pub has_user_specified_description: bool,
    /// Whether this command comes from MCP.
    pub is_mcp: bool,
    /// Plugin metadata, if this command was loaded from a plugin.
    pub plugin_info: Option<PluginInfo>,
    /// User-facing display name (defaults to `name` if not set).
    pub user_facing_name: Option<&'static str>,
}

impl Default for CommandMeta {
    fn default() -> Self {
        Self {
            name: "",
            description: "",
            aliases: &[],
            is_hidden: false,
            immediate: false,
            argument_hint: None,
            availability: &[],
            source: CommandSource::Builtin,
            loaded_from: None,
            when_to_use: None,
            is_sensitive: false,
            is_enabled: true,
            user_invocable: None,
            disable_model_invocation: false,
            kind: None,
            version: None,
            has_user_specified_description: false,
            is_mcp: false,
            plugin_info: None,
            user_facing_name: None,
        }
    }
}

/// Returns the user-visible name for a command, falling back to `meta.name`.
pub fn get_command_name(meta: &CommandMeta) -> &str {
    meta.user_facing_name.unwrap_or(meta.name)
}

/// Returns whether the command is enabled. Defaults to true.
pub fn is_command_enabled(meta: &CommandMeta) -> bool {
    meta.is_enabled
}
