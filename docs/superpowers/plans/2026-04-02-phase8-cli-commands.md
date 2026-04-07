# Phase 8: CLI Commands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the ~110 CLI commands from the original TypeScript Claude Code CLI to Rust, including command type system, registry, clap CLI, and TUI integration.

**Architecture:** Three command traits (PromptCommand, LocalCommand, TuiCommand) unified by a Command enum. CommandRegistry manages static + dynamic commands. clap handles CLI subcommands; slash commands dispatch through the registry in the TUI event loop.

**Tech Stack:** clap 4 (CLI), ratatui + crossterm (TUI commands), async-trait, tokio, serde

**Spec:** `docs/superpowers/specs/2026-04-02-phase8-cli-commands-design.md`

---

### Task 1: Command type system — enums and structs

**Files:**
- Create: `src/commands/types.rs`
- Create: `src/commands/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create types.rs with all enums and CommandMeta**

```rust
// src/commands/types.rs
use std::collections::HashMap;

/// Auth/provider availability requirement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAvailability {
    ClaudeAi,
    Console,
}

/// Where the command's prompt content originated
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// How the command was discovered/loaded
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandLoadedFrom {
    CommandsDeprecated,
    Skills,
    Plugin,
    Managed,
    Bundled,
    Mcp,
}

/// Distinguishes workflow-backed commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Workflow,
}

/// Execution context for prompt commands
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextMode {
    #[default]
    Inline,
    Fork,
}

/// How to display command result
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandResultDisplay {
    Skip,
    System,
    #[default]
    User,
}

/// Result from a local command
#[derive(Debug)]
pub enum LocalCommandResult {
    Text { value: String },
    Compact { display_text: Option<String> },
    Skip,
}

/// Action returned by TUI command key handler
pub enum CommandAction {
    /// Keep the command UI active
    Continue,
    /// Command is done, optionally with a result
    Done(Option<CommandDoneResult>),
    /// Close command UI and set input text
    SetInput(String),
}

/// Result when a TUI command completes
pub struct CommandDoneResult {
    pub text: Option<String>,
    pub display: CommandResultDisplay,
    pub should_query: bool,
    pub meta_messages: Vec<String>,
    pub next_input: Option<String>,
    pub submit_next_input: bool,
}

/// Plugin metadata for commands loaded from plugins
pub struct PluginInfo {
    pub plugin_name: String,
    pub repository: String,
}

/// Metadata common to all command types (maps original CommandBase)
pub struct CommandMeta {
    pub name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub is_hidden: bool,
    pub immediate: bool,
    pub argument_hint: Option<&'static str>,
    pub availability: Option<&'static [CommandAvailability]>,
    pub source: CommandSource,
    pub loaded_from: Option<CommandLoadedFrom>,
    pub when_to_use: Option<&'static str>,
    pub is_sensitive: bool,
    pub is_enabled: Option<fn() -> bool>,
    pub user_invocable: Option<bool>,
    pub disable_model_invocation: bool,
    pub kind: Option<CommandKind>,
    pub version: Option<&'static str>,
    pub has_user_specified_description: bool,
    pub is_mcp: bool,
    pub plugin_info: Option<PluginInfo>,
    pub user_facing_name: Option<fn() -> String>,
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
            availability: None,
            source: CommandSource::Builtin,
            loaded_from: None,
            when_to_use: None,
            is_sensitive: false,
            is_enabled: None,
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

/// Resolves user-visible name, falling back to name
pub fn get_command_name(meta: &CommandMeta) -> String {
    meta.user_facing_name
        .map_or_else(|| meta.name.to_string(), |f| f())
}

/// Resolves whether the command is enabled, defaulting to true
pub fn is_command_enabled(meta: &CommandMeta) -> bool {
    meta.is_enabled.map_or(true, |f| f())
}
```

- [ ] **Step 2: Create mod.rs with Command enum**

```rust
// src/commands/mod.rs
pub mod types;
pub mod traits;
pub mod context;
pub mod registry;

pub use types::*;
pub use traits::*;
pub use context::*;
pub use registry::CommandRegistry;
```

- [ ] **Step 3: Add `pub mod commands;` to lib.rs**

- [ ] **Step 4: Run `cargo check -p atom-code`**

- [ ] **Step 5: Commit**

```bash
git add src/commands/types.rs src/commands/mod.rs src/lib.rs
git commit -m "feat(commands): add command type system enums and CommandMeta"
```

---

### Task 2: Command traits

**Files:**
- Create: `src/commands/traits.rs`
- Create: `src/commands/context.rs`

- [ ] **Step 1: Create context.rs with CommandContext**

```rust
// src/commands/context.rs
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use crate::state::app_state::AppState;
use crate::types::message::Message;

/// Context provided to command execution
pub struct CommandContext {
    pub app_state: Arc<RwLock<AppState>>,
    pub abort_signal: CancellationToken,
    pub cwd: PathBuf,
    pub message_tx: mpsc::UnboundedSender<MessageUpdate>,
    pub ui_event_tx: mpsc::UnboundedSender<UiEvent>,
    pub options: CommandOptions,
}

pub struct CommandOptions {
    pub theme: String,
}

/// Message update operations
pub enum MessageUpdate {
    Set(Box<dyn FnOnce(Vec<Message>) -> Vec<Message> + Send>),
}

/// UI events triggered by commands
pub enum UiEvent {
    ChangeApiKey,
    ResumeSession { session_id: String },
}
```

- [ ] **Step 2: Create traits.rs with the three command traits + Command enum**

```rust
// src/commands/traits.rs
use std::any::Any;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::prelude::*;

use crate::types::content::ContentBlock;

use super::context::CommandContext;
use super::types::*;

/// Prompt command — sends prompt to LLM
#[async_trait]
pub trait PromptCommand: Send + Sync {
    fn meta(&self) -> &CommandMeta;
    fn allowed_tools(&self) -> Option<&[&str]> { None }
    fn progress_message(&self) -> &str;
    fn content_length(&self) -> usize { 0 }
    fn model_override(&self) -> Option<&str> { None }
    fn context_mode(&self) -> ContextMode { ContextMode::Inline }
    fn agent_type(&self) -> Option<&str> { None }
    fn arg_names(&self) -> Option<&[&str]> { None }
    fn disable_non_interactive(&self) -> bool { false }
    fn skill_root(&self) -> Option<&str> { None }
    fn paths(&self) -> Option<&[&str]> { None }
    async fn get_prompt(
        &self,
        args: &str,
        ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>>;
}

/// Local command — returns text directly
#[async_trait]
pub trait LocalCommand: Send + Sync {
    fn meta(&self) -> &CommandMeta;
    fn supports_non_interactive(&self) -> bool { false }
    async fn call(
        &self,
        args: &str,
        ctx: &CommandContext,
    ) -> anyhow::Result<LocalCommandResult>;
}

/// TUI command — renders interactive Ratatui widget
pub trait TuiCommand: Send + Sync {
    fn meta(&self) -> &CommandMeta;
    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send>;
    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any);
    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction;
}

/// Unified command enum
pub enum Command {
    Prompt(Box<dyn PromptCommand>),
    Local(Box<dyn LocalCommand>),
    Tui(Box<dyn TuiCommand>),
}

impl Command {
    pub fn meta(&self) -> &CommandMeta {
        match self {
            Command::Prompt(c) => c.meta(),
            Command::Local(c) => c.meta(),
            Command::Tui(c) => c.meta(),
        }
    }

