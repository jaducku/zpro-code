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
use crate::query::QueryEvent;
use crate::services::api::provider::LlmProvider;
use crate::services::api::types::SystemBlock;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};

/// Run the TUI application
pub async fn run_app(
    provider: Box<dyn LlmProvider>,
    _tools: Vec<Box<dyn crate::tool::Tool>>,
    system_prompt: Option<Vec<SystemBlock>>,
) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let model = provider.model_info().id.clone();
    let mut app = App::new(model.clone());

    let (_query_event_tx, mut query_event_rx) = mpsc::unbounded_channel::<QueryEvent>();

    loop {
        // Render
        terminal.draw(|frame| ui::render(frame, &app))?;

        // Handle events with timeout for non-blocking
        tokio::select! {
            // Terminal events (keyboard, mouse, resize)
            _ = async {
                if event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                    if let Ok(ev) = event::read() {
                        match ev {
                            Event::Key(key) => {
                                if key.code == KeyCode::Enter && !app.is_loading && !app.input.is_empty() {
                                    let input = app.take_input();
                                    submit_query(
                                        &mut app,
                                        &input,
                                        &model,
                                        &system_prompt,
                                    );
                                } else {
                                    app.handle_key(key);
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
