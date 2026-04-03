use std::io;
use std::sync::Arc;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use super::app::{App, DisplayMessage, DisplayRole, PermissionDialog};
use super::ui;
use crate::commands::registry::CommandRegistry;
use crate::commands::traits::Command;
use crate::commands::types::{CommandAction, LocalCommandResult};
use crate::query::{PermissionAsk, PermissionReply, QueryEvent};
use crate::services::api::provider::LlmProvider;
use crate::services::api::types::SystemBlock;
use crate::services::mcp::McpConnectionManager;
use crate::services::session_transcript::{SessionStore, SessionTranscript};
use crate::tools::get_builtin_tools;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};
use crate::types::permissions::{PermissionMode, PermissionRule, ToolPermissionContext};

/// Run the TUI application
pub async fn run_app(
    provider: Option<Arc<dyn LlmProvider>>,
    mcp_manager: McpConnectionManager,
    system_prompt: Option<Vec<SystemBlock>>,
    command_registry: CommandRegistry,
    permission_mode: PermissionMode,
    resume_transcript: Option<SessionTranscript>,
    no_session_persistence: bool,
    allow_rules: Vec<PermissionRule>,
    deny_rules: Vec<PermissionRule>,
    tools_filter: Option<Vec<String>>,
) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let model = provider.as_ref()
        .map(|p| p.model_info().id.clone())
        .unwrap_or_else(|| "(no provider)".into());
    let mut app = App::new(model.clone(), command_registry);
    app.no_session_persistence = no_session_persistence;

    // Restore session if resuming
    if let Some(transcript) = resume_transcript {
        let msg_count = transcript.messages.len();
        app.session_id = transcript.session_id.clone();
        app.created_at = transcript.created_at;
        let restored = App::restore_display_messages(&transcript.messages);
        app.messages.extend(restored);
        app.api_messages = transcript.messages;
        app.messages.push(DisplayMessage {
            role: DisplayRole::System,
            content: format!(
                "Resumed session {} ({} messages)",
                transcript.session_id, msg_count
            ),
            tool_name: None,
        });
    }

    let (query_event_tx, mut query_event_rx) = mpsc::unbounded_channel::<QueryEvent>();
    let mut permission_rx: Option<mpsc::UnboundedReceiver<PermissionAsk>> = None;

    loop {
        // Render
        terminal.draw(|frame| {
            let has_active = app.active_command.is_some();

            if has_active {
                // Split screen: chat on left, command drawer on right
                let area = frame.area();
                let drawer_width = (area.width / 2).max(30).min(area.width.saturating_sub(20));
                let chunks = Layout::horizontal([
                    Constraint::Min(20),
                    Constraint::Length(drawer_width),
                ])
                .split(area);

                // Left: normal chat UI (read-only borrow of app)
                ui::render_in_area(frame, &app, chunks[0]);

                // Right: command drawer (need mutable borrow of active_command state)
                if let Some(active) = &mut app.active_command {
                    let cmd_name = active.command_name.clone();
                    if let Some(Command::Tui(tui_cmd)) = app.command_registry.find(&cmd_name) {
                        // Safety: we clone cmd_name above to avoid borrow conflict
                        let active = app.active_command.as_mut().unwrap();
                        tui_cmd.render(frame, chunks[1], active.state.as_mut());
                    }
                }
            } else {
                ui::render(frame, &app);
            }
        })?;

        // Use tokio::select! to handle terminal events, query events, and permission
        // requests concurrently. Terminal polling is wrapped in spawn_blocking to avoid
        // blocking the tokio runtime.
        tokio::select! {
            // Terminal events via spawn_blocking (doesn't block tokio runtime)
            terminal_event = tokio::task::spawn_blocking(|| {
                if event::poll(std::time::Duration::from_millis(30)).unwrap_or(false) {
                    event::read().ok()
                } else {
                    None
                }
            }) => {
                if let Ok(Some(ev)) = terminal_event {
                    match ev {
                        Event::Key(key) => {
                            // Permission dialog takes priority over all other key handling
                            if app.permission_dialog.is_some() {
                                let resolved = app.handle_permission_key(key);
                                if resolved {
                                    let dialog = app.permission_dialog.take().unwrap();
                                    if let Some(reply_tx) = dialog.reply_tx {
                                        let reply = match dialog.selected {
                                            0 => PermissionReply::Allow,
                                            1 => PermissionReply::AllowAlways {
                                                tool_name: dialog.tool_name.clone(),
                                                rule_content: None,
                                            },
                                            _ => PermissionReply::Deny,
                                        };
                                        let _ = reply_tx.send(reply);
                                    }
                                }
                            } else if app.active_command.is_some() {
                                let cmd_name = app.active_command.as_ref().unwrap().command_name.clone();
                                if let Some(Command::Tui(tui_cmd)) = app.command_registry.find(&cmd_name) {
                                    let state = &mut app.active_command.as_mut().unwrap().state;
                                    let action = tui_cmd.handle_key(key, state.as_mut());
                                    match action {
                                        CommandAction::Done(_) => {
                                            app.active_command = None;
                                        }
                                        CommandAction::SetInput(text) => {
                                            app.input = text;
                                            app.cursor_pos = app.input.chars().count();
                                            app.active_command = None;
                                        }
                                        CommandAction::Continue => {}
                                    }
                                }
                            } else if key.code == KeyCode::Enter && !app.is_loading && !app.input.is_empty() {
                                if app.typeahead.is_some() {
                                    app.accept_typeahead();
                                }
                                let input = app.take_input();
                                handle_submit(&mut app, &input, &provider, &system_prompt, &query_event_tx, &permission_mode, &mut permission_rx, &mcp_manager, &allow_rules, &deny_rules, &tools_filter).await;
                            } else {
                                let typeahead_accepted = app.handle_key(key);
                                if typeahead_accepted && !app.is_loading && !app.input.is_empty() {
                                    let input = app.take_input();
                                    handle_submit(&mut app, &input, &provider, &system_prompt, &query_event_tx, &permission_mode, &mut permission_rx, &mcp_manager, &allow_rules, &deny_rules, &tools_filter).await;
                                }
                            }
                        }
                        Event::Resize(w, h) => {
                            app.status = format!("Model: {} | {}x{}", model, w, h);
                        }
                        _ => {}
                    }
                }
            }

            // Query events from the spawned query task
            Some(event) = query_event_rx.recv() => {
                let is_response_complete = matches!(event, QueryEvent::ResponseComplete { .. });
                app.handle_query_event(event);

                // Auto-save session after each completed response
                if is_response_complete && !app.no_session_persistence {
                    if let Ok(store) = SessionStore::new() {
                        let _ = store.save(&app.build_transcript());
                    }
                }
            }

            // Permission requests from the query engine
            Some(ask) = async {
                match &mut permission_rx {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                app.permission_dialog = Some(PermissionDialog {
                    tool_name: ask.tool_name,
                    description: ask.description,
                    selected: 0,
                    reply_tx: Some(ask.reply_tx),
                });
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

async fn handle_submit(
    app: &mut App,
    input: &str,
    provider: &Option<Arc<dyn LlmProvider>>,
    system_prompt: &Option<Vec<SystemBlock>>,
    query_event_tx: &mpsc::UnboundedSender<QueryEvent>,
    permission_mode: &PermissionMode,
    permission_rx: &mut Option<mpsc::UnboundedReceiver<PermissionAsk>>,
    mcp_manager: &McpConnectionManager,
    allow_rules: &[PermissionRule],
    deny_rules: &[PermissionRule],
    tools_filter: &Option<Vec<String>>,
) {
    // Check for slash commands
    if let Some((name, args)) = App::parse_slash_command(input) {
        dispatch_slash_command(app, &name, &args).await;
        return;
    }

    submit_query(app, input, provider, system_prompt, query_event_tx, permission_mode, permission_rx, mcp_manager, allow_rules, deny_rules, tools_filter).await;
}

/// What kind of command was found, with any data we need extracted before
/// releasing the borrow on the registry.
enum FoundCommand {
    Local { is_exit: bool },
    Tui,
    Prompt { progress: String },
    NotFound,
}

async fn dispatch_slash_command(app: &mut App, name: &str, args: &str) {
    // First pass: identify the command kind and extract needed data without
    // holding a mutable borrow on app.
    let found = match app.command_registry.find(name) {
        Some(Command::Local(cmd)) => {
            let is_exit = cmd.meta().name == "exit";
            FoundCommand::Local { is_exit }
        }
        Some(Command::Tui(_)) => FoundCommand::Tui,
        Some(Command::Prompt(cmd)) => FoundCommand::Prompt {
            progress: cmd.progress_message().to_string(),
        },
        None => FoundCommand::NotFound,
    };

    // Second pass: act on the result with full mutable access to app.
    match found {
        FoundCommand::Local { is_exit } => {
            use crate::commands::context::{CommandContext, CommandOptions, MessageUpdate, UiEvent};
            use crate::state::AppState;
            use std::path::PathBuf;
            use std::sync::Arc;
            use tokio::sync::RwLock;

            // Show the user's command first
            app.push_message(DisplayMessage {
                role: DisplayRole::User,
                content: format!("/{}{}", name, if args.is_empty() { String::new() } else { format!(" {}", args) }),
                tool_name: None,
            });

            // Build a CommandContext (same pattern as the Tui branch)
            let (msg_tx, _msg_rx) = tokio::sync::mpsc::unbounded_channel::<MessageUpdate>();
            let (ui_tx, _ui_rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
            let ctx = CommandContext {
                app_state: Arc::new(RwLock::new(AppState::default())),
                abort_signal: tokio_util::sync::CancellationToken::new(),
                cwd: PathBuf::from("."),
                message_tx: msg_tx,
                ui_event_tx: ui_tx,
                options: CommandOptions {
                    theme: "default".to_string(),
                },
            };

            // Look up the command again and call it
            if let Some(Command::Local(cmd)) = app.command_registry.find(name) {
                match cmd.call(args, &ctx).await {
                    Ok(LocalCommandResult::Text { value }) => {
                        app.push_message(DisplayMessage {
                            role: DisplayRole::System,
                            content: value,
                            tool_name: None,
                        });
                    }
                    Ok(LocalCommandResult::Compact { display_text }) => {
                        app.messages.clear();
                        app.api_messages.clear();
                        if let Some(text) = display_text {
                            app.push_message(DisplayMessage {
                                role: DisplayRole::System,
                                content: text,
                                tool_name: None,
                            });
                        }
                    }
                    Ok(LocalCommandResult::Skip) => {}
                    Err(e) => {
                        app.push_message(DisplayMessage {
                            role: DisplayRole::System,
                            content: format!("Error: {}", e),
                            tool_name: None,
                        });
                    }
                }
            }

            if is_exit {
                app.should_quit = true;
            }
        }
        FoundCommand::Tui => {
            use crate::commands::context::{CommandContext, CommandOptions, MessageUpdate, UiEvent};
            use crate::state::AppState;
            use std::path::PathBuf;
            use std::sync::Arc;
            use tokio::sync::RwLock;

            let (msg_tx, _msg_rx) = tokio::sync::mpsc::unbounded_channel::<MessageUpdate>();
            let (ui_tx, _ui_rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
            let ctx = CommandContext {
                app_state: Arc::new(RwLock::new(AppState::default())),
                abort_signal: tokio_util::sync::CancellationToken::new(),
                cwd: PathBuf::from("."),
                message_tx: msg_tx,
                ui_event_tx: ui_tx,
                options: CommandOptions {
                    theme: "default".to_string(),
                },
            };

            if let Some(Command::Tui(tui_cmd)) = app.command_registry.find(name) {
                let state = tui_cmd.create_state(&ctx);
                app.active_command = Some(super::app::ActiveCommand {
                    command_name: name.to_string(),
                    state,
                });
            }
        }
        FoundCommand::Prompt { progress } => {
            app.push_message(DisplayMessage {
                role: DisplayRole::User,
                content: format!("/{}{}", name, if args.is_empty() { String::new() } else { format!(" {}", args) }),
                tool_name: None,
            });
            app.push_message(DisplayMessage {
                role: DisplayRole::System,
                content: format!("{}... (LLM integration pending)", progress),
                tool_name: None,
            });
        }
        FoundCommand::NotFound => {
            app.push_message(DisplayMessage {
                role: DisplayRole::System,
                content: format!("Unknown command: /{}. Type /help for available commands.", name),
                tool_name: None,
            });
        }
    }
}

async fn submit_query(
    app: &mut App,
    input: &str,
    provider: &Option<Arc<dyn LlmProvider>>,
    system_prompt: &Option<Vec<SystemBlock>>,
    query_event_tx: &mpsc::UnboundedSender<QueryEvent>,
    permission_mode: &PermissionMode,
    permission_rx: &mut Option<mpsc::UnboundedReceiver<PermissionAsk>>,
    mcp_manager: &McpConnectionManager,
    allow_rules: &[PermissionRule],
    deny_rules: &[PermissionRule],
    tools_filter: &Option<Vec<String>>,
) {
    let Some(provider) = provider.clone() else {
        app.push_message(DisplayMessage {
            role: DisplayRole::System,
            content: "No LLM provider connected. Use --provider or --api-base.".into(),
            tool_name: None,
        });
        return;
    };

    // Add user message to display and API messages
    app.push_message(DisplayMessage {
        role: DisplayRole::User,
        content: input.to_string(),
        tool_name: None,
    });
    app.api_messages.push(ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: input.to_string(),
        }],
    });

    app.is_loading = true;

    // Create permission channel for this query
    let (perm_tx, perm_rx) = mpsc::unbounded_channel();
    *permission_rx = Some(perm_rx);

    // Gather tools: built-in + MCP
    let mut tools = get_builtin_tools();
    tools.extend(mcp_manager.get_all_tools().await);

    // Apply --tools filter
    if let Some(ref filter) = tools_filter {
        if filter.is_empty() || (filter.len() == 1 && filter[0].is_empty()) {
            tools.clear();
        } else if !filter.contains(&"default".to_string()) {
            tools.retain(|t| filter.iter().any(|f| t.name() == f));
        }
    }

    // Build query context
    let cwd = std::env::current_dir().unwrap_or_default();
    let hooks = crate::services::hooks::load_hooks(&cwd);
    let ctx = crate::query::QueryContext {
        messages: app.api_messages.clone(),
        tools,
        system_prompt: system_prompt.clone(),
        model: provider.model_info().id.clone(),
        max_tokens: provider.model_info().max_output_tokens,
        permission_ctx: ToolPermissionContext {
            mode: permission_mode.clone(),
            always_allow_rules: allow_rules.to_vec(),
            always_deny_rules: deny_rules.to_vec(),
            ..Default::default()
        },
        permission_tx: perm_tx,
        hooks,
        cwd,
        max_turns: None,       // TUI mode: no turn limit
        max_budget_usd: None,  // TUI mode: no budget limit
    };

    let event_tx = query_event_tx.clone();
    let cancel = tokio_util::sync::CancellationToken::new();

    // Spawn query as background task
    tokio::spawn(async move {
        match crate::query::engine::query(ctx, &*provider, event_tx.clone(), cancel).await {
            Ok(_) => {
                // Signal completion (event already sent via ResponseComplete in engine)
            }
            Err(e) => {
                // Write error to file for debugging (TUI swallows stderr)
                let _ = std::fs::write("/tmp/atom-query-error.log", format!("{:?}", e));
                let _ = event_tx.send(crate::query::QueryEvent::Error(format!("Query failed: {}", e)));
            }
        }
    });
}
