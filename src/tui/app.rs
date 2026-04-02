use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Instant;

use crate::commands::CommandRegistry;
use crate::query::QueryEvent;
use crate::types::message::ApiMessage;

/// State for a currently active TUI command overlay.
pub struct ActiveCommand {
    pub command_name: String,
    pub state: Box<dyn std::any::Any + Send>,
}

/// Application state
pub struct App {
    /// Conversation messages for display
    pub messages: Vec<DisplayMessage>,
    /// Current input text
    pub input: String,
    /// Cursor position in input
    pub cursor_pos: usize,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Whether a query is currently running
    pub is_loading: bool,
    /// Scroll offset for message list
    pub scroll_offset: u16,
    /// Status bar text
    pub status: String,
    /// Current model name
    pub model: String,
    /// Permission dialog (if showing)
    pub permission_dialog: Option<PermissionDialog>,
    /// API messages (conversation history for LLM)
    pub api_messages: Vec<ApiMessage>,
    /// App start time (for animations)
    pub start_time: Instant,
    /// Whether provider is connected
    pub has_provider: bool,
    /// Number of tokens used
    pub token_count: u64,
    /// Session cost
    pub session_cost: f64,
    /// Command registry for slash commands
    pub command_registry: CommandRegistry,
    /// Currently active TUI command overlay
    pub active_command: Option<ActiveCommand>,
    /// Slash command typeahead state (shown when input starts with '/')
    pub typeahead: Option<TypeaheadState>,
}

/// Typeahead autocomplete state for slash commands
pub struct TypeaheadState {
    /// Filtered command entries: (name, description)
    pub items: Vec<(String, String)>,
    /// Currently selected index
    pub selected: usize,
}

/// A message for display purposes
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: DisplayRole,
    pub content: String,
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Permission dialog state
pub struct PermissionDialog {
    pub tool_name: String,
    pub description: String,
    pub selected: bool, // true = allow, false = deny
}

impl App {
    pub fn new(model: String, command_registry: CommandRegistry) -> Self {
        let has_provider = model != "(no provider)";
        let mut messages = vec![
            DisplayMessage {
                role: DisplayRole::System,
                content: format!("atom-code v{}", env!("CARGO_PKG_VERSION")),
                tool_name: None,
            },
        ];

        if has_provider {
            messages.push(DisplayMessage {
                role: DisplayRole::System,
                content: format!("Connected to model: {}", model),
                tool_name: None,
            });
        } else {
            messages.push(DisplayMessage {
                role: DisplayRole::System,
                content: "No LLM provider connected. Use --api-base to connect.".into(),
                tool_name: None,
            });
        }

        messages.push(DisplayMessage {
            role: DisplayRole::System,
            content: "Type a message and press Enter. Ctrl+C to quit.".into(),
            tool_name: None,
        });

        Self {
            messages,
            input: String::new(),
            cursor_pos: 0,
            should_quit: false,
            is_loading: false,
            scroll_offset: 0,
            status: String::new(),
            model,
            permission_dialog: None,
            api_messages: Vec::new(),
            start_time: Instant::now(),
            has_provider,
            token_count: 0,
            session_cost: 0.0,
            command_registry,
            active_command: None,
            typeahead: None,
        }
    }

