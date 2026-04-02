# Phase 8: CLI Commands — Design Spec

## Overview

Port the ~110 CLI commands from the original TypeScript Claude Code CLI to Rust. This includes the command type system, command registry with dynamic loading, clap CLI integration, and full Ratatui widget implementations for interactive commands.

**Scope:**
- Command type system (3 types: Prompt, Local, Tui)
- Command registry with dynamic loading (skills, plugins, MCP, workflows)
- clap CLI structure (~40 main options + subcommands)
- ~110 individual command implementations (login/logout excluded)
- TUI event loop integration for interactive commands
- Cargo feature flags for conditional compilation

**References:**
- Original: `src/commands.ts` (registry), `src/types/command.ts` (types), `src/commands/` (~110 commands)
- Spec: `docs/superpowers/specs/2026-04-02-ts-to-rust-migration-design.md` (Sections 10, 13)

---

## 1. Command Type System

Maps the original `Command = CommandBase & (PromptCommand | LocalCommand | LocalJSXCommand)`.

### CommandMeta (original CommandBase)

```rust
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

pub struct PluginInfo {
    pub plugin_manifest: PluginManifest,
    pub repository: String,
}

pub enum CommandAvailability { ClaudeAi, Console }

/// Maps SettingSource | 'builtin' | 'mcp' | 'plugin' | 'bundled'
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

pub enum CommandLoadedFrom { CommandsDeprecated, Skills, Plugin, Managed, Bundled, Mcp }
pub enum CommandKind { Workflow }
```

### Command Traits

```rust
/// Prompt command — sends prompt to LLM (original PromptCommand)
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
    fn hooks(&self) -> Option<&HooksSettings> { None }
    fn skill_root(&self) -> Option<&str> { None }
    fn effort(&self) -> Option<EffortValue> { None }
    fn paths(&self) -> Option<&[&str]> { None }
    async fn get_prompt(&self, args: &str, ctx: &CommandContext) -> Result<Vec<ContentBlock>>;
}

pub enum ContextMode { Inline, Fork }

/// Local command — returns text directly (original LocalCommand)
#[async_trait]
pub trait LocalCommand: Send + Sync {
    fn meta(&self) -> &CommandMeta;
    fn supports_non_interactive(&self) -> bool { false }
    async fn call(&self, args: &str, ctx: &CommandContext) -> Result<LocalCommandResult>;
}

pub enum LocalCommandResult {
    Text { value: String },
    Compact { compaction_result: CompactionResult, display_text: Option<String> },
    Skip,
}

/// TUI command — renders interactive Ratatui widget (original LocalJSXCommand)
pub trait TuiCommand: Send + Sync {
    fn meta(&self) -> &CommandMeta;
    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any>;
    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any);
    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction;
}

pub enum CommandAction {
    Continue,
    Done(Option<CommandDoneResult>),
    SetInput(String),
}

pub struct CommandDoneResult {
    pub text: Option<String>,
    pub display: CommandResultDisplay,
    pub should_query: bool,
    pub meta_messages: Vec<String>,
    pub next_input: Option<String>,
    pub submit_next_input: bool,
}

pub enum CommandResultDisplay { Skip, System, User }

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

    pub fn name(&self) -> &str { self.meta().name }

    pub fn is_enabled(&self) -> bool {
        self.meta().is_enabled.map_or(true, |f| f())
    }

    pub fn user_facing_name(&self) -> String {
        self.meta().user_facing_name.map_or_else(
            || self.meta().name.to_string(),
            |f| f(),
        )
    }
}
```

### CommandContext

Maps original `LocalJSXCommandContext` which extends `ToolUseContext`.

