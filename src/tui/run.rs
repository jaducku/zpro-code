use std::io;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use super::app::{App, DisplayMessage, DisplayRole};
use super::ui;
use crate::commands::registry::CommandRegistry;
use crate::commands::traits::Command;
use crate::commands::types::CommandAction;
use crate::query::QueryEvent;
use crate::services::api::provider::LlmProvider;
use crate::services::api::types::SystemBlock;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};

/// Run the TUI application
pub async fn run_app(
    provider: Option<Box<dyn LlmProvider>>,
    _tools: Vec<Box<dyn crate::tool::Tool>>,
    system_prompt: Option<Vec<SystemBlock>>,
    command_registry: CommandRegistry,
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

    let (_query_event_tx, mut query_event_rx) = mpsc::unbounded_channel::<QueryEvent>();

    loop {
        // Render
        terminal.draw(|frame| {
            if let Some(active) = &mut app.active_command {
                // Route rendering to the active TUI command
                if let Some(Command::Tui(tui_cmd)) = app.command_registry.find(&active.command_name) {
                    tui_cmd.render(frame, frame.area(), active.state.as_mut());
                }
            } else {
                ui::render(frame, &app);
            }
        })?;

        // Handle events with timeout for non-blocking
        tokio::select! {
            // Terminal events (keyboard, mouse, resize)
            _ = async {
                if event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                    if let Ok(ev) = event::read() {
                        match ev {
                            Event::Key(key) => {
                                // If a TUI command is active, route keys to it
                                if app.active_command.is_some() {
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
                                                app.cursor_pos = app.input.len();
                                                app.active_command = None;
                                            }
                                            CommandAction::Continue => {}
                                        }
                                    }
                                } else if key.code == KeyCode::Enter && !app.is_loading && !app.input.is_empty() {
                                    // If typeahead is open, accept it first; then submit
                                    if app.typeahead.is_some() {
                                        app.accept_typeahead();
                                    }
                                    let input = app.take_input();
                                    handle_submit(&mut app, &input, &model, &system_prompt);
                                } else {
                                    let typeahead_accepted = app.handle_key(key);
                                    if typeahead_accepted && !app.is_loading && !app.input.is_empty() {
                                        // Typeahead accepted via Enter — submit immediately
                                        let input = app.take_input();
                                        handle_submit(&mut app, &input, &model, &system_prompt);
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
            } => {}

            // Query events (streaming responses)
            Some(event) = query_event_rx.recv() => {
                app.handle_query_event(event);
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

fn handle_submit(
    app: &mut App,
    input: &str,
    model: &str,
    system_prompt: &Option<Vec<SystemBlock>>,
) {
    // Check for slash commands
    if let Some((name, args)) = App::parse_slash_command(input) {
        dispatch_slash_command(app, &name, &args);
        return;
    }

    submit_query(app, input, model, system_prompt);
}

/// What kind of command was found, with any data we need extracted before
/// releasing the borrow on the registry.
enum FoundCommand {
    Local { description: String, is_clear: bool },
    Tui,
    Prompt { progress: String },
    NotFound,
}

fn dispatch_slash_command(app: &mut App, name: &str, args: &str) {
    // First pass: identify the command kind and extract needed data without
    // holding a mutable borrow on app.
    let found = match app.command_registry.find(name) {
        Some(Command::Local(cmd)) => {
            let meta = cmd.meta();
            let is_clear = meta.name == "clear" || meta.aliases.contains(&name);
            FoundCommand::Local {
                description: meta.description.to_string(),
                is_clear,
            }
        }
        Some(Command::Tui(_)) => FoundCommand::Tui,
        Some(Command::Prompt(cmd)) => FoundCommand::Prompt {
            progress: cmd.progress_message().to_string(),
        },
        None => FoundCommand::NotFound,
    };

    // Second pass: act on the result with full mutable access to app.
    match found {
        FoundCommand::Local { description, is_clear } => {
            app.push_message(DisplayMessage {
                role: DisplayRole::User,
                content: format!("/{}{}", name, if args.is_empty() { String::new() } else { format!(" {}", args) }),
                tool_name: None,
            });

            if is_clear {
                app.messages.clear();
                app.api_messages.clear();
                app.push_message(DisplayMessage {
                    role: DisplayRole::System,
                    content: "Conversation cleared.".to_string(),
                    tool_name: None,
                });
            } else {
                app.push_message(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("/{}: {} (executed)", name, description),
                    tool_name: None,
                });
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

fn submit_query(
    app: &mut App,
    input: &str,
    model: &str,
    _system_prompt: &Option<Vec<SystemBlock>>,
) {
    // Add user message to display
    app.push_message(DisplayMessage {
        role: DisplayRole::User,
        content: input.to_string(),
        tool_name: None,
    });

    // Add to API messages
    app.api_messages.push(ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: input.to_string(),
        }],
    });

    app.is_loading = true;

    // The actual query execution would be spawned as a background task
    // For the skeleton, we just set loading state
    // tokio::spawn(async move { query(ctx, provider, event_tx, cancel).await });
    app.status = format!("Model: {} | Thinking...", model);
}
