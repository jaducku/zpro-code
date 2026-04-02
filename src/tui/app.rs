use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::query::QueryEvent;
use crate::types::message::ApiMessage;

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
    pub fn new(model: String) -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            cursor_pos: 0,
            should_quit: false,
            is_loading: false,
            scroll_offset: 0,
            status: format!("Model: {}", model),
            model,
            permission_dialog: None,
            api_messages: Vec::new(),
        }
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('d') => {
                    self.should_quit = true;
                }
                _ => {}
            }
            return;
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

    #[test]
    fn test_app_creation() {
        let app = App::new("test-model".into());
        assert!(app.messages.is_empty());
        assert!(app.input.is_empty());
        assert!(!app.should_quit);
    }

    #[test]
    fn test_handle_char_input() {
        let mut app = App::new("test".into());
        app.handle_key(KeyEvent::from(KeyCode::Char('h')));
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        assert_eq!(app.input, "hi");
        assert_eq!(app.cursor_pos, 2);
    }

    #[test]
    fn test_handle_backspace() {
        let mut app = App::new("test".into());
        app.input = "hello".into();
        app.cursor_pos = 5;
        app.handle_key(KeyEvent::from(KeyCode::Backspace));
        assert_eq!(app.input, "hell");
    }

    #[test]
    fn test_take_input() {
        let mut app = App::new("test".into());
        app.input = "test input".into();
        app.cursor_pos = 10;
        let input = app.take_input();
        assert_eq!(input, "test input");
        assert!(app.input.is_empty());
        assert_eq!(app.cursor_pos, 0);
    }

    #[test]
    fn test_text_delta_streaming() {
        let mut app = App::new("test".into());
        app.handle_query_event(QueryEvent::TextDelta("Hello ".into()));
        app.handle_query_event(QueryEvent::TextDelta("world".into()));
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].content, "Hello world");
    }

    #[test]
    fn test_ctrl_c_quits() {
        let mut app = App::new("test".into());
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }
}