```rust
pub struct CommandContext {
    pub app_state: Arc<RwLock<AppState>>,
    pub abort_signal: CancellationToken,
    pub cwd: PathBuf,
    /// Channel for sending message updates (replaces setMessages callback)
    pub message_tx: mpsc::UnboundedSender<MessageUpdate>,
    /// Channel for UI events (replaces onChangeAPIKey, onInstallIDEExtension, etc.)
    pub ui_event_tx: mpsc::UnboundedSender<UiEvent>,
    /// Options from the TUI context
    pub options: CommandOptions,
}

pub struct CommandOptions {
    pub dynamic_mcp_config: Option<HashMap<String, ScopedMcpServerConfig>>,
    pub ide_installation_status: Option<IdeExtensionInstallationStatus>,
    pub theme: ThemeName,
}

pub enum UiEvent {
    ChangeApiKey,
    ChangeDynamicMcpConfig(HashMap<String, ScopedMcpServerConfig>),
    InstallIdeExtension(IdeType),
    ResumeSession { session_id: Uuid, entry_point: ResumeEntrypoint },
}

pub enum MessageUpdate {
    Set(Box<dyn FnOnce(Vec<Message>) -> Vec<Message> + Send>),
}
```

---

## 2. Command Registry

Maps the original `commands.ts` — `getCommands()`, `findCommand()`, `meetsAvailabilityRequirement()`.

```rust
pub struct CommandRegistry {
    /// Built-in commands (static, registered at startup)
    builtin: Vec<Command>,
    /// Dynamic commands (skills, plugins, MCP, workflows)
    dynamic: Vec<Command>,
}

impl CommandRegistry {
    /// Register all built-in commands
    pub fn new() -> Self;

    /// Dynamic loading
    pub async fn load_skill_dir_commands(&mut self, cwd: &Path) -> Result<()>;
    pub async fn load_plugin_commands(&mut self) -> Result<()>;
    pub fn register_mcp_commands(&mut self, commands: Vec<Command>);
    #[cfg(feature = "workflow_scripts")]
    pub async fn load_workflow_commands(&mut self, cwd: &Path) -> Result<()>;
    pub fn load_bundled_skills(&mut self);
    pub fn register_dynamic_skill(&mut self, cmd: Command);

    /// Lookup
    pub fn find(&self, name: &str) -> Option<&Command>;
    pub fn has(&self, name: &str) -> bool;

    /// Filtered views (availability + isEnabled checked fresh each call)
    pub fn available_commands(&self) -> Vec<&Command>;
    pub fn remote_safe_commands(&self) -> Vec<&Command>;
    pub fn bridge_safe_commands(&self) -> Vec<&Command>;
    pub fn skill_tool_commands(&self) -> Vec<&Command>;
    pub fn slash_command_tool_skills(&self) -> Vec<&Command>;
    pub fn mcp_skill_commands(&self) -> Vec<&Command>;

    /// Cache management
    pub fn clear_caches(&mut self);
    pub fn clear_memoization_caches(&mut self);
}

/// Availability check (original meetsAvailabilityRequirement)
pub fn meets_availability_requirement(cmd: &Command) -> bool;

/// Description formatting with source annotation
pub fn format_description_with_source(cmd: &Command) -> String;
```

**Load order** (matches original `loadAllCommands`):
1. Bundled skills
2. Built-in plugin skills
3. Skill directory commands
4. Workflow commands
5. Plugin commands + skills
6. Built-in commands (COMMANDS)

**Deduplication:** Dynamic skills discovered during file operations are inserted after plugin skills but before built-in commands. If a dynamic skill has the same name as an existing command, it is skipped. This insertion-point logic ensures dynamic skills can override plugin skills but not built-in commands.

**Filtering logic for `skill_tool_commands()`:**
Returns all prompt-type commands where `disable_model_invocation == false`,
`source != Builtin`, and at least one of: `loaded_from` is `Bundled`/`Skills`/`CommandsDeprecated`,
or `has_user_specified_description == true`, or `when_to_use.is_some()`.

**Filtering logic for `slash_command_tool_skills()`:**
Returns all prompt-type commands where `source != Builtin`,
`has_user_specified_description == true` or `when_to_use.is_some()`,
and `loaded_from` is `Skills`/`Plugin`/`Bundled` or `disable_model_invocation == true`.

