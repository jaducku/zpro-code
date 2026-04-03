use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};

use atom_code::commands::CommandRegistry;
use atom_code::services::api::create_provider;
use atom_code::services::api::provider::LlmProvider;
use atom_code::services::context::assemble_system_prompt;
use atom_code::services::session_transcript::SessionStore;
use atom_code::tui::run::run_app;
use atom_code::types::permissions::PermissionMode;

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

    /// LLM provider: "codex" or "openai" (default)
    #[arg(long)]
    provider: Option<String>,

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
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
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
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
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
            McpAction::List { scope: _ } => {
                println!("No MCP servers configured.");
                println!();
                println!("Add a server with: atom mcp add <name> <command...>");
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
            AuthAction::Status { json } => {
                if json {
                    println!("{}", serde_json::json!({
                        "authenticated": false,
                        "provider": null
                    }));
                } else {
                    println!("Not authenticated.");
                    println!("Use --api-key or --api-base to connect to an LLM provider.");
                }
            }
        },
        SubCmd::Plugin { action } => match action {
            PluginAction::Add { name } => {
                eprintln!("plugin add: name={name} (not yet implemented)");
            }
            PluginAction::Remove { name } => {
                eprintln!("plugin remove: name={name} (not yet implemented)");
            }
            PluginAction::List { json } => {
                if json {
                    println!("[]");
                } else {
                    println!("No plugins installed.");
                }
            }
        },
        SubCmd::Doctor => {
            println!("atom-code doctor");
            println!();

            // Check git
            let git_ok = std::process::Command::new("git")
                .arg("--version")
                .output()
                .is_ok();
            println!("  {} git", if git_ok { "\u{2713}" } else { "\u{2717}" });

            // Check working directory
            let cwd = std::env::current_dir().unwrap_or_default();
            println!("  \u{2713} working directory: {}", cwd.display());

            // Check if .git exists
            let is_git = cwd.join(".git").exists();
            println!(
                "  {} git repository",
                if is_git { "\u{2713}" } else { "\u{2717}" }
            );

            // Check config directory
            let config_dir = std::env::var("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".claude"))
                .unwrap_or_default();
            let config_exists = config_dir.exists();
            println!(
                "  {} config directory: {}",
                if config_exists { "\u{2713}" } else { "\u{2717}" },
                config_dir.display()
            );

            println!();
            if git_ok && config_exists {
                println!("  All checks passed!");
            } else {
                println!("  Some checks failed.");
            }
        }
        SubCmd::Update => {
            println!("atom-code v{}", env!("CARGO_PKG_VERSION"));
            println!("You are running the latest version.");
        }
        SubCmd::Agents { setting_sources: _ } => {
            println!("No custom agents configured.");
            println!();
            println!("Use --agents flag or agents.json to configure agents.");
        }
        SubCmd::Install { target, force } => {
            let version = target.as_deref().unwrap_or("stable");
            println!("Installing atom-code {}...", version);
            if force {
                println!("(force mode)");
            }
            println!("Installation not yet available via this command.");
            println!("Use: cargo install atom-code");
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
            // Print mode: non-interactive
            let provider: Arc<dyn LlmProvider> = match create_provider(
                cli.provider.as_deref(),
                cli.api_base.as_deref(),
                cli.api_key.as_deref(),
                cli.model.as_deref(),
            ) {
                Ok(p) => Arc::from(p),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let cwd = std::env::current_dir().unwrap_or_default();

            let mcp_configs = atom_code::services::mcp::load_mcp_configs(
                &cwd,
                cli.mcp_config.as_deref().unwrap_or(&[]),
            );
            let mut mcp_manager = atom_code::services::mcp::McpConnectionManager::new(mcp_configs);
            let mcp_errors = mcp_manager.connect_all().await;
            for err in &mcp_errors {
                eprintln!("Warning: MCP server error: {}", err);
            }

            let system_prompt = assemble_system_prompt(
                &cwd,
                cli.system_prompt.as_deref(),
                cli.append_system_prompt.as_deref(),
            );

            let output_format = match cli.output_format {
                Some(OutputFormat::Json) => atom_code::entrypoints::print::OutputFormat::Json,
                Some(OutputFormat::StreamJson) => atom_code::entrypoints::print::OutputFormat::StreamJson,
                _ => atom_code::entrypoints::print::OutputFormat::Text,
            };

            atom_code::entrypoints::print::run_print_mode(
                cli.prompt,
                provider,
                system_prompt,
                mcp_manager,
                output_format,
            ).await?;
        }
        None => {
            let provider: Option<Arc<dyn LlmProvider>> = match create_provider(
                cli.provider.as_deref(),
                cli.api_base.as_deref(),
                cli.api_key.as_deref(),
                cli.model.as_deref(),
            ) {
                Ok(p) => Some(Arc::from(p)),
                Err(e) if cli.provider.is_some() || cli.api_base.is_some() => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                Err(_) => None, // no-provider mode
            };

            let cwd = std::env::current_dir().unwrap_or_default();

            // Load MCP configs and connect servers
            let mcp_configs = atom_code::services::mcp::load_mcp_configs(
                &cwd,
                cli.mcp_config.as_deref().unwrap_or(&[]),
            );

            let mut mcp_manager = atom_code::services::mcp::McpConnectionManager::new(mcp_configs);
            let mcp_errors = mcp_manager.connect_all().await;
            for err in &mcp_errors {
                eprintln!("Warning: MCP server error: {}", err);
            }

            let system_prompt = Some(assemble_system_prompt(
                &cwd,
                cli.system_prompt.as_deref(),
                cli.append_system_prompt.as_deref(),
            ));

            let permission_mode = if cli.dangerously_skip_permissions {
                PermissionMode::BypassPermissions
            } else {
                cli.permission_mode
                    .as_deref()
                    .map(|s| match s {
                        "default" => PermissionMode::Default,
                        "plan" => PermissionMode::Plan,
                        "acceptEdits" => PermissionMode::AcceptEdits,
                        "bypassPermissions" => PermissionMode::BypassPermissions,
                        "auto" => PermissionMode::Auto,
                        _ => PermissionMode::Default,
                    })
                    .unwrap_or(PermissionMode::Default)
            };

            // Load a previous session if --continue or --resume is used
            let resume_transcript = if cli.continue_session {
                let store = SessionStore::new()?;
                let mut sessions = store.list()?;
                sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                match sessions.first() {
                    Some(latest) => Some(store.load(&latest.session_id)?),
                    None => {
                        eprintln!("No previous sessions found.");
                        None
                    }
                }
            } else if let Some(resume_val) = &cli.resume {
                let store = SessionStore::new()?;
                if resume_val.is_empty() {
                    // --resume with no value: load most recent
                    let mut sessions = store.list()?;
                    sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                    match sessions.first() {
                        Some(latest) => Some(store.load(&latest.session_id)?),
                        None => {
                            eprintln!("No previous sessions found.");
                            None
                        }
                    }
                } else {
                    // --resume <session-id>: load specific session
                    Some(store.load(resume_val)?)
                }
            } else {
                None
            };

            let command_registry = CommandRegistry::new();
            run_app(provider, mcp_manager, system_prompt, command_registry, permission_mode, resume_transcript).await?;
        }
    }

    Ok(())
}
