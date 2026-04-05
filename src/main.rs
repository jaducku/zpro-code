use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};

use zpro_code::commands::CommandRegistry;
use zpro_code::services::api::create_provider;
use zpro_code::services::api::provider::LlmProvider;
use zpro_code::services::config::AtomConfig;
use zpro_code::services::context::{assemble_system_prompt, assemble_system_prompt_with_config, SystemPromptConfig};
use zpro_code::services::session_transcript::SessionStore;
use zpro_code::tui::run::run_app;
use zpro_code::types::permissions::{PermissionMode, PermissionRule};

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

    /// Fork a resumed session into a new session ID
    #[arg(long)]
    fork_session: bool,

    /// Disable session persistence (no auto-save)
    #[arg(long)]
    no_session_persistence: bool,

    /// Filter available tools (comma-separated names, "default" for all, empty for none)
    #[arg(long, num_args = 1)]
    tools: Option<Vec<String>>,

    /// JSON schema for structured output (not yet implemented)
    #[arg(long)]
    json_schema: Option<String>,

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

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

// ---------------------------------------------------------------------------
// MCP sub-actions
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum McpAction {
    /// Add an MCP server interactively
    Add,

    /// Add an MCP server from a JSON string
    AddJson {
        /// Server name
        name: String,
        /// JSON config string (e.g. '{"command":"npx","args":["-y","my-server"]}')
        json: String,
        /// Scope: local (CWD) or user
        #[arg(short, long, default_value = "local")]
        scope: String,
    },

    /// Remove an MCP server
    Remove {
        /// Server name
        name: String,
        /// Scope: local (CWD) or user
        #[arg(short, long, default_value = "local")]
        scope: String,
    },

    /// List configured MCP servers
    List {
        /// Scope: local (CWD) or user
        #[arg(short, long, default_value = "local")]
        scope: String,
    },

    /// Show details of a specific MCP server
    Get {
        /// Server name
        name: String,
    },

    /// Reset project MCP choices
    ResetProjectChoices,

    /// Run zpro-code as an MCP server
    Serve,

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
// Config sub-actions
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current config
    Show,
    /// Set a config value
    Set {
        key: String,
        value: String,
    },
    /// Get a config value
    Get {
        key: String,
    },
    /// Show config file path
    Path,
}

// ---------------------------------------------------------------------------
// Subcommand execution (stubs)
// ---------------------------------------------------------------------------

async fn execute_subcommand(sub: SubCmd) -> anyhow::Result<()> {
    match sub {
        SubCmd::Mcp { action } => {
            let cwd = std::env::current_dir().unwrap_or_default();
            match action {
                McpAction::List { scope: _ } => {
                    let configs = zpro_code::services::mcp::read_mcp_json(&cwd);
                    if configs.is_empty() {
                        println!("No MCP servers configured.");
                    } else {
                        println!("MCP Servers:");
                        let mut names: Vec<_> = configs.keys().collect();
                        names.sort();
                        for name in names {
                            let cfg = &configs[name];
                            let full_cmd = if cfg.args.is_empty() {
                                cfg.command.clone()
                            } else {
                                format!("{} {}", cfg.command, cfg.args.join(" "))
                            };
                            println!("  {:<16}{}", name, full_cmd);
                        }
                    }
                }
                McpAction::Get { name } => {
                    let configs = zpro_code::services::mcp::read_mcp_json(&cwd);
                    match configs.get(&name) {
                        Some(cfg) => {
                            println!("Server: {}", name);
                            println!("  Command: {}", cfg.command);
                            println!("  Args: {}", if cfg.args.is_empty() {
                                "(none)".to_string()
                            } else {
                                cfg.args.join(" ")
                            });
                            println!("  Env: {}", if cfg.env.is_empty() {
                                "(none)".to_string()
                            } else {
                                cfg.env.iter()
                                    .map(|(k, v)| format!("{}={}", k, v))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            });
                        }
                        None => {
                            eprintln!("MCP server '{}' not found.", name);
                            std::process::exit(1);
                        }
                    }
                }
                McpAction::AddJson { name, json, scope: _ } => {
                    let server_config: zpro_code::services::mcp::McpServerConfig =
                        match serde_json::from_str(&json) {
                            Ok(c) => c,
                            Err(e) => {
                                eprintln!("Invalid JSON: {}", e);
                                std::process::exit(1);
                            }
                        };
                    let mut configs = zpro_code::services::mcp::read_mcp_json(&cwd);
                    configs.insert(name.clone(), server_config);
                    zpro_code::services::mcp::write_mcp_json(&cwd, &configs)?;
                    println!("Added MCP server '{}'", name);
                }
                McpAction::Add => {
                    println!("Use 'atom mcp add-json <name> <json>' to add a server.");
                    println!("Example: atom mcp add-json myserver '{{\"command\":\"npx\",\"args\":[\"-y\",\"my-mcp-server\"]}}'");
                }
                McpAction::Remove { name, scope: _ } => {
                    let mut configs = zpro_code::services::mcp::read_mcp_json(&cwd);
                    if configs.remove(&name).is_none() {
                        eprintln!("MCP server '{}' not found.", name);
                        std::process::exit(1);
                    }
                    zpro_code::services::mcp::write_mcp_json(&cwd, &configs)?;
                    println!("Removed MCP server '{}'", name);
                }
                McpAction::ResetProjectChoices => {
                    println!("Project choices reset.");
                }
                McpAction::Serve => {
                    println!("MCP serve mode not yet implemented.");
                }
                McpAction::Marketplace { action: marketplace_action } => match marketplace_action {
                    MarketplaceAction::Search { query } => {
                        eprintln!("mcp marketplace search: query={query} (not yet implemented)");
                    }
                    MarketplaceAction::Details { name } => {
                        eprintln!("mcp marketplace details: name={name} (not yet implemented)");
                    }
                },
            }
        }
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
            println!("zpro-code doctor");
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
            let config_dir = zpro_code::utils::paths::global_config_dir();
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
            println!("zpro-code v{}", env!("CARGO_PKG_VERSION"));
            println!("You are running the latest version.");
        }
        SubCmd::Agents { setting_sources: _ } => {
            println!("No custom agents configured.");
            println!();
            println!("Use --agents flag or agents.json to configure agents.");
        }
        SubCmd::Install { target, force } => {
            let version = target.as_deref().unwrap_or("stable");
            println!("Installing zpro-code {}...", version);
            if force {
                println!("(force mode)");
            }
            println!("Installation not yet available via this command.");
            println!("Use: cargo install zpro-code");
        }
        SubCmd::SetupToken => {
            eprintln!("setup-token: interactive token setup (not yet implemented)");
        }
        SubCmd::Config { action } => execute_config_action(action).await?,
    }
    Ok(())
}