---

## 3. File Structure

```
src/commands/
├── mod.rs                    # Command enum, re-exports
├── types.rs                  # CommandMeta, CommandAvailability, CommandSource, etc.
├── traits.rs                 # PromptCommand, LocalCommand, TuiCommand traits
├── registry.rs               # CommandRegistry implementation
├── context.rs                # CommandContext, LocalCommandResult, CommandAction
│
├── prompt/                   # Prompt commands (~12)
│   ├── mod.rs                # pub fn get_prompt_commands() -> Vec<Command>
│   ├── commit.rs             # internal-only
│   ├── commit_push_pr.rs     # internal-only
│   ├── review.rs
│   ├── security_review.rs
│   ├── pr_comments.rs
│   ├── init.rs
│   ├── init_verifiers.rs     # internal-only
│   ├── insights.rs
│   ├── statusline.rs
│   ├── subscribe_pr.rs       # #[cfg(feature = "kairos_github_webhooks")], internal-only
│   └── torch.rs              # #[cfg(feature = "torch")]
│
├── local/                    # Local commands (~25)
│   ├── mod.rs                # pub fn get_local_commands() -> Vec<Command>
│   ├── clear.rs
│   ├── compact.rs
│   ├── version.rs            # internal-only
│   ├── cost.rs
│   ├── exit.rs
│   ├── copy.rs
│   ├── vim.rs
│   ├── advisor.rs
│   ├── release_notes.rs
│   ├── stickers.rs
│   ├── keybindings.rs
│   ├── heapdump.rs           # internal-only
│   ├── reload_plugins.rs
│   ├── rewind.rs
│   ├── install_slack_app.rs
│   ├── thinkback_play.rs
│   ├── bridge_kick.rs        # internal-only
│   ├── break_cache.rs        # internal-only
│   ├── mock_limits.rs        # internal-only
│   ├── reset_limits.rs       # internal-only
│   ├── reset_limits_noninteractive.rs  # internal-only
│   ├── oauth_refresh.rs      # internal-only
│   ├── voice.rs              # #[cfg(feature = "voice_mode")]
│   ├── context_noninteractive.rs  # non-interactive variant of /context
│   ├── extra_usage_noninteractive.rs  # non-interactive variant of /extra-usage
│   └── force_snip.rs         # #[cfg(feature = "history_snip")], internal-only
│
├── helpers.rs                # createMovedToPluginCommand and other shared helpers
│
└── tui/                      # TUI commands (~60)
    ├── mod.rs                # pub fn get_tui_commands() -> Vec<Command>
    ├── help.rs
    ├── config.rs
    ├── status.rs
    ├── memory.rs
    ├── mcp.rs
    ├── skills.rs
    ├── tasks.rs
    ├── session.rs
    ├── resume.rs
    ├── permissions.rs
    ├── plan.rs
    ├── hooks.rs
    ├── files.rs
    ├── branch.rs
    ├── agents.rs
    ├── plugin.rs
    ├── doctor.rs
    ├── theme.rs
    ├── model.rs
    ├── ide.rs
    ├── desktop.rs
    ├── mobile.rs
    ├── chrome.rs
    ├── usage.rs
    ├── stats.rs
    ├── upgrade.rs
    ├── feedback.rs
    ├── privacy_settings.rs
    ├── add_dir.rs
    ├── context.rs
    ├── diff.rs
    ├── fast.rs
    ├── color.rs
    ├── btw.rs
    ├── rename.rs
    ├── tag.rs
    ├── effort.rs
    ├── passes.rs
    ├── export.rs
    ├── output_style.rs
    ├── sandbox_toggle.rs
    ├── remote_env.rs
    ├── rate_limit_options.rs
    ├── extra_usage.rs
    ├── terminal_setup.rs
    ├── install_github_app.rs
    ├── thinkback.rs
    ├── good_claude.rs         # internal-only
    ├── issue.rs               # internal-only
    ├── ctx_viz.rs             # internal-only
    ├── onboarding.rs          # internal-only
    ├── share.rs               # internal-only
    ├── summary.rs             # internal-only
    ├── teleport.rs            # internal-only
    ├── ant_trace.rs           # internal-only
    ├── perf_issue.rs          # internal-only
    ├── env.rs                 # internal-only
    ├── debug_tool_call.rs     # internal-only
    ├── bughunter.rs           # internal-only
    ├── backfill_sessions.rs   # internal-only
    ├── autofix_pr.rs          # internal-only
    ├── ultrareview.rs
    ├── brief.rs               # #[cfg(any(feature = "kairos", feature = "kairos_brief"))]
    ├── proactive.rs           # #[cfg(any(feature = "proactive", feature = "kairos"))]
    ├── agents_platform.rs     # #[cfg(feature = "agents_platform")]
    ├── assistant.rs           # #[cfg(feature = "kairos")]
    ├── bridge.rs              # #[cfg(feature = "bridge_mode")]
    ├── remote_control_server.rs # #[cfg(all(feature = "daemon", feature = "bridge_mode"))]
    ├── workflows.rs           # #[cfg(feature = "workflow_scripts")]
    ├── remote_setup.rs        # #[cfg(feature = "ccr_remote_setup")]
    ├── ultraplan.rs           # #[cfg(feature = "ultraplan")], internal-only
    ├── peers.rs               # #[cfg(feature = "uds_inbox")]
    ├── fork.rs                # #[cfg(feature = "fork_subagent")]
    └── buddy.rs               # #[cfg(feature = "buddy")]
```