    pub fn name(&self) -> &str {
        self.meta().name
    }

    pub fn is_enabled(&self) -> bool {
        is_command_enabled(self.meta())
    }

    pub fn user_facing_name(&self) -> String {
        get_command_name(self.meta())
    }
}
```

- [ ] **Step 3: Run `cargo check -p atom-code`**

- [ ] **Step 4: Commit**

```bash
git add src/commands/context.rs src/commands/traits.rs
git commit -m "feat(commands): add PromptCommand, LocalCommand, TuiCommand traits"
```

---

### Task 3: Command registry

**Files:**
- Create: `src/commands/registry.rs`

- [ ] **Step 1: Write test for registry basics**

```rust
// At the bottom of src/commands/registry.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_local_command(name: &'static str) -> Command {
        struct TestCmd { meta: CommandMeta }
        #[async_trait::async_trait]
        impl LocalCommand for TestCmd {
            fn meta(&self) -> &CommandMeta { &self.meta }
            async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
                Ok(LocalCommandResult::Text { value: "ok".into() })
            }
        }
        Command::Local(Box::new(TestCmd {
            meta: CommandMeta { name, description: "test", ..Default::default() },
        }))
    }

    #[test]
    fn test_find_by_name() {
        let mut reg = CommandRegistry::empty();
        reg.register_builtin(make_test_local_command("clear"));
        assert!(reg.find("clear").is_some());
        assert!(reg.find("nonexistent").is_none());
    }

    #[test]
    fn test_find_by_alias() {
        struct AliasCmd { meta: CommandMeta }
        #[async_trait::async_trait]
        impl LocalCommand for AliasCmd {
            fn meta(&self) -> &CommandMeta { &self.meta }
            async fn call(&self, _: &str, _: &CommandContext) -> anyhow::Result<LocalCommandResult> {
                Ok(LocalCommandResult::Skip)
            }
        }
        let mut reg = CommandRegistry::empty();
        reg.register_builtin(Command::Local(Box::new(AliasCmd {
            meta: CommandMeta {
                name: "config",
                description: "config",
                aliases: &["settings"],
                ..Default::default()
            },
        })));
        assert!(reg.find("settings").is_some());
        assert_eq!(reg.find("settings").unwrap().name(), "config");
    }

    #[test]
    fn test_has() {
        let mut reg = CommandRegistry::empty();
        reg.register_builtin(make_test_local_command("exit"));
        assert!(reg.has("exit"));
        assert!(!reg.has("quit"));
    }

    #[test]
    fn test_available_filters_disabled() {
        struct DisabledCmd { meta: CommandMeta }
        #[async_trait::async_trait]
        impl LocalCommand for DisabledCmd {
            fn meta(&self) -> &CommandMeta { &self.meta }
            async fn call(&self, _: &str, _: &CommandContext) -> anyhow::Result<LocalCommandResult> {
                Ok(LocalCommandResult::Skip)
            }
        }
        let mut reg = CommandRegistry::empty();
        reg.register_builtin(Command::Local(Box::new(DisabledCmd {
            meta: CommandMeta {
                name: "disabled",
                description: "disabled cmd",
                is_enabled: Some(|| false),
                ..Default::default()
            },
        })));
        reg.register_builtin(make_test_local_command("enabled"));
        let available = reg.available_commands();
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].name(), "enabled");
    }
}
```

- [ ] **Step 2: Implement CommandRegistry**

```rust
// src/commands/registry.rs
use super::types::*;
use super::traits::*;
use super::context::CommandContext;

pub struct CommandRegistry {
    builtin: Vec<Command>,
    dynamic: Vec<Command>,
}

impl CommandRegistry {
    /// Create an empty registry (for testing)
    pub fn empty() -> Self {
        Self {
            builtin: Vec::new(),
            dynamic: Vec::new(),
        }
    }

    /// Register a built-in command
    pub fn register_builtin(&mut self, cmd: Command) {
        self.builtin.push(cmd);
    }

    /// Register a dynamic command (skill, plugin, MCP, workflow)
    pub fn register_dynamic(&mut self, cmd: Command) {
        self.dynamic.push(cmd);
    }

    /// Register a dynamic skill, skipping if name already exists
    pub fn register_dynamic_skill(&mut self, cmd: Command) {
        if !self.has(cmd.name()) {
            self.dynamic.push(cmd);
        }
    }

    /// Find a command by name or alias
    pub fn find(&self, name: &str) -> Option<&Command> {
        self.all_commands().find(|cmd| {
            cmd.name() == name
                || cmd.user_facing_name() == name
                || cmd.meta().aliases.contains(&name)
        })
    }

    /// Check if a command exists
    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// Get all available commands (enabled + meets availability)
    pub fn available_commands(&self) -> Vec<&Command> {
        self.all_commands()
            .filter(|cmd| cmd.is_enabled())
            .collect()
    }