async fn execute_config_action(action: ConfigAction) -> anyhow::Result<()> {
    match action {
        ConfigAction::Show => {
            let config = AtomConfig::load();
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        ConfigAction::Path => {
            println!("{}", zpro_code::utils::paths::global_config_dir().join("config.json").display());
        }
        ConfigAction::Get { key } => {
            let config = AtomConfig::load();
            let value = match key.as_str() {
                "provider" => config.provider.unwrap_or_default(),
                "api_key" => "(hidden)".to_string(),
                "api_base" => config.api_base.unwrap_or_default(),
                "default_model" => config.default_model.unwrap_or_default(),
                "permission_mode" => config.permission_mode.unwrap_or_default(),
                other => {
                    // Check models map
                    config.models.get(other)
                        .cloned()
                        .unwrap_or_else(|| format!("Unknown key: {}", other))
                }
            };
            println!("{}", value);
        }
        ConfigAction::Set { key, value } => {
            let mut config = AtomConfig::load();
            match key.as_str() {
                "provider" => config.provider = Some(value),
                "api_key" => {
                    let encrypted = zpro_code::services::keychain::encrypt_api_key(&value);
                    config.api_key = Some(encrypted);
                    println!("API key encrypted and saved.");
                }
                "api_base" => config.api_base = Some(value),
                "default_model" => config.default_model = Some(value),
                "permission_mode" => config.permission_mode = Some(value),
                other => {
                    // Treat as model alias
                    config.models.insert(other.to_string(), value.clone());
                    println!("Model alias '{}' set to '{}'", other, value);
                }
            }
            config.save()?;
            println!("Config saved.");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a tool spec like "Bash(git:*)" into (tool_name, optional rule_content).
fn parse_tool_spec(spec: &str) -> (String, Option<String>) {
    if let Some(paren_start) = spec.find('(') {
        let name = spec[..paren_start].to_string();
        let content = spec[paren_start + 1..].trim_end_matches(')').to_string();
        (name, Some(content))
    } else {
        (spec.to_string(), None)
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // First-time setup (only if no config.json and no CLI flags)
    if cli.subcommand.is_none() && !cli.print {
        zpro_code::entrypoints::setup::run_setup_if_needed();
    }

    // Load config file (CLI flags override config values)
    let config = AtomConfig::load();

    // Merge CLI flags with config: CLI wins
    let effective_provider = cli.provider.clone().or_else(|| config.provider.clone());
    let config_api_base = config.api_base_for_provider();
    let effective_api_base = cli.api_base.clone().or(config_api_base);
    let effective_api_key = cli.api_key.clone().or_else(|| {
        config.api_key.as_ref().and_then(|k| {
            zpro_code::services::keychain::decrypt_api_key(k)
        })
    });
    let effective_model = cli
        .model
        .as_deref()
        .map(|m| config.resolve_model(m))
        .or_else(|| config.default_model());

    // Warn about unimplemented flags
    if cli.json_schema.is_some() {
        eprintln!("Warning: --json-schema is not yet implemented");
    }

    // Parse --allowed-tools / --disallowed-tools into permission rules
    let mut allow_rules: Vec<PermissionRule> = Vec::new();
    let mut deny_rules: Vec<PermissionRule> = Vec::new();

    if let Some(allowed) = &cli.allowed_tools {
        for tool_spec in allowed {
            let (tool_name, rule_content) = parse_tool_spec(tool_spec);
            allow_rules.push(PermissionRule { tool_name, rule_content });
        }
    }

    if let Some(disallowed) = &cli.disallowed_tools {
        for tool_spec in disallowed {
            let (tool_name, rule_content) = parse_tool_spec(tool_spec);
            deny_rules.push(PermissionRule { tool_name, rule_content });
        }
    }

    let tools_filter = cli.tools.clone();

    match cli.subcommand {
        Some(sub) => execute_subcommand(sub).await?,
        None if cli.print => {
            // Print mode: non-interactive
            let provider: Arc<dyn LlmProvider> = match create_provider(
                effective_provider.as_deref(),
                effective_api_base.as_deref(),
                effective_api_key.as_deref(),
                effective_model.as_deref(),
            ) {
                Ok(p) => Arc::from(p),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let cwd = std::env::current_dir().unwrap_or_default();

            let mcp_configs = zpro_code::services::mcp::load_mcp_configs(
                &cwd,
                cli.mcp_config.as_deref().unwrap_or(&[]),
            );
            let mut mcp_manager = zpro_code::services::mcp::McpConnectionManager::new(mcp_configs);
            let mcp_errors = mcp_manager.connect_all().await;
            for err in &mcp_errors {
                eprintln!("Warning: MCP server error: {}", err);
            }

            let system_prompt = assemble_system_prompt(
                &cwd,
                cli.system_prompt.as_deref(),
                cli.append_system_prompt.as_deref(),
                None,
            );

            let output_format = match cli.output_format {
                Some(OutputFormat::Json) => zpro_code::entrypoints::print::OutputFormat::Json,
                Some(OutputFormat::StreamJson) => zpro_code::entrypoints::print::OutputFormat::StreamJson,
                _ => zpro_code::entrypoints::print::OutputFormat::Text,
            };

            zpro_code::entrypoints::print::run_print_mode(
                cli.prompt,
                provider,
                system_prompt,
                mcp_manager,
                output_format,
                cli.max_turns,
                cli.max_budget_usd,
                allow_rules.clone(),
                deny_rules.clone(),
                tools_filter.clone(),
            ).await?;
        }
        None => {
            let provider: Option<Arc<dyn LlmProvider>> = match create_provider(
                effective_provider.as_deref(),
                effective_api_base.as_deref(),
                effective_api_key.as_deref(),
                effective_model.as_deref(),
            ) {
                Ok(p) => Some(Arc::from(p)),
                Err(e) if effective_provider.is_some() || effective_api_base.is_some() => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                Err(_) => None, // no-provider mode
            };

            let cwd = std::env::current_dir().unwrap_or_default();

            // Load MCP configs and connect servers
            let mcp_configs = zpro_code::services::mcp::load_mcp_configs(
                &cwd,
                cli.mcp_config.as_deref().unwrap_or(&[]),
            );

            let mut mcp_manager = zpro_code::services::mcp::McpConnectionManager::new(mcp_configs);
            let mcp_errors = mcp_manager.connect_all().await;
            for err in &mcp_errors {
                eprintln!("Warning: MCP server error: {}", err);
            }

            // system_prompt is built later, after command_registry is loaded

            let permission_mode = if cli.dangerously_skip_permissions {
                PermissionMode::BypassPermissions
            } else {
                // CLI > config > default (Auto)
                let mode_str = cli.permission_mode.as_deref()
                    .or(config.permission_mode.as_deref());
                match mode_str {
                    Some("default") => PermissionMode::Default,
                    Some("plan") => PermissionMode::Plan,
                    Some("acceptEdits") => PermissionMode::AcceptEdits,
                    Some("bypassPermissions") => PermissionMode::BypassPermissions,
                    Some("auto") => PermissionMode::Auto,
                    _ => PermissionMode::Auto, // default to Auto
                }
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

            // --fork-session: generate a new session ID when resuming
            let resume_transcript = if cli.fork_session {
                resume_transcript.map(|mut t| {
                    t.session_id = uuid::Uuid::new_v4().to_string();
                    t
                })
            } else {
                resume_transcript
            };

            let mut command_registry = CommandRegistry::new();
            command_registry.load_skill_dir_commands(&cwd).await?;

            let skill_entries: Vec<(String, String, Option<String>)> = command_registry
                .skill_tool_commands()
                .iter()
                .map(|cmd| {
                    let meta = cmd.meta();
                    (
                        meta.name.to_string(),
                        meta.description.to_string(),
                        meta.when_to_use.map(|s| s.to_string()),
                    )
                })
                .collect();

            let system_prompt = Some(assemble_system_prompt_with_config(&SystemPromptConfig {
                cwd: &cwd,
                custom_prompt: cli.system_prompt.as_deref(),
                append_prompt: cli.append_system_prompt.as_deref(),
                output_style: None,
                skill_entries: &skill_entries,
            }));

            let command_registry = Arc::new(command_registry);
            run_app(
                provider,
                mcp_manager,
                system_prompt,
                command_registry,
                permission_mode,
                resume_transcript,
                cli.no_session_persistence,
                allow_rules,
                deny_rules,
                tools_filter,
            ).await?;
        }
    }

    Ok(())
}