---

## 4. Cargo Feature Flags

Maps original `feature('...')` build-time dead code elimination:

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

# Convenience
all = ["kairos", "kairos_brief", "kairos_github_webhooks", "bridge_mode",
       "voice_mode", "history_snip", "workflow_scripts", "ccr_remote_setup",
       "ultraplan", "torch", "uds_inbox", "fork_subagent", "buddy",
       "coordinator_mode", "agents_platform", "daemon", "proactive",
       "mcp_skills", "experimental_skill_search", "upload_user_settings"]

# Internal build (all features + internal-only commands)
internal = ["all"]
```

Usage in code (must match original's compound gates):
```rust
// brief: feature('KAIROS') || feature('KAIROS_BRIEF')
#[cfg(any(feature = "kairos", feature = "kairos_brief"))]
commands.push(Command::Tui(Box::new(BriefCommand)));

// proactive: feature('PROACTIVE') || feature('KAIROS')
#[cfg(any(feature = "proactive", feature = "kairos"))]
commands.push(Command::Tui(Box::new(ProactiveCommand)));

// remote_control_server: feature('DAEMON') && feature('BRIDGE_MODE')
#[cfg(all(feature = "daemon", feature = "bridge_mode"))]
commands.push(Command::Tui(Box::new(RemoteControlServerCommand)));

// simple single-feature gates
#[cfg(feature = "bridge_mode")]
commands.push(Command::Tui(Box::new(BridgeCommand)));
```

**Note:** All code is statically compiled — the original's `load: () => import(...)` lazy-loading pattern does not apply in Rust. This is an intentional divergence; all command implementations are linked at build time.

---

## 5. clap CLI Structure

Maps the original commander.js program structure 1:1.

### Main program options (~40)

```rust
#[derive(Parser)]
#[command(name = "atom", version)]
struct Cli {
    /// Prompt (non-interactive mode)
    prompt: Option<String>,

    // --- Core ---
    #[arg(short = 'p', long)]
    print: bool,
    #[arg(short = 'd', long)]
    debug: Option<Option<String>>,
    #[arg(long)]
    debug_file: Option<String>,
    #[arg(long)]
    debug_to_stderr: bool,
    #[arg(long)]
    verbose: bool,
    #[arg(long)]
    bare: bool,