    /// Handle a key event.
    /// Returns true if typeahead accepted a selection (caller should handle Enter).
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('d') => {
                    self.should_quit = true;
                }
                _ => {}
            }
            return false;
        }

        // When typeahead is active, Up/Down/Tab navigate, Enter/Tab accepts, Esc closes
        if self.typeahead.is_some() {
            match key.code {
                KeyCode::Up => {
                    if let Some(ta) = &mut self.typeahead {
                        ta.selected = ta.selected.saturating_sub(1);
                    }
                    return false;
                }
                KeyCode::Down => {
                    if let Some(ta) = &mut self.typeahead {
                        ta.selected = (ta.selected + 1).min(ta.items.len().saturating_sub(1));
                    }
                    return false;
                }
                KeyCode::Tab => {
                    self.accept_typeahead();
                    return false;
                }
                KeyCode::Enter => {
                    // Accept typeahead and signal caller to submit
                    self.accept_typeahead();
                    return true;
                }
                KeyCode::Esc => {
                    self.typeahead = None;
                    return false;
                }
                _ => {
                    // Fall through to normal input handling, then update typeahead
                }
            }
        }

        match key.code {
            KeyCode::Char(c) => {
                self.input.insert(self.cursor_pos, c);
                self.cursor_pos += 1;
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    self.input.remove(self.cursor_pos);
                }
            }
            KeyCode::Delete => {
                if self.cursor_pos < self.input.len() {
                    self.input.remove(self.cursor_pos);
                }
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_pos < self.input.len() {
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
            }
            KeyCode::End => {
                self.cursor_pos = self.input.len();
            }
            KeyCode::Up => {
                if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            }
            KeyCode::Down => {
                self.scroll_offset += 1;
            }
            KeyCode::Enter => {
                // Submit is handled by the main loop
            }
            _ => {}
        }

        // Update typeahead after any input change
        self.update_typeahead();
        false
    }

    /// Take the current input (clears it)
    pub fn take_input(&mut self) -> String {
        let input = self.input.clone();
        self.input.clear();
        self.cursor_pos = 0;
        input
    }

    /// Add a display message
    pub fn push_message(&mut self, msg: DisplayMessage) {
        self.messages.push(msg);
    }

    /// Update typeahead based on current input.
    /// Called after every input change.
    pub fn update_typeahead(&mut self) {
        if !self.input.starts_with('/') || self.input.contains(char::is_whitespace) {
            self.typeahead = None;
            return;
        }

        let query = &self.input[1..]; // strip '/'
        let available = self.command_registry.available_commands();

        let mut items: Vec<(String, String)> = available
            .iter()
            .filter(|cmd| {
                if !cmd.meta().is_hidden {
                    let name = cmd.name();
                    if query.is_empty() {
                        return true;
                    }
                    // Fuzzy prefix match on name and aliases
                    name.starts_with(query)
                        || cmd.meta().aliases.iter().any(|a| a.starts_with(query))
                        || name.contains(query)
                } else {
                    false
                }
            })
            .map(|cmd| {
                (cmd.name().to_string(), cmd.meta().description.to_string())
            })
            .collect();

        items.sort_by(|a, b| a.0.cmp(&b.0));
        items.dedup_by(|a, b| a.0 == b.0);

        if items.is_empty() {
            self.typeahead = None;
        } else {
            let prev_selected = self.typeahead.as_ref().map(|t| t.selected).unwrap_or(0);
            self.typeahead = Some(TypeaheadState {
                selected: prev_selected.min(items.len().saturating_sub(1)),
                items,
            });
        }
    }

    /// Accept the currently selected typeahead item, filling the input.
    pub fn accept_typeahead(&mut self) {
        if let Some(ta) = &self.typeahead {
            if let Some((name, _)) = ta.items.get(ta.selected) {
                self.input = format!("/{}", name);
                self.cursor_pos = self.input.len();
            }
        }
        self.typeahead = None;
    }

    /// Parse a slash command from input, returning (name, args) if it starts with '/'.
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

    /// Handle a query event (streaming update)
    pub fn handle_query_event(&mut self, event: QueryEvent) {
        match event {
            QueryEvent::TextDelta(text) => {
                if let Some(last) = self.messages.last_mut() {
                    if last.role == DisplayRole::Assistant {
                        last.content.push_str(&text);
                        return;
                    }
                }
                self.messages.push(DisplayMessage {
                    role: DisplayRole::Assistant,
                    content: text,
                    tool_name: None,
                });
            }
            QueryEvent::ToolCallStart { name, .. } => {
                self.messages.push(DisplayMessage {
                    role: DisplayRole::Tool,
                    content: format!("Running {}...", name),
                    tool_name: Some(name),
                });
            }
            QueryEvent::ToolCallComplete { result, .. } => {
                if let Some(last) = self.messages.last_mut() {
                    if last.role == DisplayRole::Tool {
                        last.content = result.content;
                    }
                }
            }
            QueryEvent::ResponseComplete { .. } => {
                self.is_loading = false;
            }
            QueryEvent::Error(msg) => {
                self.messages.push(DisplayMessage {
                    role: DisplayRole::System,
                    content: format!("Error: {}", msg),
                    tool_name: None,
                });
                self.is_loading = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(model: &str) -> App {
        App::new(model.into(), CommandRegistry::empty())
    }

    #[test]
    fn test_app_creation() {
        let app = test_app("test-model");
        assert!(!app.messages.is_empty()); // has welcome messages
        assert!(app.input.is_empty());
        assert!(!app.should_quit);
    }

    #[test]
    fn test_handle_char_input() {
        let mut app = test_app("test");
        app.handle_key(KeyEvent::from(KeyCode::Char('h')));
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        assert_eq!(app.input, "hi");
        assert_eq!(app.cursor_pos, 2);
    }

    #[test]
    fn test_handle_backspace() {
        let mut app = test_app("test");
        app.input = "hello".into();
        app.cursor_pos = 5;
        app.handle_key(KeyEvent::from(KeyCode::Backspace));
        assert_eq!(app.input, "hell");
    }

    #[test]
    fn test_take_input() {
        let mut app = test_app("test");
        app.input = "test input".into();
        app.cursor_pos = 10;
        let input = app.take_input();
        assert_eq!(input, "test input");
        assert!(app.input.is_empty());
        assert_eq!(app.cursor_pos, 0);
    }

    #[test]
    fn test_text_delta_streaming() {
        let mut app = test_app("test");
        let initial_count = app.messages.len();
        app.handle_query_event(QueryEvent::TextDelta("Hello ".into()));
        app.handle_query_event(QueryEvent::TextDelta("world".into()));
        assert_eq!(app.messages.len(), initial_count + 1);
        assert_eq!(app.messages.last().unwrap().content, "Hello world");
    }

    #[test]
    fn test_ctrl_c_quits() {
        let mut app = test_app("test");
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn test_parse_slash_command_simple() {
        let result = App::parse_slash_command("/help");
        assert_eq!(result, Some(("help".into(), String::new())));
    }

    #[test]
    fn test_parse_slash_command_with_args() {
        let result = App::parse_slash_command("/review some args here");
        assert_eq!(result, Some(("review".into(), "some args here".into())));
    }

    #[test]
    fn test_parse_slash_command_with_leading_whitespace() {
        let result = App::parse_slash_command("  /clear  ");
        assert_eq!(result, Some(("clear".into(), String::new())));
    }

    #[test]
    fn test_parse_slash_command_not_slash() {
        assert_eq!(App::parse_slash_command("hello"), None);
        assert_eq!(App::parse_slash_command(""), None);
        assert_eq!(App::parse_slash_command("  no slash"), None);
    }

    #[test]
    fn test_parse_slash_command_args_trimmed() {
        let result = App::parse_slash_command("/cmd   arg1  arg2  ");
        assert_eq!(result, Some(("cmd".into(), "arg1  arg2".into())));
    }

    #[test]
    fn test_app_has_registry() {
        let reg = CommandRegistry::new();
        let app = App::new("test".into(), reg);
        assert!(app.command_registry.has("clear"));
        assert!(app.command_registry.has("help"));
        assert!(app.command_registry.has("review"));
    }
}
