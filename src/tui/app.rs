use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::commands::CommandRegistry;
use crate::query::{PermissionReply, QueryEvent};
use crate::services::file_index::FileIndex;
use crate::services::session_transcript::SessionTranscript;
use crate::types::message::{ApiMessage, Role};

/// State for a currently active TUI command overlay.
pub struct ActiveCommand {
    pub command_name: String,
    pub state: Box<dyn std::any::Any + Send>,
}

/// Application state
pub struct App {
    /// Unique session identifier
    pub session_id: String,
    /// When the session was created
    pub created_at: chrono::DateTime<chrono::Utc>,
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
    /// Current input mode (normal text or permission prompt)
    pub input_mode: InputMode,
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
    pub command_registry: Arc<CommandRegistry>,
    /// Currently active TUI command overlay
    pub active_command: Option<ActiveCommand>,
    /// Typeahead state (for '/' commands or '@' file mentions)
    pub typeahead: Option<TypeaheadState>,
    /// Working directory for file suggestions
    pub cwd: PathBuf,
    /// File index for fast @ mention search (built lazily on first use)
    pub file_index: Option<FileIndex>,
    /// Disable session persistence
    pub no_session_persistence: bool,
    /// Model override (set by /model command for hot-swap)
    pub model_override: Option<String>,
    /// Active output style name (e.g. "default", "concise", etc.)
    pub output_style: String,
    /// Loading spinner label (e.g. "Working..." or "brainstorming 스킬 사용")
    pub loading_label: Option<String>,
}

/// What kind of typeahead is active
#[derive(Debug, Clone, PartialEq)]
pub enum TypeaheadKind {
    /// Slash command typeahead (triggered by '/')
    Command,
    /// File mention typeahead (triggered by '@')
    FileMention {
        /// Position in input where the '@' token starts
        token_start: usize,
        /// The query text after '@'
        query: String,
    },
}