    // --- Model/Agent ---
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    effort: Option<String>,
    #[arg(long)]
    agent: Option<String>,
    #[arg(long)]
    fallback_model: Option<String>,
    #[arg(long)]
    betas: Option<Vec<String>>,
    #[arg(long)]
    thinking: Option<String>,

    // --- System prompt ---
    #[arg(long)]
    system_prompt: Option<String>,
    #[arg(long)]
    system_prompt_file: Option<String>,
    #[arg(long)]
    append_system_prompt: Option<String>,
    #[arg(long)]
    append_system_prompt_file: Option<String>,

    // --- Session ---
    #[arg(short = 'c', long = "continue")]
    continue_session: bool,
    #[arg(short = 'r', long)]
    resume: Option<Option<String>>,
    #[arg(long)]
    fork_session: bool,
    #[arg(long)]
    session_id: Option<String>,
    #[arg(short = 'n', long)]
    name: Option<String>,
    #[arg(long)]
    from_pr: Option<Option<String>>,
    #[arg(long)]
    no_session_persistence: bool,

    // --- I/O format ---
    #[arg(long, value_enum)]
    output_format: Option<OutputFormat>,
    #[arg(long, value_enum)]
    input_format: Option<InputFormat>,
    #[arg(long)]
    json_schema: Option<String>,
    #[arg(long)]
    include_hook_events: bool,
    #[arg(long)]
    include_partial_messages: bool,
    #[arg(long)]
    replay_user_messages: bool,

    // --- Tools/Permissions ---
    #[arg(long)]
    allowed_tools: Option<Vec<String>>,
    #[arg(long)]
    tools: Option<Vec<String>>,
    #[arg(long)]
    disallowed_tools: Option<Vec<String>>,
    #[arg(long)]
    dangerously_skip_permissions: bool,
    #[arg(long)]
    allow_dangerously_skip_permissions: bool,
    #[arg(long, value_enum)]
    permission_mode: Option<String>,
    #[arg(long)]
    permission_prompt_tool: Option<String>,

    // --- MCP/Plugins ---
    #[arg(long)]
    mcp_config: Option<Vec<String>>,
    #[arg(long)]
    strict_mcp_config: bool,
    #[arg(long)]
    mcp_debug: bool,
    #[arg(long)]
    add_dir: Option<Vec<String>>,
    #[arg(long)]
    plugin_dir: Option<Vec<String>>,
    #[arg(long)]
    disable_slash_commands: bool,

    // --- Limits ---
    #[arg(long)]
    max_turns: Option<u32>,
    #[arg(long)]
    max_budget_usd: Option<f64>,
    #[arg(long)]
    max_thinking_tokens: Option<u32>,
    #[arg(long)]
    task_budget: Option<u32>,

    // --- Misc ---
    #[arg(long)]
    api_base: Option<String>,
    #[arg(long)]
    api_key: Option<String>,
    #[arg(short = 'w', long)]
    worktree: Option<Option<String>>,
    #[arg(long)]
    tmux: bool,
    #[arg(long)]
    ide: bool,
    #[arg(long)]
    chrome: Option<bool>,
    #[arg(long)]
    settings: Option<String>,
    #[arg(long)]
    agents: Option<String>,
    #[arg(long)]
    setting_sources: Option<String>,
    #[arg(long)]
    file: Option<Vec<String>>,
    #[arg(long)]
    prefill: Option<String>,

    #[command(subcommand)]
    subcommand: Option<SubCommand>,
}

#[derive(Clone, ValueEnum)]
pub enum OutputFormat { Text, Json, StreamJson }

#[derive(Clone, ValueEnum)]
pub enum InputFormat { Text, StreamJson }
```

### Subcommands

```rust
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
    /// Health check
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
    /// MCP server mode
    #[cfg(feature = "bridge_mode")]
    Open {
        cc_url: String,
        #[arg(short = 'p', long)]
        print: Option<Option<String>>,
        #[arg(long)]
        output_format: Option<String>,
    },
}

