use clap::{Parser, Subcommand, ValueEnum};

use atom_code::commands::CommandRegistry;
use atom_code::services::api::openai_provider::OpenAiCompatibleProvider;
use atom_code::services::api::types::SystemBlock;
use atom_code::tools::get_builtin_tools;
use atom_code::tui::run::run_app;

// ---------------------------------------------------------------------------
// Output / Input format enums
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    StreamJson,
}

#[derive(Clone, Debug, ValueEnum)]
enum InputFormat {
    Text,
    StreamJson,
}

// ---------------------------------------------------------------------------
// Top-level CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "atom",
    version,
    about = "Open LLM-powered coding assistant CLI"
)]
struct Cli {
    /// Initial prompt (non-interactive mode)
    prompt: Option<String>,

    /// Print response and exit (non-interactive)
    #[arg(short = 'p', long = "print")]
    print: bool,

    /// Enable debug logging (optionally to a file path)
    #[arg(short = 'd', long = "debug", num_args = 0..=1, default_missing_value = "")]
    debug: Option<String>,

    /// Verbose output
    #[arg(long)]
    verbose: bool,

    /// Bare mode — no chrome, no TUI, just raw output
    #[arg(long)]
    bare: bool,

    /// Model to use
    #[arg(long)]
    model: Option<String>,

    /// Effort level (e.g. low, medium, high)
    #[arg(long)]
    effort: Option<String>,

    /// Agent mode identifier
    #[arg(long)]
    agent: Option<String>,

    /// Override the system prompt entirely
    #[arg(long)]
    system_prompt: Option<String>,

    /// Append text to the default system prompt
    #[arg(long)]
    append_system_prompt: Option<String>,

    /// Continue the most recent session
    #[arg(short = 'c', long = "continue")]
    continue_session: bool,

    /// Resume a previous session (optionally by id)
    #[arg(short = 'r', long = "resume", num_args = 0..=1, default_missing_value = "")]
    resume: Option<String>,

    /// Explicit session ID to use
    #[arg(long)]
    session_id: Option<String>,

    /// Session display name
    #[arg(short = 'n', long = "name")]
    name: Option<String>,

    /// Output format
    #[arg(long, value_enum)]
    output_format: Option<OutputFormat>,

    /// Input format
    #[arg(long, value_enum)]
    input_format: Option<InputFormat>,

    /// Allowed tool names (repeatable)
    #[arg(long, num_args = 1)]
    allowed_tools: Option<Vec<String>>,

    /// Disallowed tool names (repeatable)
    #[arg(long, num_args = 1)]
    disallowed_tools: Option<Vec<String>>,

    /// Skip all permission checks (dangerous!)
    #[arg(long)]
    dangerously_skip_permissions: bool,

    /// Permission mode (e.g. default, plan, bypassAll)
    #[arg(long)]
    permission_mode: Option<String>,

    /// MCP config files (repeatable)
    #[arg(long, num_args = 1)]
    mcp_config: Option<Vec<String>>,

    /// Additional directories to include (repeatable)
    #[arg(long, num_args = 1)]
    add_dir: Option<Vec<String>>,

    /// Maximum number of agentic turns
    #[arg(long)]
    max_turns: Option<u32>,

    /// Maximum budget in USD
    #[arg(long)]
    max_budget_usd: Option<f64>,

    /// API base URL (e.g. https://openrouter.ai/api/v1)
    #[arg(long)]
    api_base: Option<String>,

    /// API key
    #[arg(long)]
    api_key: Option<String>,

    /// Git worktree mode (optionally specify path)
    #[arg(short = 'w', long = "worktree", num_args = 0..=1, default_missing_value = "")]
    worktree: Option<String>,

    /// Path to settings file
    #[arg(long)]
    settings: Option<String>,

