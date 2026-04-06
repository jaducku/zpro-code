use std::io;
use std::sync::Arc;

use crossterm::{
    event::{Event, EventStream, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::prelude::*;
use tokio::sync::mpsc;

use super::app::{App, DisplayMessage, DisplayRole, InputMode};
use super::ui;
use crate::commands::registry::CommandRegistry;
use crate::commands::traits::Command;
use crate::commands::types::{CommandAction, LocalCommandResult};
use crate::query::{PermissionAsk, PermissionReply, QueryEvent, UserQuestionAsk, UserQuestionReply};
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
    command_registry: Arc<CommandRegistry>,
    permission_mode: PermissionMode,
    resume_transcript: Option<SessionTranscript>,
    no_session_persistence: bool,
    allow_rules: Vec<PermissionRule>,
    deny_rules: Vec<PermissionRule>,
    tools_filter: Option<Vec<String>>,
    registry: Arc<crate::services::api::registry::ProviderRegistry>,
) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app_state = Arc::new(tokio::sync::RwLock::new(crate::state::AppState::default()));

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
    let mut question_rx: Option<mpsc::UnboundedReceiver<UserQuestionAsk>> = None;
    let mut provider = provider; // make mutable for /model switching
    let mut event_stream = EventStream::new();

    loop {
        // Render
        terminal.draw(|frame| {
            if app.active_command.is_some() {
                // Command active: split layout — messages shrink, command panel below
                let term_height = frame.area().height;
                let cmd_height = (term_height / 3).max(8).min(term_height.saturating_sub(8));
                let cmd_area = ui::render_with_command(frame, &app, cmd_height);

                if let Some(active) = &mut app.active_command {
                    let cmd_name = active.command_name.clone();
                    if let Some(Command::Tui(tui_cmd)) = app.command_registry.find(&cmd_name) {
                        let active = app.active_command.as_mut().unwrap();
                        tui_cmd.render(frame, cmd_area, active.state.as_mut());
                    }
                }
            } else {
                ui::render(frame, &app);
            }
        })?;

        // Use tokio::select! to handle terminal events, query events, and permission
        // requests concurrently. Uses crossterm's async EventStream for true non-blocking.
        // The tick branch ensures periodic re-renders (for loading spinner, dialog appearance).
        tokio::select! {
            // Periodic tick for re-rendering (30ms)
            _ = tokio::time::sleep(std::time::Duration::from_millis(30)) => {
                // Just re-render (terminal.draw at top of loop)
            }

            // Terminal events via async EventStream (truly non-blocking)
            maybe_event = event_stream.next() => {
                if let Some(Ok(ev)) = maybe_event {
                    match ev {
                        Event::Key(key) => {
                            // Question inline prompt
                            if let InputMode::Question { .. } = &app.input_mode {
                                match key.code {
                                    KeyCode::Up | KeyCode::Char('k') => {
                                        if let InputMode::Question { options, selected, .. } = &mut app.input_mode {
                                            if !options.is_empty() && *selected > 0 {
                                                *selected -= 1;
                                            }
                                        }
                                    }
                                    KeyCode::Down | KeyCode::Char('j') => {
                                        if let InputMode::Question { options, selected, .. } = &mut app.input_mode {
                                            if !options.is_empty() && *selected + 1 < options.len() {
                                                *selected += 1;
                                            }
                                        }
                                    }
                                    KeyCode::Enter => {
                                        if let InputMode::Question { options, selected, text_input, .. } = &app.input_mode {
                                            let answer = if options.is_empty() {
                                                text_input.clone()
                                            } else {
                                                options[*selected].0.clone()
                                            };
                                            if let InputMode::Question { reply_tx, .. } = std::mem::replace(&mut app.input_mode, InputMode::Normal) {
                                                if let Some(tx) = reply_tx {
                                                    let _ = tx.send(UserQuestionReply::Answer(answer));
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Esc => {
                                        if let InputMode::Question { reply_tx, .. } = std::mem::replace(&mut app.input_mode, InputMode::Normal) {
                                            if let Some(tx) = reply_tx {
                                                let _ = tx.send(UserQuestionReply::Dismissed);
                                            }
                                        }
                                    }
                                    KeyCode::Backspace => {
                                        if let InputMode::Question { options, text_input, .. } = &mut app.input_mode {
                                            if options.is_empty() {
                                                text_input.pop();
                                            }
                                        }
                                    }
                                    KeyCode::Char(c) => {
                                        if let InputMode::Question { options, text_input, .. } = &mut app.input_mode {
                                            if options.is_empty() {
                                                text_input.push(c);
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            // Permission inline prompt takes priority over all other key handling
                            else if let InputMode::Permission { .. } = &app.input_mode {
                                match key.code {
                                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                                        if let InputMode::Permission { reply_tx, .. } = std::mem::replace(&mut app.input_mode, InputMode::Normal) {
                                            if let Some(tx) = reply_tx { let _ = tx.send(PermissionReply::Allow); }
                                        }
                                    }
                                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                        if let InputMode::Permission { reply_tx, .. } = std::mem::replace(&mut app.input_mode, InputMode::Normal) {
                                            if let Some(tx) = reply_tx { let _ = tx.send(PermissionReply::Deny); }
                                        }
                                    }
                                    KeyCode::Char('a') | KeyCode::Char('A') => {
                                        if let InputMode::Permission { tool_name, reply_tx, .. } = std::mem::replace(&mut app.input_mode, InputMode::Normal) {
                                            if let Some(tx) = reply_tx { let _ = tx.send(PermissionReply::AllowAlways { tool_name, rule_content: None }); }
                                        }
                                    }
                                    _ => {} // ignore other keys in permission mode
                                }
                            } else if app.active_command.is_some() {
                                let cmd_name = app.active_command.as_ref().unwrap().command_name.clone();
                                if let Some(Command::Tui(tui_cmd)) = app.command_registry.find(&cmd_name) {
                                    let state = &mut app.active_command.as_mut().unwrap().state;
                                    let action = tui_cmd.handle_key(key, state.as_mut());
                                    match action {
                                        CommandAction::Done(result) => {
                                            // Handle command results (e.g., /model selection)
                                            if cmd_name == "model" {
                                                if let Some(ref r) = result {
                                                    if let Some(ref model_id) = r.text {
                                                        // Resolve new provider from registry (model_id is an alias)
                                                        match registry.resolve(model_id) {
                                                            Ok(new_provider) => {
                                                                let actual_model_id = new_provider.model_info().id.clone();
                                                                app.model_override = Some(actual_model_id.clone());
                                                                app.model = actual_model_id.clone();
                                                                provider = Some(new_provider);
                                                                app.push_message(DisplayMessage {
                                                                    role: DisplayRole::System,
                                                                    content: format!("Model switched to: {} ({})", model_id, actual_model_id),
                                                                    tool_name: None,
                                                                });
                                                            }
                                                            Err(e) => {
                                                                app.push_message(DisplayMessage {
                                                                    role: DisplayRole::System,
                                                                    content: format!("Failed to switch model: {}", e),
                                                                    tool_name: None,
                                                                });
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            if cmd_name == "output-style" {
                                                if let Some(ref r) = result {
                                                    if let Some(ref style_name) = r.text {
                                                        app.output_style = style_name.clone();
                                                        app.push_message(DisplayMessage {
                                                            role: DisplayRole::System,
                                                            content: format!("Output style set to: {}", style_name),
                                                            tool_name: None,
                                                        });
                                                    }
                                                }
                                            }
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
                                handle_submit(&mut app, &input, &provider, &system_prompt, &query_event_tx, &permission_mode, &mut permission_rx, &mut question_rx, &mcp_manager, &allow_rules, &deny_rules, &tools_filter, &app_state, &registry).await;
                            } else {
                                let typeahead_accepted = app.handle_key(key);
                                if typeahead_accepted && !app.is_loading && !app.input.is_empty() {
                                    let input = app.take_input();
                                    handle_submit(&mut app, &input, &provider, &system_prompt, &query_event_tx, &permission_mode, &mut permission_rx, &mut question_rx, &mcp_manager, &allow_rules, &deny_rules, &tools_filter, &app_state, &registry).await;
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

                // Clear loading label when response is complete
                if is_response_complete {
                    app.loading_label = None;
                }

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
                // Permission prompt shown only in the input area (no chat message)
                app.input_mode = InputMode::Permission {
                    tool_name: ask.tool_name,
                    description: ask.description,
                    reply_tx: Some(ask.reply_tx),
                };
            }

            // Question requests from tools (AskUserQuestionTool)
            Some(ask) = async {
                match &mut question_rx {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                // Question prompt shown only in the input area (no chat message)
                app.input_mode = InputMode::Question {
                    options: ask.options.into_iter().map(|o| (o.label, o.description)).collect(),
                    selected: 0,
                    text_input: String::new(),
                    reply_tx: Some(ask.reply_tx),
                };
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

#[allow(clippy::too_many_arguments)]
async fn handle_submit(
    app: &mut App,
    input: &str,
    provider: &Option<Arc<dyn LlmProvider>>,
    system_prompt: &Option<Vec<SystemBlock>>,
    query_event_tx: &mpsc::UnboundedSender<QueryEvent>,
    permission_mode: &PermissionMode,
    permission_rx: &mut Option<mpsc::UnboundedReceiver<PermissionAsk>>,
    question_rx: &mut Option<mpsc::UnboundedReceiver<UserQuestionAsk>>,
    mcp_manager: &McpConnectionManager,
    allow_rules: &[PermissionRule],
    deny_rules: &[PermissionRule],
    tools_filter: &Option<Vec<String>>,
    app_state: &Arc<tokio::sync::RwLock<crate::state::AppState>>,
    registry: &Arc<crate::services::api::registry::ProviderRegistry>,
) {
    // Check for slash commands
    if let Some((name, args)) = App::parse_slash_command(input) {
        dispatch_slash_command(app, &name, &args, provider, system_prompt, query_event_tx, permission_mode, permission_rx, question_rx, mcp_manager, allow_rules, deny_rules, tools_filter, app_state, registry).await;
        return;
    }

    submit_query(app, input, provider, system_prompt, query_event_tx, permission_mode, permission_rx, question_rx, mcp_manager, allow_rules, deny_rules, tools_filter, app_state, registry).await;
}

/// What kind of command was found, with any data we need extracted before
/// releasing the borrow on the registry.
enum FoundCommand {
    Local { is_exit: bool },
    Tui,
    Prompt,
    NotFound,
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_slash_command(
    app: &mut App,
    name: &str,
    args: &str,
    provider: &Option<Arc<dyn LlmProvider>>,
    system_prompt: &Option<Vec<SystemBlock>>,
    query_event_tx: &mpsc::UnboundedSender<QueryEvent>,
    permission_mode: &PermissionMode,
    permission_rx: &mut Option<mpsc::UnboundedReceiver<PermissionAsk>>,
    question_rx: &mut Option<mpsc::UnboundedReceiver<UserQuestionAsk>>,
    mcp_manager: &McpConnectionManager,
    allow_rules: &[PermissionRule],
    deny_rules: &[PermissionRule],
    tools_filter: &Option<Vec<String>>,
    app_state: &Arc<tokio::sync::RwLock<crate::state::AppState>>,
    registry: &Arc<crate::services::api::registry::ProviderRegistry>,
) {
    // First pass: identify the command kind and extract needed data without
    // holding a mutable borrow on app.
    let found = match app.command_registry.find(name) {
        Some(Command::Local(cmd)) => {
            let is_exit = cmd.meta().name == "exit";
            FoundCommand::Local { is_exit }
        }
        Some(Command::Tui(_)) => FoundCommand::Tui,
        Some(Command::Prompt(_)) => FoundCommand::Prompt,
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
        FoundCommand::Prompt => {
            use crate::commands::context::{CommandContext, CommandOptions, MessageUpdate, UiEvent};
            use crate::state::AppState;
            use std::path::PathBuf;
            use std::sync::Arc;
            use tokio::sync::RwLock;

            // Show skill usage (not the full prompt content)
            app.push_message(DisplayMessage {
                role: DisplayRole::User,
                content: format!("/{}{}", name, if args.is_empty() { String::new() } else { format!(" {}", args) }),
                tool_name: None,
            });

            // Set loading label to show which skill is being used
            app.loading_label = Some(format!("{} 스킬 사용", name));

            // Build a CommandContext for get_prompt()
            let (msg_tx, _msg_rx) = tokio::sync::mpsc::unbounded_channel::<MessageUpdate>();
            let (ui_tx, _ui_rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let cmd_ctx = CommandContext {
                app_state: Arc::new(RwLock::new(AppState::default())),
                abort_signal: tokio_util::sync::CancellationToken::new(),
                cwd,
                message_tx: msg_tx,
                ui_event_tx: ui_tx,
                options: CommandOptions {
                    theme: "default".to_string(),
                },
            };

            // Re-lookup the command and call get_prompt()
            let args_owned = args.to_string();
            let prompt_result = if let Some(Command::Prompt(cmd)) = app.command_registry.find(name) {
                cmd.get_prompt(&args_owned, &cmd_ctx).await
            } else {
                Err(anyhow::anyhow!("Command /{} not found as PromptCommand", name))
            };

            match prompt_result {
                Ok(blocks) => {
                    // Extract text from content blocks
                    let text: String = blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    if text.is_empty() {
                        app.loading_label = None;
                        app.push_message(DisplayMessage {
                            role: DisplayRole::System,
                            content: format!("/{} produced no prompt content.", name),
                            tool_name: None,
                        });
                    } else {
                        // Submit the prompt text as a query — but DON'T show the raw skill content
                        // submit_query_hidden sends to LLM without displaying the prompt in chat
                        submit_query_skill(app, &text, name, provider, system_prompt, query_event_tx, permission_mode, permission_rx, question_rx, mcp_manager, allow_rules, deny_rules, tools_filter, app_state, registry).await;
                    }
                }
                Err(e) => {
                    app.loading_label = None;
                    app.push_message(DisplayMessage {
                        role: DisplayRole::System,
                        content: format!("Error running /{}: {}", name, e),
                        tool_name: None,
                    });
                }
            }
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

#[allow(clippy::too_many_arguments)]
async fn submit_query(
    app: &mut App,
    input: &str,
    provider: &Option<Arc<dyn LlmProvider>>,
    system_prompt: &Option<Vec<SystemBlock>>,
    query_event_tx: &mpsc::UnboundedSender<QueryEvent>,
    permission_mode: &PermissionMode,
    permission_rx: &mut Option<mpsc::UnboundedReceiver<PermissionAsk>>,
    question_rx: &mut Option<mpsc::UnboundedReceiver<UserQuestionAsk>>,
    mcp_manager: &McpConnectionManager,
    allow_rules: &[PermissionRule],
    deny_rules: &[PermissionRule],
    tools_filter: &Option<Vec<String>>,
    app_state: &Arc<tokio::sync::RwLock<crate::state::AppState>>,
    registry: &Arc<crate::services::api::registry::ProviderRegistry>,
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

    // Inject pending background agent results
    {
        let mut state = app_state.write().await;
        let pending = std::mem::take(&mut state.pending_agent_results);
        for result in pending {
            let status = if result.is_error { "failed" } else { "completed" };
            app.api_messages.push(ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: format!(
                        "[Background agent '{}' {}: {}]",
                        result.description, status, result.result
                    ),
                }],
            });
        }
    }

    app.is_loading = true;

    // Create permission channel for this query
    let (perm_tx, perm_rx) = mpsc::unbounded_channel();
    *permission_rx = Some(perm_rx);

    // Create question channel for this query
    let (q_tx, q_rx) = mpsc::unbounded_channel();
    *question_rx = Some(q_rx);

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
    let hooks = crate::services::hooks::load_all_hooks(&cwd);
    let ctx = crate::query::QueryContext {
        messages: app.api_messages.clone(),
        tools,
        system_prompt: system_prompt.clone(),
        model: app.active_model().to_string(),
        max_tokens: provider.model_info().max_output_tokens,
        permission_ctx: ToolPermissionContext {
            mode: permission_mode.clone(),
            always_allow_rules: allow_rules.to_vec(),
            always_deny_rules: deny_rules.to_vec(),
            ..Default::default()
        },
        permission_tx: perm_tx,
        question_tx: q_tx,
        hooks,
        cwd,
        max_turns: None,       // TUI mode: no turn limit
        max_budget_usd: None,  // TUI mode: no budget limit
        commands: app.command_registry.clone(),
        provider: provider.clone(),
        registry: Some(registry.clone()),
        app_state: app_state.clone(),
        mcp_manager: None,
        agent_depth: None,
        query_source: None,
    };

    let cancel = tokio_util::sync::CancellationToken::new();

    // Spawn query as background task
    let event_tx = query_event_tx.clone();
    tokio::spawn(async move {
        match crate::query::engine::query(ctx, event_tx.clone(), cancel).await {
            Ok(result) => {
                // Send full message history back so TUI can sync api_messages
                // (includes tool_use/tool_result that would otherwise be lost)
                let _ = event_tx.send(crate::query::QueryEvent::QueryCompleted {
                    messages: result.messages,
                });
            }
            Err(e) => {
                let _ = event_tx.send(crate::query::QueryEvent::Error(format!("Query failed: {}", e)));
            }
        }
    });
}

/// Submit a skill/prompt command to the LLM. Unlike submit_query, this does NOT
/// display the full prompt text in chat — only shows the skill name.
/// The full prompt is sent to the LLM as a user message but hidden from the UI.
#[allow(clippy::too_many_arguments)]
async fn submit_query_skill(
    app: &mut App,
    prompt_text: &str,
    _skill_name: &str,
    provider: &Option<Arc<dyn LlmProvider>>,
    system_prompt: &Option<Vec<SystemBlock>>,
    query_event_tx: &mpsc::UnboundedSender<QueryEvent>,
    permission_mode: &PermissionMode,
    permission_rx: &mut Option<mpsc::UnboundedReceiver<PermissionAsk>>,
    question_rx: &mut Option<mpsc::UnboundedReceiver<UserQuestionAsk>>,
    mcp_manager: &McpConnectionManager,
    allow_rules: &[PermissionRule],
    deny_rules: &[PermissionRule],
    tools_filter: &Option<Vec<String>>,
    app_state: &Arc<tokio::sync::RwLock<crate::state::AppState>>,
    registry: &Arc<crate::services::api::registry::ProviderRegistry>,
) {
    let Some(provider) = provider.clone() else {
        app.loading_label = None;
        app.push_message(DisplayMessage {
            role: DisplayRole::System,
            content: "No LLM provider connected. Use --provider or --api-base.".into(),
            tool_name: None,
        });
        return;
    };

    // Add prompt to API messages (sent to LLM) but NOT to display messages
    // The user already sees "/{skill_name}" in chat — no need to show the full prompt
    app.api_messages.push(ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: prompt_text.to_string(),
        }],
    });

    app.is_loading = true;

    // Create permission channel for this query
    let (perm_tx, perm_rx_new) = mpsc::unbounded_channel();
    *permission_rx = Some(perm_rx_new);

    // Create question channel for this query
    let (q_tx, q_rx) = mpsc::unbounded_channel();
    *question_rx = Some(q_rx);

    // Gather tools: built-in + MCP
    let mut tools = get_builtin_tools();
    tools.extend(mcp_manager.get_all_tools().await);

    if let Some(ref filter) = tools_filter {
        if filter.is_empty() || (filter.len() == 1 && filter[0].is_empty()) {
            tools.clear();
        } else if !filter.contains(&"default".to_string()) {
            tools.retain(|t| filter.iter().any(|f| t.name() == f));
        }
    }

    let cwd = std::env::current_dir().unwrap_or_default();
    let hooks = crate::services::hooks::load_all_hooks(&cwd);
    let ctx = crate::query::QueryContext {
        messages: app.api_messages.clone(),
        tools,
        system_prompt: system_prompt.clone(),
        model: app.active_model().to_string(),
        max_tokens: provider.model_info().max_output_tokens,
        permission_ctx: ToolPermissionContext {
            mode: permission_mode.clone(),
            always_allow_rules: allow_rules.to_vec(),
            always_deny_rules: deny_rules.to_vec(),
            ..Default::default()
        },
        permission_tx: perm_tx,
        question_tx: q_tx,
        hooks,
        cwd,
        max_turns: None,
        max_budget_usd: None,
        commands: app.command_registry.clone(),
        provider: provider.clone(),
        registry: Some(registry.clone()),
        app_state: app_state.clone(),
        mcp_manager: None,
        agent_depth: None,
        query_source: None,
    };

    let cancel = tokio_util::sync::CancellationToken::new();
    let event_tx = query_event_tx.clone();
    tokio::spawn(async move {
        match crate::query::engine::query(ctx, event_tx.clone(), cancel).await {
            Ok(result) => {
                let _ = event_tx.send(crate::query::QueryEvent::QueryCompleted {
                    messages: result.messages,
                });
            }
            Err(e) => {
                let _ = event_tx.send(crate::query::QueryEvent::Error(format!("Query failed: {}", e)));
            }
        }
    });
}