#[derive(Subcommand)]
enum McpAction {
    /// Start MCP server
    Serve {
        #[arg(short)]
        debug: bool,
        #[arg(long)]
        verbose: bool,
    },
    /// Add MCP server (interactive)
    Add {
        #[arg(short = 's', long)]
        scope: Option<String>,
        #[arg(long)]
        transport: Option<String>,
        #[arg(long)]
        name: Option<String>,
        args: Vec<String>,
    },
    /// Remove MCP server
    Remove {
        name: String,
        #[arg(short = 's', long)]
        scope: Option<String>,
    },
    /// List MCP servers
    List,
    /// Get MCP server details
    Get { name: String },
    /// Add MCP server from JSON
    AddJson {
        name: String,
        json: String,
        #[arg(short = 's', long, default_value = "local")]
        scope: String,
        #[arg(long)]
        client_secret: bool,
    },
    /// Import from Claude Desktop
    AddFromClaudeDesktop {
        #[arg(short = 's', long, default_value = "local")]
        scope: String,
    },
    /// Reset project choices
    ResetProjectChoices,
}

#[derive(Subcommand)]
enum AuthAction {
    /// Sign in
    Login {
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        sso: bool,
        #[arg(long)]
        console: bool,
        #[arg(long)]
        claudeai: bool,
    },
    /// Show auth status
    Status {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        text: bool,
    },
    /// Sign out
    Logout,
}

#[derive(Subcommand)]
enum PluginAction {
    /// Validate plugin manifest
    Validate { path: String },
    /// List installed plugins
    List {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        available: bool,
    },
    /// Install plugin
    #[command(alias = "i")]
    Install {
        plugin: String,
        #[arg(short = 's', long, default_value = "user")]
        scope: String,
    },
    /// Uninstall plugin
    #[command(alias = "remove", alias = "rm")]
    Uninstall {
        plugin: String,
        #[arg(short = 's', long, default_value = "user")]
        scope: String,
        #[arg(long)]
        keep_data: bool,
    },
    /// Enable plugin
    Enable {
        plugin: String,
        #[arg(short = 's', long)]
        scope: Option<String>,
    },
    /// Disable plugin
    Disable {
        plugin: Option<String>,
        #[arg(short = 'a', long)]
        all: bool,
        #[arg(short = 's', long)]
        scope: Option<String>,
    },
    /// Update plugin
    Update {
        plugin: String,
        #[arg(short = 's', long, default_value = "user")]
        scope: String,
    },
    /// Marketplace management
    Marketplace {
        #[command(subcommand)]
        action: MarketplaceAction,
    },
}

#[derive(Subcommand)]
enum MarketplaceAction {
    /// Add marketplace
    Add {
        source: String,
        #[arg(long)]
        sparse: Option<Vec<String>>,
        #[arg(long, default_value = "user")]
        scope: String,
    },
    /// List marketplaces
    List {
        #[arg(long)]
        json: bool,
    },
    /// Remove marketplace
    #[command(alias = "rm")]
    Remove { name: String },
    /// Update marketplace(s)
    Update { name: Option<String> },
}

#[derive(Subcommand)]
enum AutoModeAction {
    /// Print default auto mode rules as JSON
    Defaults,
    /// Print effective auto mode config as JSON
    Config,
    /// Get AI feedback on custom auto mode rules
    Critique {
        #[arg(long)]
        model: Option<String>,
    },
}
```

### Execution flow

```
main() {
    let cli = Cli::parse();

    match cli.subcommand {
        Some(sub) => execute_subcommand(sub),  // run and exit
        None if cli.print => run_print_mode(cli),  // non-interactive
        None => run_tui(cli),  // interactive TUI with slash commands
    }
}
```

---

## 6. TUI Event Loop Integration

How TUI commands integrate with the existing `tui/run.rs` event loop.

### App state extension

```rust
pub struct App {
    // ... existing fields from Phase 7
    pub command_registry: CommandRegistry,
    pub active_command: Option<ActiveCommand>,
}