    #[command(subcommand)]
    subcommand: Option<SubCmd>,
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum SubCmd {
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

    /// Run diagnostic checks
    Doctor,

    /// Update atom to the latest version
    #[command(alias = "upgrade")]
    Update,

    /// List available agents
    Agents {
        /// Filter by setting source
        #[arg(long)]
        setting_sources: Option<String>,
    },

    /// Install shell integrations or extensions
    Install {
        /// Target to install (e.g. "zsh", "bash", "fish")
        target: Option<String>,

        /// Force reinstall even if already present
        #[arg(long)]
        force: bool,
    },

    /// Set up API token interactively
    SetupToken,
}

// ---------------------------------------------------------------------------
// MCP sub-actions
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum McpAction {
    /// Add an MCP server
    Add {
        /// Server name
        name: String,
        /// Server command and args
        #[arg(trailing_var_arg = true)]
        command: Vec<String>,
        /// Scope: user or project
        #[arg(short, long, default_value = "user")]
        scope: String,
    },

    /// Remove an MCP server
    Remove {
        /// Server name
        name: String,
        /// Scope: user or project
        #[arg(short, long, default_value = "user")]
        scope: String,
    },

    /// List configured MCP servers
    List {
        /// Scope: user or project
        #[arg(short, long, default_value = "user")]
        scope: String,
    },

    /// Browse the MCP marketplace
    Marketplace {
        #[command(subcommand)]
        action: MarketplaceAction,
    },
}

// ---------------------------------------------------------------------------
// Marketplace sub-actions
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum MarketplaceAction {
    /// Search the marketplace
    Search {
        /// Search query
        query: String,
    },

    /// Show details for a marketplace entry
    Details {
        /// Entry name or id
        name: String,
    },
}

// ---------------------------------------------------------------------------
// Auth sub-actions
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum AuthAction {
    /// Log in
    Login {
        /// Provider (e.g. anthropic, openai)
        #[arg(long)]
        provider: Option<String>,
    },

    /// Log out
    Logout,

    /// Show current auth status
    Status,
}

// ---------------------------------------------------------------------------
// Plugin sub-actions
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum PluginAction {
    /// Install a plugin
    Add {
        /// Plugin name or URL
        name: String,
    },

    /// Remove a plugin
    Remove {
        /// Plugin name
        name: String,
    },

    /// List installed plugins
    List,
}

// ---------------------------------------------------------------------------
// Subcommand execution (stubs)
// ---------------------------------------------------------------------------

async fn execute_subcommand(sub: SubCmd) -> anyhow::Result<()> {
    match sub {
        SubCmd::Mcp { action } => match action {
            McpAction::Add { name, command, scope } => {
                eprintln!("mcp add: name={name}, command={command:?}, scope={scope} (not yet implemented)");
            }
            McpAction::Remove { name, scope } => {
                eprintln!("mcp remove: name={name}, scope={scope} (not yet implemented)");
            }
            McpAction::List { scope } => {
                eprintln!("mcp list: scope={scope} (not yet implemented)");
            }
            McpAction::Marketplace { action } => match action {
                MarketplaceAction::Search { query } => {
                    eprintln!("mcp marketplace search: query={query} (not yet implemented)");
                }
                MarketplaceAction::Details { name } => {
                    eprintln!("mcp marketplace details: name={name} (not yet implemented)");
                }
            },
        },
        SubCmd::Auth { action } => match action {
            AuthAction::Login { provider } => {
                eprintln!("auth login: provider={provider:?} (not yet implemented)");
            }
            AuthAction::Logout => {
                eprintln!("auth logout (not yet implemented)");
            }
            AuthAction::Status => {
                eprintln!("auth status (not yet implemented)");
            }
        },
        SubCmd::Plugin { action } => match action {
            PluginAction::Add { name } => {
                eprintln!("plugin add: name={name} (not yet implemented)");
            }
            PluginAction::Remove { name } => {
                eprintln!("plugin remove: name={name} (not yet implemented)");
            }
            PluginAction::List => {
                eprintln!("plugin list (not yet implemented)");
            }
        },
        SubCmd::Doctor => {
            eprintln!("doctor: running diagnostics (not yet implemented)");
        }
        SubCmd::Update => {
            eprintln!("update: checking for updates (not yet implemented)");
        }
        SubCmd::Agents { setting_sources } => {
            eprintln!("agents: setting_sources={setting_sources:?} (not yet implemented)");
        }
        SubCmd::Install { target, force } => {
            eprintln!("install: target={target:?}, force={force} (not yet implemented)");
        }
        SubCmd::SetupToken => {
            eprintln!("setup-token: interactive token setup (not yet implemented)");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.subcommand {
        Some(sub) => execute_subcommand(sub).await?,
        None if cli.print => {
            eprintln!("Print mode not yet implemented");
        }
        None => {
            // Existing TUI boot logic
            let provider = cli.api_base.map(|base| {
                let model = cli.model.unwrap_or_else(|| "default".into());
                Box::new(OpenAiCompatibleProvider::new(base, cli.api_key, model))
                    as Box<dyn atom_code::services::api::provider::LlmProvider>
            });

            let tools = get_builtin_tools();
            let system_prompt =
                cli.system_prompt.map(|s| vec![SystemBlock::Text { text: s }]);

            let command_registry = CommandRegistry::new();
            run_app(provider, tools, system_prompt, command_registry).await?;
        }
    }

    Ok(())
}