/// Typeahead autocomplete state
pub struct TypeaheadState {
    /// What triggered the typeahead
    pub kind: TypeaheadKind,
    /// Filtered entries: (display_name, description)
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

/// What the input area is currently showing
pub enum InputMode {
    /// Normal text input
    Normal,
    /// Permission prompt inline in chat
    Permission {
        tool_name: String,
        description: String,
        reply_tx: Option<tokio::sync::oneshot::Sender<PermissionReply>>,
    },
    /// User question prompt inline
    Question {
        options: Vec<(String, Option<String>)>, // (label, description)
        selected: usize,
        text_input: String,
        reply_tx: Option<tokio::sync::oneshot::Sender<crate::query::types::UserQuestionReply>>,
    },
}

impl App {
    pub fn new(model: String, command_registry: Arc<CommandRegistry>) -> Self {
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
            session_id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now(),
            messages,
            input: String::new(),
            cursor_pos: 0,
            should_quit: false,
            is_loading: false,
            scroll_offset: 0,
            status: String::new(),
            model,
            input_mode: InputMode::Normal,
            api_messages: Vec::new(),
            start_time: Instant::now(),
            has_provider,
            token_count: 0,
            session_cost: 0.0,
            command_registry,
            active_command: None,
            typeahead: None,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            file_index: None,
            no_session_persistence: false,
            model_override: None,
            output_style: "default".to_string(),
            loading_label: None,
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
                let byte_pos = char_to_byte_pos(&self.input, self.cursor_pos);
                self.input.insert(byte_pos, c);
                self.cursor_pos += 1;
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    let byte_pos = char_to_byte_pos(&self.input, self.cursor_pos);
                    self.input.remove(byte_pos);
                }
            }
            KeyCode::Delete => {
                let char_count = self.input.chars().count();
                if self.cursor_pos < char_count {
                    let byte_pos = char_to_byte_pos(&self.input, self.cursor_pos);
                    self.input.remove(byte_pos);
                }
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                let char_count = self.input.chars().count();
                if self.cursor_pos < char_count {
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
            }
            KeyCode::End => {
                self.cursor_pos = self.input.chars().count();
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

    /// Get the active model (override or default)
    pub fn active_model(&self) -> &str {
        self.model_override.as_deref().unwrap_or(&self.model)
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
        // Check for '/' command typeahead
        if self.input.starts_with('/') && !self.input.contains(char::is_whitespace) {
            self.update_command_typeahead();
            return;
        }

        // Check for '@' file mention typeahead
        if let Some((token_start, query)) = find_at_token(&self.input, self.cursor_pos) {
            self.update_file_typeahead(token_start, &query);
            return;
        }

        self.typeahead = None;
    }

    fn update_command_typeahead(&mut self) {
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
                kind: TypeaheadKind::Command,
                selected: prev_selected.min(items.len().saturating_sub(1)),
                items,
            });
        }
    }

    /// Build the file index lazily on first @ trigger using git ls-files.
    pub fn ensure_file_index(&mut self) {
        if self.file_index.is_some() {
            return;
        }
        if let Ok(output) = std::process::Command::new("git")
            .args(["ls-files", "--cached", "--others", "--exclude-standard"])
            .current_dir(&self.cwd)
            .output()
        {
            if output.status.success() {
                let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(|s| s.to_string())
                    .collect();
                let mut index = FileIndex::new();
                index.load_from_file_list(files);
                self.file_index = Some(index);
            }
        }
    }

    fn update_file_typeahead(&mut self, token_start: usize, query: &str) {
        self.ensure_file_index();

        let items = if let Some(ref index) = self.file_index {
            let results = index.search(query, 15);
            results
                .into_iter()
                .map(|r| {
                    let desc = if r.path.ends_with('/') {
                        "dir".to_string()
                    } else {
                        Path::new(&r.path)
                            .extension()
                            .map(|e| e.to_string_lossy().to_string())
                            .unwrap_or_else(|| "file".to_string())
                    };
                    (r.path, desc)
                })
                .collect()
        } else {
            get_file_suggestions(&self.cwd, query)
        };

        if items.is_empty() {
            self.typeahead = None;
        } else {
            let prev_selected = self.typeahead.as_ref().map(|t| t.selected).unwrap_or(0);
            self.typeahead = Some(TypeaheadState {
                kind: TypeaheadKind::FileMention {
                    token_start,
                    query: query.to_string(),
                },
                selected: prev_selected.min(items.len().saturating_sub(1)),
                items,
            });
        }
    }

    /// Accept the currently selected typeahead item, filling the input.
    pub fn accept_typeahead(&mut self) {
        if let Some(ta) = &self.typeahead {
            if let Some((name, _)) = ta.items.get(ta.selected) {
                match &ta.kind {
                    TypeaheadKind::Command => {
                        self.input = format!("/{}", name);
                        self.cursor_pos = self.input.chars().count();
                    }
                    TypeaheadKind::FileMention { token_start, .. } => {
                        let start = *token_start;
                        // Replace @query with @filename (add space after)
                        let needs_quote = name.contains(' ');
                        let replacement = if needs_quote {
                            format!("@\"{}\" ", name)
                        } else {
                            format!("@{} ", name)
                        };
                        // Find end of current @ token
                        let after_at = &self.input[start..];
                        let token_end = start + at_token_len(after_at);
                        self.input.replace_range(start..token_end, &replacement);
                        self.cursor_pos = start + replacement.chars().count();
                    }
                }
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

    /// Build a `SessionTranscript` from the current app state.
    pub fn build_transcript(&self) -> SessionTranscript {
        SessionTranscript {
            session_id: self.session_id.clone(),
            messages: self.api_messages.clone(),
            model: self.model.clone(),
            created_at: self.created_at,
            updated_at: chrono::Utc::now(),
        }
    }

    /// Restore display messages from a loaded transcript's API messages.
    pub fn restore_display_messages(api_messages: &[ApiMessage]) -> Vec<DisplayMessage> {
        api_messages
            .iter()
            .filter_map(|msg| {
                let content = msg
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        crate::types::content::ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                if content.is_empty() {
                    return None;
                }

                let role = match msg.role {
                    Role::User => DisplayRole::User,
                    Role::Assistant => DisplayRole::Assistant,
                    _ => DisplayRole::System,
                };

                Some(DisplayMessage {
                    role,
                    content,
                    tool_name: None,
                })
            })
            .collect()
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
                // Add assistant response to api_messages for conversation continuity
                if let Some(last) = self.messages.last() {
                    if last.role == DisplayRole::Assistant {
                        // Estimate tokens if provider didn't send usage
                        if self.token_count == 0 {
                            let estimated = crate::services::token_estimation::estimate_tokens(&last.content);
                            self.token_count = estimated;
                        }
                        self.api_messages.push(ApiMessage {
                            role: Role::Assistant,
                            content: vec![crate::types::content::ContentBlock::Text {
                                text: last.content.clone(),
                            }],
                        });
                    }
                }
            }
            QueryEvent::Usage { input_tokens, output_tokens } => {
                self.token_count += input_tokens + output_tokens;
                // Simple cost estimate: ~$0.01 per 1K tokens (rough average)
                self.session_cost += (input_tokens + output_tokens) as f64 * 0.00001;
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

/// Convert a char index to a byte position in a string.
fn char_to_byte_pos(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte_pos, _)| byte_pos)
        .unwrap_or(s.len())
}

/// Find the '@' token at or before the cursor position (char index).
/// Returns (byte_start_index, query_after_at) if found.
fn find_at_token(input: &str, cursor_pos: usize) -> Option<(usize, String)> {
    // Convert char-based cursor_pos to byte offset
    let byte_cursor = char_to_byte_pos(input, cursor_pos);
    // Look backwards from cursor to find '@'
    let before_cursor = &input[..byte_cursor];

    // Find the last '@' that's either at start or preceded by whitespace
    let mut at_pos = None;
    for (i, c) in before_cursor.char_indices().rev() {
        if c == '@' && (i == 0 || before_cursor[..i].ends_with(char::is_whitespace)) {
            at_pos = Some(i);
            break;
        }
        // Stop if we hit whitespace before finding '@'
        if c.is_whitespace() {
            break;
        }
    }

    let start = at_pos?;
    let after_at = &input[start + 1..]; // text after '@'

    // Extract the query: everything until whitespace (or end), supporting quoted paths
    let query = if let Some(stripped) = after_at.strip_prefix('"') {
        // Quoted path: @"some path" — extract until closing quote
        let end = stripped.find('"').unwrap_or(stripped.len());
        stripped[..end].to_string()
    } else {
        // Unquoted: take until whitespace
        let end = after_at
            .find(char::is_whitespace)
            .unwrap_or(after_at.len());
        after_at[..end].to_string()
    };

    Some((start, query))
}

/// Get the length of an @ token in the input (from '@' to end of token)
fn at_token_len(s: &str) -> usize {
    if !s.starts_with('@') {
        return 0;
    }
    let after_at = &s[1..];
    if after_at.starts_with('"') {
        // Quoted: find closing quote
        1 + after_at
            .find('"')
            .map(|i| {
                // Check if there's a second quote (closing)
                after_at[i + 1..]
                    .find('"')
                    .map(|j| i + 1 + j + 1 + 1) // @"..."
                    .unwrap_or(after_at.len() + 1) // @"... (unclosed)
            })
            .unwrap_or(after_at.len() + 1)
    } else {
        1 + after_at
            .find(char::is_whitespace)
            .unwrap_or(after_at.len())
    }
}

/// Get file suggestions matching a query in the given directory.
/// Returns up to 15 results as (relative_path, file_type_description).
fn get_file_suggestions(cwd: &Path, query: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();

    // Use ignore crate to respect .gitignore
    let walker = ignore::WalkBuilder::new(cwd)
        .max_depth(Some(4))
        .hidden(true) // skip hidden files
        .build();

    let query_lower = query.to_lowercase();

    for entry in walker.flatten() {
        if results.len() >= 15 {
            break;
        }

        let path = entry.path();
        if path == cwd {
            continue;
        }

        let relative = path
            .strip_prefix(cwd)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        if relative.is_empty() {
            continue;
        }

        let rel_lower = relative.to_lowercase();

        // Match: prefix, contains, or path component match
        let matches = if query.is_empty() {
            // Show top-level entries when '@' is typed with no query
            !relative.contains(std::path::MAIN_SEPARATOR)
                || relative.matches(std::path::MAIN_SEPARATOR).count() <= 1
        } else {
            rel_lower.starts_with(&query_lower)
                || rel_lower.contains(&query_lower)
                || path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().to_lowercase().contains(&query_lower))
        };

        if matches {
            let desc = if path.is_dir() {
                "dir".to_string()
            } else {
                path.extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".to_string())
            };
            let display = if path.is_dir() {
                format!("{}/", relative)
            } else {
                relative
            };
            results.push((display, desc));
        }
    }

    // Sort: directories first, then by name
    results.sort_by(|a, b| {
        let a_is_dir = a.0.ends_with('/');
        let b_is_dir = b.0.ends_with('/');
        b_is_dir.cmp(&a_is_dir).then(a.0.cmp(&b.0))
    });

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(model: &str) -> App {
        App::new(model.into(), Arc::new(CommandRegistry::empty()))
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
        let reg = Arc::new(CommandRegistry::new());
        let app = App::new("test".into(), reg);
        assert!(app.command_registry.has("clear"));
        assert!(app.command_registry.has("help"));
        assert!(app.command_registry.has("review"));
    }

    // --- @ mention tests ---

    #[test]
    fn test_find_at_token_simple() {
        let result = find_at_token("@file", 5);
        assert_eq!(result, Some((0, "file".to_string())));
    }

    #[test]
    fn test_find_at_token_mid_input() {
        let result = find_at_token("look at @src/main", 17);
        assert_eq!(result, Some((8, "src/main".to_string())));
    }

    #[test]
    fn test_find_at_token_no_at() {
        assert_eq!(find_at_token("no at here", 10), None);
    }

    #[test]
    fn test_find_at_token_at_start_empty() {
        let result = find_at_token("@", 1);
        assert_eq!(result, Some((0, String::new())));
    }

    #[test]
    fn test_find_at_token_after_space() {
        let result = find_at_token("check @foo.rs please", 13);
        assert_eq!(result, Some((6, "foo.rs".to_string())));
    }

    #[test]
    fn test_find_at_token_not_at_boundary() {
        // email@example.com — '@' not preceded by whitespace
        assert_eq!(find_at_token("email@example.com", 17), None);
    }

    #[test]
    fn test_at_token_len_simple() {
        // "@file.rs" = 1('@') + 7("file.rs") = 8
        assert_eq!(at_token_len("@file.rs rest"), 8);
    }

    #[test]
    fn test_at_token_len_to_end() {
        assert_eq!(at_token_len("@file.rs"), 8);
    }
}