pub struct ActiveCommand {
    pub command_name: String,  // lookup key into registry (stable across dynamic changes)
    pub state: Box<dyn Any>,
}
```

### Event loop modification

```rust
loop {
    terminal.draw(|frame| {
        if let Some(active) = &mut app.active_command {
            let cmd = app.command_registry.find(&active.command_name).unwrap();
            if let Command::Tui(tui_cmd) = cmd {
                tui_cmd.render(frame, frame.area(), &mut *active.state);
            }
        } else {
            ui::render(frame, &app);
        }
    });

    tokio::select! {
        event = terminal_events.next() => {
            if let Event::Key(key) = event {
                if let Some(active) = &mut app.active_command {
                    let cmd = app.command_registry.find(&active.command_name).unwrap();
                    if let Command::Tui(tui_cmd) = cmd {
                        match tui_cmd.handle_key(key, &mut *active.state) {
                            CommandAction::Continue => {}
                            CommandAction::Done(result) => {
                                app.active_command = None;
                                handle_command_done(&mut app, result);
                            }
                            CommandAction::SetInput(text) => {
                                app.active_command = None;
                                app.input = text;
                            }
                        }
                    }
                } else if key.code == KeyCode::Enter && app.input.starts_with('/') {
                    dispatch_slash_command(&mut app).await;
                } else {
                    app.handle_key(key);
                }
            }
        }
        // ... query events, tool progress
    }
}
```

### Slash command dispatch

```rust
async fn dispatch_slash_command(app: &mut App) {
    let input = app.take_input();
    let (name, args) = parse_slash_command(&input);

    if let Some(cmd) = app.command_registry.find(&name) {
        match cmd {
            Command::Prompt(p) => {
                let blocks = p.get_prompt(&args, &ctx).await?;
                // Send to LLM with allowed_tools restriction
                submit_prompt_command(app, blocks, p.allowed_tools()).await;
            }
            Command::Local(l) => {
                let result = l.call(&args, &ctx).await?;
                handle_local_result(app, result);
            }
            Command::Tui(t) => {
                let state = t.create_state(&ctx);
                app.active_command = Some(ActiveCommand {
                    command_name: name.clone(),
                    state,
                });
            }
        }
    }
}

fn parse_slash_command(input: &str) -> (String, String) {
    let trimmed = input.trim_start_matches('/');
    match trimmed.split_once(char::is_whitespace) {
        Some((name, args)) => (name.to_string(), args.to_string()),
        None => (trimmed.to_string(), String::new()),
    }
}
```

---

## 7. Dynamic Command Loading

### Skill directory loading

Skills are loaded from:
- `~/.claude/skills/` (user-level)
- `.claude/skills/` (project-level)
- Plugin skill directories

Each skill is a `SKILL.md` file with YAML frontmatter parsed into a `PromptCommand`.

### Plugin command loading

Plugins provide commands via their manifest. Loaded asynchronously at startup, cached with memoization.

### MCP command registration

MCP servers can provide prompt-type commands. These are registered via `register_mcp_commands()` after MCP client initialization.

### Workflow commands

`#[cfg(feature = "workflow_scripts")]` — Workflow scripts in the project are converted to commands.

### Dynamic skill discovery

During file operations (glob, read, etc.), skills with `paths` patterns matching touched files are dynamically added via `register_dynamic_skill()`.

---

## 8. Internal-Only Commands

Commands gated behind internal build flag. These commands exist in their respective
type directories (prompt/, local/, tui/) but are only registered when the `internal`
feature is enabled. They are NOT included in `get_prompt_commands()` / `get_local_commands()`
/ `get_tui_commands()` — instead they come from a separate `get_internal_commands()`.