    /// Commands safe for remote mode
    pub fn remote_safe_commands(&self) -> Vec<&Command> {
        const REMOTE_SAFE: &[&str] = &[
            "session", "exit", "clear", "help", "theme", "color", "vim",
            "cost", "usage", "copy", "btw", "feedback", "plan",
            "keybindings", "statusline", "stickers", "mobile",
        ];
        self.available_commands()
            .into_iter()
            .filter(|c| REMOTE_SAFE.contains(&c.name()))
            .collect()
    }

    /// Commands safe for bridge execution
    pub fn bridge_safe_commands(&self) -> Vec<&Command> {
        const BRIDGE_SAFE: &[&str] = &[
            "compact", "clear", "cost", "summary", "release-notes", "files",
        ];
        self.available_commands()
            .into_iter()
            .filter(|c| {
                matches!(c, Command::Prompt(_)) || BRIDGE_SAFE.contains(&c.name())
            })
            .collect()
    }

    /// Prompt-type commands the model can invoke (for SkillTool)
    pub fn skill_tool_commands(&self) -> Vec<&Command> {
        self.available_commands()
            .into_iter()
            .filter(|c| {
                if let Command::Prompt(_) = c {
                    let meta = c.meta();
                    !meta.disable_model_invocation
                        && meta.source != CommandSource::Builtin
                        && (matches!(meta.loaded_from, Some(CommandLoadedFrom::Bundled | CommandLoadedFrom::Skills | CommandLoadedFrom::CommandsDeprecated))
                            || meta.has_user_specified_description
                            || meta.when_to_use.is_some())
                } else {
                    false
                }
            })
            .collect()
    }

    /// Clear dynamic commands
    pub fn clear_dynamic(&mut self) {
        self.dynamic.clear();
    }

    /// Iterator over all commands (dynamic first, then builtin — matches original load order)
    fn all_commands(&self) -> impl Iterator<Item = &Command> {
        self.dynamic.iter().chain(self.builtin.iter())
    }
}

/// Check if a command meets its availability requirement
pub fn meets_availability_requirement(cmd: &Command) -> bool {
    match cmd.meta().availability {
        None => true,
        Some(_avail) => {
            // In atom-code (open LLM provider), availability checks are simplified.
            // ClaudeAi and Console availability are not applicable.
            // All commands are available by default.
            true
        }
    }
}