```rust
/// Returns internal-only commands (only included in internal builds)
#[cfg(feature = "internal")]
pub fn get_internal_commands() -> Vec<Command> {
    let mut cmds = vec![
        Command::Tui(Box::new(BackfillSessionsCommand)),
        Command::Local(Box::new(BreakCacheCommand)),
        Command::Tui(Box::new(BughunterCommand)),
        Command::Prompt(Box::new(CommitCommand)),
        Command::Prompt(Box::new(CommitPushPrCommand)),
        Command::Tui(Box::new(CtxVizCommand)),
        Command::Tui(Box::new(GoodClaudeCommand)),
        Command::Tui(Box::new(IssueCommand)),
        Command::Prompt(Box::new(InitVerifiersCommand)),
        Command::Local(Box::new(MockLimitsCommand)),
        Command::Local(Box::new(BridgeKickCommand)),
        Command::Local(Box::new(VersionCommand)),
        Command::Local(Box::new(ResetLimitsCommand)),
        Command::Local(Box::new(ResetLimitsNonInteractiveCommand)),
        Command::Tui(Box::new(OnboardingCommand)),
        Command::Tui(Box::new(ShareCommand)),
        Command::Tui(Box::new(SummaryCommand)),
        Command::Tui(Box::new(TeleportCommand)),
        Command::Tui(Box::new(AntTraceCommand)),
        Command::Tui(Box::new(PerfIssueCommand)),
        Command::Tui(Box::new(EnvCommand)),
        Command::Local(Box::new(OauthRefreshCommand)),
        Command::Tui(Box::new(DebugToolCallCommand)),
        Command::Tui(Box::new(AutofixPrCommand)),
        Command::Local(Box::new(HeapdumpCommand)),
    ];

    #[cfg(feature = "history_snip")]
    cmds.push(Command::Local(Box::new(ForceSnipCommand)));

    #[cfg(feature = "ultraplan")]
    cmds.push(Command::Tui(Box::new(UltraplanCommand)));

    #[cfg(feature = "kairos_github_webhooks")]
    cmds.push(Command::Prompt(Box::new(SubscribePrCommand)));

    cmds
}
```

---

## 9. Special Command Sets

Maps original `REMOTE_SAFE_COMMANDS` and `BRIDGE_SAFE_COMMANDS`:

```rust
impl CommandRegistry {
    /// Commands safe for remote mode (local TUI state only)
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

    /// Local commands safe for bridge execution
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
}
```

---

## 10. Excluded / Modified Commands

- **Slash commands `/login` and `/logout`** — excluded from the slash command registry. These are Anthropic OAuth specific and not applicable to open LLM providers.
- **CLI subcommands `auth login`, `auth status`, `auth logout`** — kept in the clap CLI structure. These will be adapted for atom-code's own auth mechanism (API key management, provider configuration) rather than Anthropic OAuth. The `Auth` subcommand structure serves as the extension point for future auth providers.

## 11. Registry File Location

The command registry lives at `src/commands/registry.rs` (not `src/commands_registry.rs` as suggested in the overall migration spec). This keeps all command-related code under `src/commands/`. The overall spec's `commands_registry.rs` reference should be treated as superseded by this Phase 8 design.

## 12. Notes

- **No lazy loading in Rust:** The original TypeScript uses `load: () => import(...)` to defer heavy module evaluation. In Rust, all code is statically compiled. This is an intentional and correct divergence — Rust's compilation model eliminates the startup-time concern that motivated lazy loading.
- **Non-interactive variants:** Some commands have both interactive (TUI) and non-interactive (Local) variants (e.g., `context`/`contextNonInteractive`, `extra-usage`/`extraUsageNonInteractive`, `reset-limits`/`resetLimitsNonInteractive`). Both variants are ported as separate command structs.
- **`insights` command:** The original wraps `insights.ts` (113KB) in a lazy shim. In Rust, this is a regular prompt command — the binary size concern doesn't apply the same way.