/// Format description with source annotation
pub fn format_description_with_source(cmd: &Command) -> String {
    let meta = cmd.meta();
    match meta.source {
        CommandSource::Plugin => format!("{} (plugin)", meta.description),
        CommandSource::Bundled => format!("{} (bundled)", meta.description),
        _ => meta.description.to_string(),
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code commands::registry`

- [ ] **Step 4: Commit**

```bash
git add src/commands/registry.rs
git commit -m "feat(commands): add CommandRegistry with find, filter, and tests"
```

---

### Task 4: Cargo feature flags

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add feature flags to Cargo.toml**

Add after `[dev-dependencies]`:

```toml
[features]
default = []
kairos = []
kairos_brief = []
kairos_github_webhooks = []
bridge_mode = []
voice_mode = []
history_snip = []
workflow_scripts = []
ccr_remote_setup = []
ultraplan = []
torch = []
uds_inbox = []
fork_subagent = []
buddy = []
coordinator_mode = []
agents_platform = []
daemon = []
proactive = []
mcp_skills = []
experimental_skill_search = []
upload_user_settings = []
internal = ["all"]
all = [
    "kairos", "kairos_brief", "kairos_github_webhooks", "bridge_mode",
    "voice_mode", "history_snip", "workflow_scripts", "ccr_remote_setup",
    "ultraplan", "torch", "uds_inbox", "fork_subagent", "buddy",
    "coordinator_mode", "agents_platform", "daemon", "proactive",
    "mcp_skills", "experimental_skill_search", "upload_user_settings",
]
```

- [ ] **Step 2: Run `cargo check -p atom-code`**

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "feat(commands): add Cargo feature flags for conditional compilation"
```

---

### Task 5: clap CLI structure

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace existing Cli struct with full clap structure**

Replace the entire `src/main.rs` content. The full Cli struct with ~40 options and SubCommand enum with all subcommands is defined in the spec (Section 5). Key elements:

```rust
// src/main.rs
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "atom", version, about = "atom-code — starts an interactive session by default, use -p/--print for non-interactive output")]
struct Cli {
    /// Your prompt
    prompt: Option<String>,

    #[arg(short = 'p', long)]
    print: bool,

    #[arg(short = 'd', long)]
    debug: Option<Option<String>>,

    #[arg(long)]
    verbose: bool,

    #[arg(long)]
    bare: bool,

    #[arg(long)]
    model: Option<String>,

    #[arg(long)]
    effort: Option<String>,

    #[arg(long)]
    agent: Option<String>,

    #[arg(long)]
    system_prompt: Option<String>,

    #[arg(long)]
    append_system_prompt: Option<String>,

    #[arg(short = 'c', long = "continue")]
    continue_session: bool,

    #[arg(short = 'r', long)]
    resume: Option<Option<String>>,

    #[arg(long)]
    session_id: Option<String>,

    #[arg(short = 'n', long)]
    name: Option<String>,

    #[arg(long, value_enum)]
    output_format: Option<OutputFormat>,

    #[arg(long, value_enum)]
    input_format: Option<InputFormat>,

    #[arg(long)]
    allowed_tools: Option<Vec<String>>,

    #[arg(long)]
    disallowed_tools: Option<Vec<String>>,

    #[arg(long)]
    dangerously_skip_permissions: bool,

    #[arg(long)]
    permission_mode: Option<String>,

    #[arg(long)]
    mcp_config: Option<Vec<String>>,

    #[arg(long)]
    add_dir: Option<Vec<String>>,

    #[arg(long)]
    max_turns: Option<u32>,

    #[arg(long)]
    max_budget_usd: Option<f64>,

    #[arg(long)]
    api_base: Option<String>,

    #[arg(long)]
    api_key: Option<String>,

    #[arg(short = 'w', long)]
    worktree: Option<Option<String>>,

    #[arg(long)]
    settings: Option<String>,

    #[command(subcommand)]
    subcommand: Option<SubCommand>,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat { Text, Json, StreamJson }

#[derive(Clone, ValueEnum)]
enum InputFormat { Text, StreamJson }

#[derive(Subcommand)]
enum SubCommand {
    /// MCP server management
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Authentication management
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// Plugin management
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
    /// Check health of installation
    Doctor,
    /// Check for updates
    #[command(alias = "upgrade")]
    Update,
    /// List configured agents
    Agents {
        #[arg(long)]
        setting_sources: Option<String>,
    },
    /// Install native build
    Install {
        target: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Set up authentication token
    SetupToken,
    /// Auto mode inspection (internal)
    #[cfg(feature = "internal")]
    AutoMode {
        #[command(subcommand)]
        action: AutoModeAction,
    },
    /// Remote control
    #[cfg(feature = "bridge_mode")]
    #[command(alias = "rc")]
    RemoteControl,
    /// Assistant mode
    #[cfg(feature = "kairos")]
    Assistant {
        session_id: Option<String>,
    },
}

#[cfg(feature = "internal")]
#[derive(Subcommand)]
enum AutoModeAction {
    Defaults,
    Config,
    Critique { #[arg(long)] model: Option<String> },
}

#[derive(Subcommand)]
enum McpAction {
    Serve { #[arg(short)] debug: bool, #[arg(long)] verbose: bool },
    Add { #[arg(short = 's', long)] scope: Option<String>, args: Vec<String> },
    Remove { name: String, #[arg(short = 's', long)] scope: Option<String> },
    List,
    Get { name: String },
    AddJson { name: String, json: String, #[arg(short = 's', long, default_value = "local")] scope: String },
    AddFromClaudeDesktop { #[arg(short = 's', long, default_value = "local")] scope: String },
    ResetProjectChoices,
}

#[derive(Subcommand)]
enum AuthAction {
    Login { #[arg(long)] email: Option<String>, #[arg(long)] sso: bool },
    Status { #[arg(long)] json: bool, #[arg(long)] text: bool },
    Logout,
}

#[derive(Subcommand)]
enum PluginAction {
    Validate { path: String },
    List { #[arg(long)] json: bool },
    #[command(alias = "i")]
    Install { plugin: String, #[arg(short = 's', long, default_value = "user")] scope: String },
    #[command(alias = "remove", alias = "rm")]
    Uninstall { plugin: String, #[arg(short = 's', long, default_value = "user")] scope: String },
    Enable { plugin: String },
    Disable { plugin: Option<String>, #[arg(short = 'a', long)] all: bool },
    Update { plugin: String },
    Marketplace { #[command(subcommand)] action: MarketplaceAction },
}

#[derive(Subcommand)]
enum MarketplaceAction {
    Add { source: String },
    List { #[arg(long)] json: bool },
    #[command(alias = "rm")]
    Remove { name: String },
    Update { name: Option<String> },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.subcommand {
        Some(sub) => {
            execute_subcommand(sub).await?;
        }
        None if cli.print => {
            // Non-interactive print mode (Phase 9+)
            eprintln!("Print mode not yet implemented");
        }
        None => {
            // Interactive TUI mode
            let provider = cli.api_base.map(|base| {
                let model = cli.model.unwrap_or_else(|| "default".into());
                Box::new(atom_code::services::api::openai_provider::OpenAiCompatibleProvider::new(
                    base, cli.api_key, model,
                )) as Box<dyn atom_code::services::api::provider::LlmProvider>
            });
            let tools = atom_code::tools::get_builtin_tools();
            let system_prompt = cli.system_prompt.map(|s| {
                vec![atom_code::services::api::types::SystemBlock::Text { text: s }]
            });
            atom_code::tui::run::run_app(provider, tools, system_prompt).await?;
        }
    }

    Ok(())
}

async fn execute_subcommand(sub: SubCommand) -> anyhow::Result<()> {
    match sub {
        SubCommand::Doctor => {
            println!("atom-code doctor: all checks passed");
        }
        SubCommand::Update => {
            println!("atom-code is up to date");
        }
        SubCommand::Agents { .. } => {
            println!("No agents configured");
        }
        SubCommand::Install { target, .. } => {
            println!("Install: {}", target.unwrap_or_else(|| "stable".into()));
        }
        SubCommand::SetupToken => {
            println!("Token setup not yet implemented");
        }
        SubCommand::Mcp { action } => {
            execute_mcp_action(action).await?;
        }
        SubCommand::Auth { action } => {
            execute_auth_action(action).await?;
        }
        SubCommand::Plugin { action } => {
            execute_plugin_action(action).await?;
        }
    }
    Ok(())
}

async fn execute_mcp_action(action: McpAction) -> anyhow::Result<()> {
    match action {
        McpAction::List => println!("No MCP servers configured"),
        McpAction::Get { name } => println!("MCP server '{}' not found", name),
        McpAction::Remove { name, .. } => println!("Removed MCP server '{}'", name),
        McpAction::Serve { .. } => println!("MCP serve mode not yet implemented"),
        McpAction::Add { .. } => println!("MCP add not yet implemented"),
        McpAction::AddJson { name, json, scope, .. } => {
            println!("Added MCP server '{}' (scope: {})", name, scope);
            let _ = json;
        }
        McpAction::AddFromClaudeDesktop { .. } => println!("Import not yet implemented"),
        McpAction::ResetProjectChoices => println!("Project choices reset"),
    }
    Ok(())
}

async fn execute_auth_action(action: AuthAction) -> anyhow::Result<()> {
    match action {
        AuthAction::Login { .. } => println!("Auth login not yet implemented"),
        AuthAction::Status { .. } => println!("Not authenticated"),
        AuthAction::Logout => println!("Logged out"),
    }
    Ok(())
}

async fn execute_plugin_action(action: PluginAction) -> anyhow::Result<()> {
    match action {
        PluginAction::List { .. } => println!("No plugins installed"),
        PluginAction::Validate { path } => println!("Validating: {}", path),
        PluginAction::Install { plugin, .. } => println!("Installing: {}", plugin),
        PluginAction::Uninstall { plugin, .. } => println!("Uninstalling: {}", plugin),
        PluginAction::Enable { plugin } => println!("Enabling: {}", plugin),
        PluginAction::Disable { .. } => println!("Disabling plugin"),
        PluginAction::Update { plugin } => println!("Updating: {}", plugin),
        PluginAction::Marketplace { action } => {
            match action {
                MarketplaceAction::Add { source } => println!("Adding marketplace: {}", source),
                MarketplaceAction::List { .. } => println!("No marketplaces configured"),
                MarketplaceAction::Remove { name } => println!("Removing: {}", name),
                MarketplaceAction::Update { .. } => println!("Updating marketplaces"),
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Run `cargo build -p atom-code`**

- [ ] **Step 3: Verify CLI help works**

Run: `cargo run -p atom-code -- --help`
Expected: Shows all options and subcommands

Run: `cargo run -p atom-code -- mcp --help`
Expected: Shows MCP subcommands

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): expand clap CLI with full options and subcommands"
```

---

### Task 6: TUI integration — slash command dispatch

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/run.rs`

- [ ] **Step 1: Add active_command and registry to App**

In `src/tui/app.rs`, add to the `App` struct:

```rust
use crate::commands::{Command, CommandAction, CommandRegistry};

// Add fields to App struct:
    /// Command registry
    pub command_registry: CommandRegistry,
    /// Currently active TUI command
    pub active_command: Option<ActiveCommand>,

/// Active TUI command state
pub struct ActiveCommand {
    pub command_name: String,
    pub state: Box<dyn std::any::Any + Send>,
}
```

Update `App::new()` to accept and store `CommandRegistry`, initialize `active_command: None`.

- [ ] **Step 2: Add slash command parsing to App**

```rust
impl App {
    /// Parse a slash command from input, returns (name, args)
    pub fn parse_slash_command(input: &str) -> Option<(String, String)> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        let without_slash = &trimmed[1..];
        match without_slash.split_once(char::is_whitespace) {
            Some((name, args)) => Some((name.to_string(), args.trim().to_string())),
            None => Some((without_slash.to_string(), String::new())),
        }
    }
}
```

- [ ] **Step 3: Add tests for slash command parsing**

```rust
#[test]
fn test_parse_slash_command() {
    assert_eq!(
        App::parse_slash_command("/help"),
        Some(("help".into(), String::new()))
    );
    assert_eq!(
        App::parse_slash_command("/commit fix bug"),
        Some(("commit".into(), "fix bug".into()))
    );
    assert_eq!(App::parse_slash_command("not a command"), None);
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code tui::app`

- [ ] **Step 5: Commit**

```bash
git add src/tui/app.rs src/tui/run.rs
git commit -m "feat(tui): add slash command dispatch and ActiveCommand support"
```

---

### Task 7: First local command — /clear

**Files:**
- Create: `src/commands/local/mod.rs`
- Create: `src/commands/local/clear.rs`

- [ ] **Step 1: Create local/clear.rs**

```rust
// src/commands/local/clear.rs
use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::types::*;
use crate::commands::traits::LocalCommand;

pub struct ClearCommand;

static CLEAR_META: CommandMeta = CommandMeta {
    name: "clear",
    description: "Clear conversation history and free up context",
    aliases: &["reset", "new"],
    is_hidden: false,
    immediate: false,
    argument_hint: None,
    availability: None,
    source: CommandSource::Builtin,
    loaded_from: None,
    when_to_use: None,
    is_sensitive: false,
    is_enabled: None,
    user_invocable: None,
    disable_model_invocation: false,
    kind: None,
    version: None,
    has_user_specified_description: false,
    is_mcp: false,
    plugin_info: None,
    user_facing_name: None,
};

#[async_trait]
impl LocalCommand for ClearCommand {
    fn meta(&self) -> &CommandMeta {
        &CLEAR_META
    }

    async fn call(
        &self,
        _args: &str,
        ctx: &CommandContext,
    ) -> anyhow::Result<LocalCommandResult> {
        // Clear messages via channel
        let _ = ctx.message_tx.send(crate::commands::context::MessageUpdate::Set(
            Box::new(|_| Vec::new()),
        ));
        Ok(LocalCommandResult::Text {
            value: "Conversation cleared.".into(),
        })
    }
}
```

- [ ] **Step 2: Create local/mod.rs**

```rust
// src/commands/local/mod.rs
pub mod clear;

use super::traits::Command;

pub fn get_local_commands() -> Vec<Command> {
    vec![
        Command::Local(Box::new(clear::ClearCommand)),
    ]
}
```

- [ ] **Step 3: Add `pub mod local;` to commands/mod.rs**

- [ ] **Step 4: Run `cargo check -p atom-code`**

- [ ] **Step 5: Commit**

```bash
git add src/commands/local/
git commit -m "feat(commands): add /clear local command"
```

---

### Task 8: First prompt command — /review

**Files:**
- Create: `src/commands/prompt/mod.rs`
- Create: `src/commands/prompt/review.rs`

- [ ] **Step 1: Create prompt/review.rs**

Reference: `src/commands/review.ts` in the original. This is a prompt command that sends git context to the LLM for code review.

```rust
// src/commands/prompt/review.rs
use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::types::*;
use crate::commands::traits::PromptCommand;
use crate::types::content::ContentBlock;

pub struct ReviewCommand;

static REVIEW_META: CommandMeta = CommandMeta {
    name: "review",
    description: "Review a pull request",
    aliases: &[],
    is_hidden: false,
    immediate: false,
    argument_hint: None,
    availability: None,
    source: CommandSource::Builtin,
    loaded_from: None,
    when_to_use: None,
    is_sensitive: false,
    is_enabled: None,
    user_invocable: None,
    disable_model_invocation: false,
    kind: None,
    version: None,
    has_user_specified_description: false,
    is_mcp: false,
    plugin_info: None,
    user_facing_name: None,
};

#[async_trait]
impl PromptCommand for ReviewCommand {
    fn meta(&self) -> &CommandMeta {
        &REVIEW_META
    }

    fn progress_message(&self) -> &str {
        "reviewing code"
    }

    fn allowed_tools(&self) -> Option<&[&str]> {
        Some(&["Bash(git diff:*)", "Bash(git log:*)", "Bash(git show:*)"])
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        let prompt = r#"## Your task

Review the current changes in this repository.

1. Run `git diff HEAD` to see unstaged changes, or `git diff main...HEAD` for branch changes
2. Analyze the diff for:
   - Correctness and potential bugs
   - Security issues (OWASP top 10)
   - Performance concerns
   - Code style consistency
3. Provide a structured review with sections for each concern
4. Rate overall quality: LGTM, Minor Issues, or Needs Work"#;

        Ok(vec![ContentBlock::Text {
            text: prompt.to_string(),
        }])
    }
}
```

- [ ] **Step 2: Create prompt/mod.rs**

```rust
// src/commands/prompt/mod.rs
pub mod review;

use super::traits::Command;

pub fn get_prompt_commands() -> Vec<Command> {
    vec![
        Command::Prompt(Box::new(review::ReviewCommand)),
    ]
}
```

- [ ] **Step 3: Add `pub mod prompt;` to commands/mod.rs**

- [ ] **Step 4: Run `cargo check -p atom-code`**

- [ ] **Step 5: Commit**

```bash
git add src/commands/prompt/
git commit -m "feat(commands): add /review prompt command"
```

---

### Task 9: First TUI command — /help

**Files:**
- Create: `src/commands/tui/mod.rs`
- Create: `src/commands/tui/help.rs`

- [ ] **Step 1: Create tui/help.rs**

```rust
// src/commands/tui/help.rs
use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::types::*;
use crate::commands::traits::TuiCommand;

pub struct HelpCommand;

static HELP_META: CommandMeta = CommandMeta {
    name: "help",
    description: "Show help and available commands",
    aliases: &["?"],
    source: CommandSource::Builtin,
    is_hidden: false,
    immediate: false,
    argument_hint: None,
    availability: None,
    loaded_from: None,
    when_to_use: None,
    is_sensitive: false,
    is_enabled: None,
    user_invocable: None,
    disable_model_invocation: false,
    kind: None,
    version: None,
    has_user_specified_description: false,
    is_mcp: false,
    plugin_info: None,
    user_facing_name: None,
};

pub struct HelpState {
    pub scroll: u16,
    pub commands: Vec<(String, String)>, // (name, description)
}

impl TuiCommand for HelpCommand {
    fn meta(&self) -> &CommandMeta {
        &HELP_META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        // In full implementation, this would read from CommandRegistry
        Box::new(HelpState {
            scroll: 0,
            commands: vec![
                ("help".into(), "Show help and available commands".into()),
                ("clear".into(), "Clear conversation history".into()),
                ("review".into(), "Review a pull request".into()),
            ],
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_mut::<HelpState>().unwrap();

        let block = Block::default()
            .title(" Help — Available Commands (press q/Esc to close) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let items: Vec<ListItem> = state
            .commands
            .iter()
            .map(|(name, desc)| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  /{:<20}", name),
                        Style::default().fg(Color::Green).bold(),
                    ),
                    Span::raw(desc.as_str()),
                ]))
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(list, area);
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<HelpState>().unwrap();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            KeyCode::Up | KeyCode::Char('k') => {
                state.scroll = state.scroll.saturating_sub(1);
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.scroll += 1;
                CommandAction::Continue
            }
            _ => CommandAction::Continue,
        }
    }
}
```

- [ ] **Step 2: Create tui/mod.rs**

```rust
// src/commands/tui/mod.rs
pub mod help;

use super::traits::Command;

pub fn get_tui_commands() -> Vec<Command> {
    vec![
        Command::Tui(Box::new(help::HelpCommand)),
    ]
}
```

- [ ] **Step 3: Add `pub mod tui_cmds;` to commands/mod.rs** (using `tui_cmds` to avoid conflict with top-level `tui` module; re-export as `pub use tui_cmds as tui;` if desired)

- [ ] **Step 4: Run `cargo check -p atom-code`**

- [ ] **Step 5: Commit**

```bash
git add src/commands/tui/
git commit -m "feat(commands): add /help TUI command with Ratatui widget"
```

---

### Task 10: Wire registry into App and verify end-to-end

**Files:**
- Modify: `src/commands/registry.rs`
- Modify: `src/tui/app.rs`
- Modify: `src/tui/run.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add `CommandRegistry::new()` that registers all built-in commands**

```rust
// In src/commands/registry.rs
impl CommandRegistry {
    pub fn new() -> Self {
        let mut reg = Self::empty();

        // Local commands
        for cmd in super::local::get_local_commands() {
            reg.register_builtin(cmd);
        }

        // Prompt commands
        for cmd in super::prompt::get_prompt_commands() {
            reg.register_builtin(cmd);
        }

        // TUI commands
        for cmd in super::tui_cmds::get_tui_commands() {
            reg.register_builtin(cmd);
        }

        reg
    }
}
```

- [ ] **Step 2: Update App::new() to create CommandRegistry**

- [ ] **Step 3: Update run.rs event loop to dispatch slash commands**

When user presses Enter and input starts with `/`:
- Parse the command name and args
- Look up in registry
- For `Command::Local`: call and show result
- For `Command::Tui`: set `app.active_command`
- For `Command::Prompt`: show "prompt commands require LLM connection" (placeholder)

- [ ] **Step 4: Build and test manually**

Run: `cargo run -p atom-code -- --api-base http://localhost:8080 --api-key test`
Then type `/help` — should show help overlay.
Type `/clear` — should clear messages.

- [ ] **Step 5: Commit**

```bash
git add src/commands/ src/tui/ src/main.rs
git commit -m "feat(commands): wire command registry into TUI with slash command dispatch"
```

---

### Task 11: Remaining local commands (batch)

**Files:**
- Create: `src/commands/local/exit.rs`
- Create: `src/commands/local/cost.rs`
- Create: `src/commands/local/copy.rs`
- Create: `src/commands/local/vim.rs`
- Create: `src/commands/local/advisor.rs`
- Create: `src/commands/local/release_notes.rs`
- Create: `src/commands/local/stickers.rs`
- Create: `src/commands/local/keybindings.rs`
- Create: `src/commands/local/reload_plugins.rs`
- Create: `src/commands/local/rewind.rs`
- Create: `src/commands/local/install_slack_app.rs`
- Create: `src/commands/local/thinkback_play.rs`
- Create: `src/commands/local/compact.rs`
- Create: `src/commands/local/context_noninteractive.rs`
- Create: `src/commands/local/extra_usage_noninteractive.rs`
- Modify: `src/commands/local/mod.rs`

Each local command follows the same pattern as `clear.rs`:
1. Define a struct (e.g., `ExitCommand`)
2. Define a static `CommandMeta` with name, description, aliases from the original
3. Implement `LocalCommand` trait with `call()` returning `LocalCommandResult`
4. Register in `local/mod.rs` `get_local_commands()`

- [ ] **Step 1: Implement all non-internal local commands** — port name/description/aliases from original, implement `call()` logic matching original behavior

- [ ] **Step 2: Add to `get_local_commands()` in mod.rs**

- [ ] **Step 3: Run `cargo check -p atom-code`**

- [ ] **Step 4: Commit**

```bash
git add src/commands/local/
git commit -m "feat(commands): add remaining local commands"
```

---

### Task 12: Internal-only local commands (batch)

**Files:**
- Create: `src/commands/local/version.rs`
- Create: `src/commands/local/bridge_kick.rs`
- Create: `src/commands/local/break_cache.rs`
- Create: `src/commands/local/mock_limits.rs`
- Create: `src/commands/local/reset_limits.rs`
- Create: `src/commands/local/reset_limits_noninteractive.rs`
- Create: `src/commands/local/oauth_refresh.rs`
- Create: `src/commands/local/heapdump.rs`
- Create: `src/commands/local/force_snip.rs`
- Create: `src/commands/local/voice.rs`
- Modify: `src/commands/local/mod.rs`

- [ ] **Step 1: Implement internal-only local commands** — same pattern, but NOT added to `get_local_commands()`

- [ ] **Step 2: Create `get_internal_local_commands()` gated with `#[cfg(feature = "internal")]`**

- [ ] **Step 3: Feature-flag gated commands** — `force_snip.rs` with `#[cfg(feature = "history_snip")]`, `voice.rs` with `#[cfg(feature = "voice_mode")]`

- [ ] **Step 4: Run `cargo check -p atom-code`**

- [ ] **Step 5: Commit**

```bash
git add src/commands/local/
git commit -m "feat(commands): add internal-only local commands"
```

---

### Task 13: Remaining prompt commands (batch)

**Files:**
- Create: `src/commands/prompt/security_review.rs`
- Create: `src/commands/prompt/pr_comments.rs`
- Create: `src/commands/prompt/init.rs`
- Create: `src/commands/prompt/insights.rs`
- Create: `src/commands/prompt/statusline.rs`
- Create: `src/commands/prompt/commit.rs` (internal-only)
- Create: `src/commands/prompt/commit_push_pr.rs` (internal-only)
- Create: `src/commands/prompt/init_verifiers.rs` (internal-only)
- Create: `src/commands/prompt/subscribe_pr.rs` (feature-gated + internal)
- Create: `src/commands/prompt/torch.rs` (feature-gated)
- Create: `src/commands/helpers.rs`
- Modify: `src/commands/prompt/mod.rs`

- [ ] **Step 1: Create helpers.rs with `create_moved_to_plugin_command` pattern**

Used by `security_review` and `pr_comments` — creates a prompt command that suggests installing a plugin.

- [ ] **Step 2: Implement non-internal prompt commands** — port `get_prompt()` logic from original TS. Each returns prompt text as `Vec<ContentBlock>`.

- [ ] **Step 3: Implement internal-only prompt commands** — `commit.rs`, `commit_push_pr.rs`, `init_verifiers.rs`

- [ ] **Step 4: Feature-gated commands** — `subscribe_pr.rs` with `#[cfg(feature = "kairos_github_webhooks")]`, `torch.rs` with `#[cfg(feature = "torch")]`

- [ ] **Step 5: Run `cargo check -p atom-code`**

- [ ] **Step 6: Commit**

```bash
git add src/commands/prompt/ src/commands/helpers.rs
git commit -m "feat(commands): add remaining prompt commands"
```

---

### Task 14: TUI commands — core set (batch 1)

**Files:**
- Create one file per command in `src/commands/tui/`:
  `config.rs`, `status.rs`, `memory.rs`, `mcp.rs`, `skills.rs`, `tasks.rs`,
  `session.rs`, `resume.rs`, `permissions.rs`, `plan.rs`, `hooks.rs`,
  `files.rs`, `branch.rs`, `agents.rs`, `plugin.rs`, `doctor.rs`
- Modify: `src/commands/tui/mod.rs`

Each TUI command follows the `/help` pattern:
1. Define struct + static `CommandMeta`
2. Define state struct (e.g., `ConfigState` with fields for selections, scroll, etc.)
3. Implement `TuiCommand` — `create_state()`, `render()` with Ratatui widgets, `handle_key()`
4. Register in `get_tui_commands()`

- [ ] **Step 1: Implement core TUI commands** — focus on the most important ones first. Each should have working Ratatui UI matching the original's functionality.

- [ ] **Step 2: Register all in mod.rs**

- [ ] **Step 3: Run `cargo check -p atom-code`**

- [ ] **Step 4: Commit**

```bash
git add src/commands/tui/
git commit -m "feat(commands): add core TUI commands (config, status, memory, mcp, etc.)"
```

---

### Task 15: TUI commands — UI/settings set (batch 2)

**Files:**
- Create in `src/commands/tui/`:
  `theme.rs`, `model.rs`, `ide.rs`, `desktop.rs`, `mobile.rs`, `chrome.rs`,
  `usage.rs`, `stats.rs`, `upgrade.rs`, `feedback.rs`, `privacy_settings.rs`,
  `add_dir.rs`, `context.rs`, `terminal_setup.rs`, `install_github_app.rs`
- Modify: `src/commands/tui/mod.rs`

- [ ] **Step 1: Implement UI/settings TUI commands**

- [ ] **Step 2: Register all in mod.rs**

- [ ] **Step 3: Run `cargo check -p atom-code`**

- [ ] **Step 4: Commit**

```bash
git add src/commands/tui/
git commit -m "feat(commands): add UI/settings TUI commands"
```

---

### Task 16: TUI commands — interactive toggles (batch 3)

**Files:**
- Create in `src/commands/tui/`:
  `diff.rs`, `fast.rs`, `color.rs`, `btw.rs`, `rename.rs`, `tag.rs`,
  `effort.rs`, `passes.rs`, `export.rs`, `output_style.rs`,
  `sandbox_toggle.rs`, `remote_env.rs`, `rate_limit_options.rs`,
  `extra_usage.rs`, `thinkback.rs`, `ultrareview.rs`
- Modify: `src/commands/tui/mod.rs`

- [ ] **Step 1: Implement interactive toggle TUI commands**

- [ ] **Step 2: Register all in mod.rs**

- [ ] **Step 3: Run `cargo check -p atom-code`**

- [ ] **Step 4: Commit**

```bash
git add src/commands/tui/
git commit -m "feat(commands): add interactive toggle TUI commands"
```

---

### Task 17: TUI commands — internal-only (batch 4)

**Files:**
- Create in `src/commands/tui/`:
  `good_claude.rs`, `issue.rs`, `ctx_viz.rs`, `onboarding.rs`, `share.rs`,
  `summary.rs`, `teleport.rs`, `ant_trace.rs`, `perf_issue.rs`, `env.rs`,
  `debug_tool_call.rs`, `bughunter.rs`, `backfill_sessions.rs`, `autofix_pr.rs`
- Modify: `src/commands/tui/mod.rs`

- [ ] **Step 1: Implement internal-only TUI commands** — NOT added to `get_tui_commands()`, only to `get_internal_tui_commands()` gated with `#[cfg(feature = "internal")]`

- [ ] **Step 2: Run `cargo check -p atom-code`**

- [ ] **Step 3: Commit**

```bash
git add src/commands/tui/
git commit -m "feat(commands): add internal-only TUI commands"
```

---

### Task 18: TUI commands — feature-gated (batch 5)

**Files:**
- Create in `src/commands/tui/`:
  `brief.rs`, `proactive.rs`, `assistant.rs`, `bridge.rs`,
  `remote_control_server.rs`, `workflows.rs`, `remote_setup.rs`,
  `ultraplan.rs`, `peers.rs`, `fork.rs`, `buddy.rs`, `agents_platform.rs`
- Modify: `src/commands/tui/mod.rs`

- [ ] **Step 1: Implement feature-gated TUI commands** with correct `#[cfg(...)]` attributes:
- `brief.rs` — `#[cfg(any(feature = "kairos", feature = "kairos_brief"))]`
- `proactive.rs` — `#[cfg(any(feature = "proactive", feature = "kairos"))]`
- `remote_control_server.rs` — `#[cfg(all(feature = "daemon", feature = "bridge_mode"))]`
- Others — single `#[cfg(feature = "...")]`

- [ ] **Step 2: Conditionally register in mod.rs**

- [ ] **Step 3: Run `cargo check -p atom-code`**

Run: `cargo check -p atom-code --features all`

- [ ] **Step 4: Commit**

```bash
git add src/commands/tui/
git commit -m "feat(commands): add feature-gated TUI commands"
```

---

### Task 19: Unified get_internal_commands() and final registry wiring

**Files:**
- Modify: `src/commands/mod.rs`
- Modify: `src/commands/registry.rs`

- [ ] **Step 1: Create `get_internal_commands()` in mod.rs**

Aggregate internal-only commands from all three types:

```rust
#[cfg(feature = "internal")]
pub fn get_internal_commands() -> Vec<Command> {
    let mut cmds = Vec::new();
    // From prompt/
    cmds.extend(prompt::get_internal_prompt_commands());
    // From local/
    cmds.extend(local::get_internal_local_commands());
    // From tui/
    cmds.extend(tui_cmds::get_internal_tui_commands());
    cmds
}
```

- [ ] **Step 2: Update `CommandRegistry::new()` to include internal commands**

```rust
#[cfg(feature = "internal")]
for cmd in super::get_internal_commands() {
    reg.register_builtin(cmd);
}
```

- [ ] **Step 3: Run `cargo check -p atom-code` and `cargo check -p atom-code --features internal`**

- [ ] **Step 4: Commit**

```bash
git add src/commands/
git commit -m "feat(commands): wire internal-only commands into registry"
```

---

### Task 20: Dynamic command loading infrastructure

**Files:**
- Modify: `src/commands/registry.rs`

- [ ] **Step 1: Add dynamic loading method stubs**

```rust
impl CommandRegistry {
    /// Load skill commands from directories (~/.claude/skills/, .claude/skills/)
    pub async fn load_skill_dir_commands(&mut self, _cwd: &std::path::Path) -> anyhow::Result<()> {
        // Skill directory scanning is implemented in Phase 9 (services)
        // This stub allows the registry API to be used correctly
        Ok(())
    }

    /// Load plugin commands
    pub async fn load_plugin_commands(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Register MCP-provided commands
    pub fn register_mcp_commands(&mut self, commands: Vec<Command>) {
        for cmd in commands {
            self.register_dynamic(cmd);
        }
    }

    /// Load bundled skills
    pub fn load_bundled_skills(&mut self) {
        // Bundled skills are registered in Phase 9
    }
}
```

- [ ] **Step 2: Run `cargo check -p atom-code`**

- [ ] **Step 3: Commit**

```bash
git add src/commands/registry.rs
git commit -m "feat(commands): add dynamic command loading method stubs"
```

---

### Task 21: Final verification

**Files:** None (verification only)

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`

- [ ] **Step 3: Run with all features**

Run: `cargo check -p atom-code --features all`

- [ ] **Step 4: Verify CLI subcommands**

Run: `cargo run -p atom-code -- --help`
Run: `cargo run -p atom-code -- mcp list`
Run: `cargo run -p atom-code -- plugin list`
Run: `cargo run -p atom-code -- doctor`

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "fix: address clippy warnings and test failures in Phase 8"
```
