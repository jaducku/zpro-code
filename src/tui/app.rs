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

/// Tracks a tool that is currently executing (not yet in messages)
#[derive(Debug, Clone)]
pub struct RunningToolInfo {
    pub id: String,
    pub name: String,
    pub input_summary: String,
    pub input_full: Option<String>,
    pub start_time: Instant,
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
    /// Input queued for submission after current query completes
    pub queued_input: Option<String>,
    /// When the current query started (for elapsed time in Thinking state)
    pub query_start_time: Option<Instant>,
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
    /// Force full terminal redraw on next frame (e.g. after /clear)
    pub needs_redraw: bool,
    /// Hit areas for mouse click detection on collapsible messages
    pub hit_areas: Vec<HitArea>,
    /// Hit areas for mouse click detection on "+N more" expand lines
    pub expand_hit_areas: Vec<HitArea>,
    /// Active query's abort signal (Some only while query is running)
    pub active_abort_signal: Option<crate::utils::abort_signal::AbortSignal>,
    /// Ctrl+C double-press detection timer
    pub last_ctrl_c_press: Option<Instant>,
    /// Ctrl+D double-press detection timer
    pub last_ctrl_d_press: Option<Instant>,
    /// Input history for Up/Down navigation
    pub history: crate::tui::history::InputHistory,
    /// Context window size (tokens) from provider's ModelInfo
    pub context_window: u64,
    /// Human-readable permission mode label
    pub permission_mode_label: String,
    /// Aggregated activity summary for running tools
    pub activity: ActivitySummary,
    /// Tools currently executing (not yet displayed in messages)
    pub running_tools: std::collections::HashMap<String, RunningToolInfo>,
    /// When the streaming response started (for elapsed timer)
    pub response_start_time: Option<Instant>,
    /// Output tokens in the current response
    pub response_token_count: u64,
    /// Cumulative output tokens across all responses in the current query
    pub cumulative_output_tokens: u64,
    /// Current todo list (synced from AppState before each render)
    pub todos: Vec<crate::state::app_state::TodoItem>,
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
pub enum DisplayMessage {
    Text {
        role: TextRole,
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        input_summary: String,
        input_full: Option<String>,
        result: Option<String>,
        is_error: bool,
        collapsed: bool,
        parent_id: Option<String>,
    },
    Agent {
        agent_id: String,
        agent_type: String,
        description: String,
        status: AgentStatus,
        result: Option<String>,
        collapsed: bool,
        tool_count: u32,
        token_count: u64,
        duration_secs: u64,
        child_tool_ids: Vec<String>,
        visible_child_count: usize,
    },
    Thinking {
        content: String,
        token_count: u64,
        is_complete: bool,
        collapsed: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Running,
    Completed,
    Failed,
}

pub struct HitArea {
    pub y: u16,
    pub message_idx: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolCategory {
    Read,
    Search,
    Edit,
    Command,
    Agent,
    Skill,
    Other,
}

#[derive(Debug, Clone, Default)]
pub struct ActivitySummary {
    pub read_count: u32,
    pub search_count: u32,
    pub edit_count: u32,
    pub command_count: u32,
    pub agent_count: u32,
    pub skill_count: u32,
    pub other_count: u32,
    pub last_detail: String,
    pub tool_categories: std::collections::HashMap<String, ToolCategory>,
}

impl ActivitySummary {
    pub fn add(&mut self, id: &str, tool_name: &str) {
        let cat = classify_tool(tool_name);
        self.tool_categories.insert(id.to_string(), cat);
        self.increment(cat);
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(cat) = self.tool_categories.remove(id) {
            self.decrement(cat);
        }
    }

    pub fn total_active(&self) -> u32 {
        self.read_count
            + self.search_count
            + self.edit_count
            + self.command_count
            + self.agent_count
            + self.skill_count
            + self.other_count
    }

    pub fn format_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.search_count > 0 {
            parts.push(format!(
                "searching for {} pattern{}",
                self.search_count,
                if self.search_count > 1 { "s" } else { "" }
            ));
        }
        if self.read_count > 0 {
            parts.push(format!(
                "reading {} file{}",
                self.read_count,
                if self.read_count > 1 { "s" } else { "" }
            ));
        }
        if self.edit_count > 0 {
            parts.push(format!(
                "editing {} file{}",
                self.edit_count,
                if self.edit_count > 1 { "s" } else { "" }
            ));
        }
        if self.command_count > 0 {
            parts.push("running command".to_string());
        }
        if self.agent_count > 0 {
            parts.push("running agent".to_string());
        }
        if self.skill_count > 0 {
            parts.push("loading skill".to_string());
        }
        if self.other_count > 0 {
            parts.push(format!(
                "{} tool{}",
                self.other_count,
                if self.other_count > 1 { "s" } else { "" }
            ));
        }
        if parts.is_empty() {
            "Working".to_string()
        } else {
            let mut s = parts.join(", ");
            // Capitalize first char
            if let Some(first) = s.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            s
        }
    }

    fn increment(&mut self, cat: ToolCategory) {
        match cat {
            ToolCategory::Read => self.read_count += 1,
            ToolCategory::Search => self.search_count += 1,
            ToolCategory::Edit => self.edit_count += 1,
            ToolCategory::Command => self.command_count += 1,
            ToolCategory::Agent => self.agent_count += 1,
            ToolCategory::Skill => self.skill_count += 1,
            ToolCategory::Other => self.other_count += 1,
        }
    }

    fn decrement(&mut self, cat: ToolCategory) {
        match cat {
            ToolCategory::Read => self.read_count = self.read_count.saturating_sub(1),
            ToolCategory::Search => self.search_count = self.search_count.saturating_sub(1),
            ToolCategory::Edit => self.edit_count = self.edit_count.saturating_sub(1),
            ToolCategory::Command => self.command_count = self.command_count.saturating_sub(1),
            ToolCategory::Agent => self.agent_count = self.agent_count.saturating_sub(1),
            ToolCategory::Skill => self.skill_count = self.skill_count.saturating_sub(1),
            ToolCategory::Other => self.other_count = self.other_count.saturating_sub(1),
        }
    }
}

fn classify_tool(name: &str) -> ToolCategory {
    match name {
        "Read" => ToolCategory::Read,
        "Grep" | "Glob" => ToolCategory::Search,
        "Edit" | "Write" => ToolCategory::Edit,
        "Bash" => ToolCategory::Command,
        "Agent" => ToolCategory::Agent,
        "Skill" => ToolCategory::Skill,
        _ => ToolCategory::Other,
    }
}

pub fn format_elapsed(start: Instant) -> String {
    let secs = start.elapsed().as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else {
        format!("{}m {}s", secs / 60, secs % 60)
    }
}

pub fn format_token_count_short(count: u64) -> String {
    if count >= 1000 {
        format!("{:.1}k", count as f64 / 1000.0)
    } else {
        format!("{}", count)
    }
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
        question: String,                       // the question text from the tool
        options: Vec<(String, Option<String>)>, // (label, description)
        selected: usize,
        text_input: String,
        reply_tx: Option<tokio::sync::oneshot::Sender<crate::query::types::UserQuestionReply>>,
    },
    /// Exit confirmation dialog (worktree active)
    ExitConfirm {
        worktree_name: String,
        uncommitted_count: usize,
        selected: usize,
    },
}

impl App {
    pub fn new(model: String, command_registry: Arc<CommandRegistry>) -> Self {
        let has_provider = model != "(no provider)";
        let mut messages = vec![DisplayMessage::Text {
            role: TextRole::System,
            content: format!("zpro-code v{}", env!("CARGO_PKG_VERSION")),
        }];

        if has_provider {
            messages.push(DisplayMessage::Text {
                role: TextRole::System,
                content: format!("Connected to model: {}", model),
            });
        } else {
            messages.push(DisplayMessage::Text {
                role: TextRole::System,
                content: "No LLM provider connected. Use --api-base to connect.".into(),
            });
        }

        messages.push(DisplayMessage::Text {
            role: TextRole::System,
            content: "Type a message and press Enter. Ctrl+C to quit.".into(),
        });

        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now(),
            messages,
            input: String::new(),
            cursor_pos: 0,
            should_quit: false,
            is_loading: false,
            queued_input: None,
            query_start_time: None,
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
            needs_redraw: false,
            hit_areas: Vec::new(),
            expand_hit_areas: Vec::new(),
            active_abort_signal: None,
            last_ctrl_c_press: None,
            last_ctrl_d_press: None,
            history: crate::tui::history::InputHistory::new(
                crate::utils::paths::global_config_dir().join("history.json"),
                500,
            ),
            context_window: 0,
            permission_mode_label: "auto".to_string(),
            activity: ActivitySummary::default(),
            running_tools: std::collections::HashMap::new(),
            response_start_time: None,
            response_token_count: 0,
            cumulative_output_tokens: 0,
            todos: Vec::new(),
        }
    }

    /// Handle a key event.
    /// Returns true if typeahead accepted a selection (caller should handle Enter).
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            // Ctrl+C/D handled in run.rs event loop (cancel/exit flow)
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
                let col = cursor_row_col(&self.input, self.cursor_pos).1;
                self.cursor_pos -= col;
            }
            KeyCode::End => {
                let after: String = self.input.chars().skip(self.cursor_pos).collect();
                let remaining = after.chars().take_while(|c| *c != '\n').count();
                self.cursor_pos += remaining;
            }
            KeyCode::Up => {
                let (row, col) = cursor_row_col(&self.input, self.cursor_pos);
                if row == 0 {
                    // First line — use history
                    if let Some(prev) = self.history.up(&self.input) {
                        self.input = prev.to_string();
                        self.cursor_pos = self.input.chars().count();
                    }
                } else {
                    // Move cursor up one line
                    let lines: Vec<&str> = self.input.split('\n').collect();
                    let prev_line_len = lines[row - 1].chars().count();
                    let target_col = col.min(prev_line_len);
                    let chars_before_prev_line: usize =
                        lines[..row - 1].iter().map(|l| l.chars().count() + 1).sum();
                    self.cursor_pos = chars_before_prev_line + target_col;
                }
            }
            KeyCode::Down => {
                let lines: Vec<&str> = self.input.split('\n').collect();
                let (row, col) = cursor_row_col(&self.input, self.cursor_pos);
                if row >= lines.len() - 1 {
                    // Last line — use history
                    if let Some(next) = self.history.down() {
                        self.input = next.to_string();
                        self.cursor_pos = self.input.chars().count();
                    }
                } else {
                    let next_line_len = lines[row + 1].chars().count();
                    let target_col = col.min(next_line_len);
                    let chars_before_next_line: usize =
                        lines[..row + 1].iter().map(|l| l.chars().count() + 1).sum();
                    self.cursor_pos = chars_before_next_line + target_col;
                }
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_add(20);
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(20);
            }
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::ALT)
                    || key.modifiers.contains(KeyModifiers::SHIFT)
                {
                    let byte_pos = char_to_byte_pos(&self.input, self.cursor_pos);
                    self.input.insert(byte_pos, '\n');
                    self.cursor_pos += 1;
                }
                // Else: submit handled by main loop
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
            .map(|cmd| (cmd.name().to_string(), cmd.meta().description.to_string()))
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

    /// Mark any in-flight tools/agents as cancelled (result=None → error state).
    /// Called when a query is cancelled or ends unexpectedly.
    fn mark_pending_tools_cancelled(&mut self) {
        for msg in &mut self.messages {
            match msg {
                DisplayMessage::ToolCall {
                    result, is_error, ..
                } if result.is_none() => {
                    *result = Some("[cancelled]".into());
                    *is_error = true;
                }
                DisplayMessage::Agent { status, result, .. } if *status == AgentStatus::Running => {
                    *status = AgentStatus::Failed;
                    *result = Some("[cancelled]".into());
                }
                _ => {}
            }
        }
    }

    /// Mark any in-flight tools as silently completed (no error).
    /// Called on ResponseComplete/QueryCompleted to clean up orphaned spinners
    /// from leaked subagent events or interrupted streams.
    fn mark_pending_tools_done(&mut self) {
        for msg in &mut self.messages {
            match msg {
                DisplayMessage::ToolCall { result, .. } if result.is_none() => {
                    *result = Some(String::new());
                }
                DisplayMessage::Agent { status, .. } if *status == AgentStatus::Running => {
                    *status = AgentStatus::Completed;
                }
                _ => {}
            }
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
                    Role::User => TextRole::User,
                    Role::Assistant => TextRole::Assistant,
                    _ => TextRole::System,
                };

                Some(DisplayMessage::Text { role, content })
            })
            .collect()
    }

    /// Determine the current activity label for the status bar.
    pub fn activity_label(&self) -> String {
        // 1. Custom loading label
        if let Some(ref label) = self.loading_label {
            return label.clone();
        }
        // 2. Active agent description
        for msg in self.messages.iter().rev() {
            if let DisplayMessage::Agent {
                description,
                status: AgentStatus::Running,
                ..
            } = msg
            {
                return description.clone();
            }
        }
        // 3. Active tools summary
        if self.activity.total_active() > 0 {
            return self.activity.format_summary();
        }
        // 4. LLM streaming
        if self.response_start_time.is_some() {
            return "Responding".into();
        }
        // 5. Default
        "Thinking".into()
    }

    /// Handle a query event (streaming update)
    pub fn handle_query_event(&mut self, event: QueryEvent) {
        // Auto-scroll to bottom on new content
        self.scroll_offset = 0;

        match event {
            QueryEvent::ThinkingDelta { text } => {
                let found = self.messages.iter_mut().rev().find(|m| {
                    matches!(
                        m,
                        DisplayMessage::Thinking {
                            is_complete: false,
                            ..
                        }
                    )
                });
                if let Some(DisplayMessage::Thinking {
                    content,
                    token_count,
                    ..
                }) = found
                {
                    content.push_str(&text);
                    *token_count = (content.len() as u64) / 4;
                } else {
                    let tc = (text.len() as u64) / 4;
                    self.messages.push(DisplayMessage::Thinking {
                        content: text,
                        token_count: tc,
                        is_complete: false,
                        collapsed: true,
                    });
                }
            }
            QueryEvent::TextDelta(text) => {
                if self.response_start_time.is_none() {
                    self.response_start_time = Some(Instant::now());
                }
                // Mark any open thinking as complete
                for msg in self.messages.iter_mut().rev() {
                    if let DisplayMessage::Thinking { is_complete, .. } = msg {
                        if !*is_complete {
                            *is_complete = true;
                        }
                        break;
                    }
                }
                if let Some(DisplayMessage::Text {
                    role: TextRole::Assistant,
                    content,
                }) = self.messages.last_mut()
                {
                    content.push_str(&text);
                    return;
                }
                self.messages.push(DisplayMessage::Text {
                    role: TextRole::Assistant,
                    content: text,
                });
            }
            QueryEvent::ToolCallStart {
                id,
                name,
                parent_agent_id: _,
            } => {
                self.activity.add(&id, &name);
                // Store in running_tools, NOT in messages
                self.running_tools.insert(id.clone(), RunningToolInfo {
                    id: id.clone(),
                    name,
                    input_summary: String::new(),
                    input_full: None,
                    start_time: Instant::now(),
                });
            }
            QueryEvent::ToolCallComplete { id, result } => {
                self.activity.remove(&id);

                // Get info from running_tools
                if let Some(tool_info) = self.running_tools.remove(&id) {
                    // Add completed tool to messages
                    self.messages.push(DisplayMessage::ToolCall {
                        id: id.clone(),
                        name: tool_info.name,
                        input_summary: tool_info.input_summary,
                        input_full: tool_info.input_full,
                        result: Some(result.content),
                        is_error: result.is_error,
                        collapsed: true,
                        parent_id: None,
                    });
                }
            }
            QueryEvent::ResponseComplete { .. } => {
                // DON'T set is_loading = false here — ResponseComplete fires after each
                // LLM streaming chunk, but more tools + LLM calls may follow.
                // is_loading is cleared by QueryCompleted (true end of query).
                self.response_start_time = None;
                self.response_token_count = 0;
                // Token estimation for display
                if let Some(DisplayMessage::Text {
                    role: TextRole::Assistant,
                    content,
                }) = self.messages.last()
                {
                    if self.token_count == 0 {
                        self.token_count =
                            crate::services::token_estimation::estimate_tokens(content);
                    }
                }
            }
            QueryEvent::QueryCompleted { messages } => {
                self.api_messages = messages;
                self.is_loading = false;
                self.activity = ActivitySummary::default();
                self.response_start_time = None;
                self.response_token_count = 0;
                self.cumulative_output_tokens = 0;

                // Clear any remaining running tools (shouldn't normally happen)
                self.running_tools.clear();

                // Mark any pending tools as done
                self.mark_pending_tools_done();

                // Mark remaining open thinking as complete
                for msg in self.messages.iter_mut() {
                    if let DisplayMessage::Thinking { is_complete, .. } = msg {
                        *is_complete = true;
                    }
                }
                // Remove empty thinking messages
                self.messages.retain(|m| {
                    !matches!(m, DisplayMessage::Thinking { content, .. } if content.is_empty())
                });
                self.query_start_time = None;
            }
            QueryEvent::Usage {
                input_tokens,
                output_tokens,
            } => {
                self.token_count += input_tokens + output_tokens;
                self.response_token_count += output_tokens;
                self.cumulative_output_tokens += output_tokens;
                // Simple cost estimate: ~$0.01 per 1K tokens (rough average)
                self.session_cost += (input_tokens + output_tokens) as f64 * 0.00001;
            }
            QueryEvent::BriefMessage { message, .. } => {
                self.messages.push(DisplayMessage::Text {
                    role: TextRole::Assistant,
                    content: message,
                });
            }
            QueryEvent::Error(msg) => {
                self.messages.push(DisplayMessage::Text {
                    role: TextRole::System,
                    content: format!("Error: {}", msg),
                });
                self.is_loading = false;
                self.loading_label = None;
                self.needs_redraw = true;
                self.activity = ActivitySummary::default();
                self.response_start_time = None;
                self.response_token_count = 0;
                self.query_start_time = None;
            }
            QueryEvent::ToolCallInputReady { id, input, name } => {
                let summary = crate::tui::tool_summary::tool_input_summary(&name, &input);
                let full = crate::tui::tool_summary::tool_input_full(&name, &input);
                self.activity.last_detail = summary.clone();
                // Update running_tools with input details
                if let Some(tool_info) = self.running_tools.get_mut(&id) {
                    tool_info.input_summary = summary;
                    tool_info.input_full = Some(full);
                }
            }
            QueryEvent::AgentCompleted {
                agent_id,
                agent_type,
                description,
                result,
                is_error,
                tool_count,
                token_count,
                duration_secs,
            } => {
                self.push_message(DisplayMessage::Agent {
                    agent_id,
                    agent_type,
                    description,
                    status: if is_error {
                        AgentStatus::Failed
                    } else {
                        AgentStatus::Completed
                    },
                    result,
                    collapsed: true,
                    tool_count,
                    token_count,
                    duration_secs,
                    child_tool_ids: Vec::new(),
                    visible_child_count: 3,
                });
            }
            QueryEvent::Compacted {
                stage,
                tokens_freed,
            } => {
                self.messages.push(DisplayMessage::Text {
                    role: TextRole::System,
                    content: format!(
                        "[Context compacted ({}): ~{}K tokens freed]",
                        stage,
                        tokens_freed / 1000
                    ),
                });
            }
            QueryEvent::QueryCancelled {
                partial_messages, ..
            } => {
                self.api_messages = partial_messages;
                self.is_loading = false;
                self.activity = ActivitySummary::default();
                self.response_start_time = None;
                self.response_token_count = 0;
                self.query_start_time = None;
                self.mark_pending_tools_cancelled();
            }
        }
    }
}

/// Compute the (row, col) of a cursor position within a multi-line string.
/// Both row and col are zero-indexed and measured in chars.
pub fn cursor_row_col(input: &str, cursor_pos: usize) -> (usize, usize) {
    let before_cursor: String = input.chars().take(cursor_pos).collect();
    let row = before_cursor.matches('\n').count();
    let col = before_cursor
        .chars()
        .rev()
        .take_while(|c| *c != '\n')
        .count();
    (row, col)
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
        let end = after_at.find(char::is_whitespace).unwrap_or(after_at.len());
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
        1 + after_at.find(char::is_whitespace).unwrap_or(after_at.len())
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
        match app.messages.last().unwrap() {
            DisplayMessage::Text { content, .. } => assert_eq!(content, "Hello world"),
            _ => panic!("Expected DisplayMessage::Text"),
        }
    }

    #[test]
    fn test_ctrl_c_handled_in_event_loop() {
        // Ctrl+C is now handled in run.rs event loop, not in handle_key.
        // handle_key should return false and NOT set should_quit.
        let mut app = test_app("test");
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!app.should_quit);
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

    // --- cursor_row_col tests ---

    #[test]
    fn test_cursor_row_col_single_line() {
        assert_eq!(cursor_row_col("hello", 3), (0, 3));
    }

    #[test]
    fn test_cursor_row_col_multiline() {
        assert_eq!(cursor_row_col("abc\ndef", 5), (1, 1));
    }

    #[test]
    fn test_cursor_row_col_at_newline() {
        assert_eq!(cursor_row_col("abc\n", 4), (1, 0));
    }

    #[test]
    fn test_cursor_row_col_empty() {
        assert_eq!(cursor_row_col("", 0), (0, 0));
    }

    #[test]
    fn test_activity_summary_format() {
        let mut a = ActivitySummary::default();
        a.add("t1", "Read");
        a.add("t2", "Read");
        a.add("t3", "Grep");
        let s = a.format_summary();
        assert!(s.contains("reading 2 files"));
        // "Searching" is capitalized because search comes first in output order
        assert!(s.contains("Searching for 1 pattern"));
    }

    #[test]
    fn test_activity_summary_remove() {
        let mut a = ActivitySummary::default();
        a.add("t1", "Read");
        assert_eq!(a.total_active(), 1);
        a.remove("t1");
        assert_eq!(a.total_active(), 0);
    }

    #[test]
    fn test_classify_tool() {
        assert_eq!(classify_tool("Read"), ToolCategory::Read);
        assert_eq!(classify_tool("Grep"), ToolCategory::Search);
        assert_eq!(classify_tool("Bash"), ToolCategory::Command);
        assert_eq!(classify_tool("Skill"), ToolCategory::Skill);
        assert_eq!(classify_tool("Unknown"), ToolCategory::Other);
    }

    #[test]
    fn test_format_elapsed() {
        let start = Instant::now();
        let s = format_elapsed(start);
        assert!(s.contains('s'));
    }

    #[test]
    fn test_format_token_count_short() {
        assert_eq!(format_token_count_short(500), "500");
        assert_eq!(format_token_count_short(1500), "1.5k");
        assert_eq!(format_token_count_short(0), "0");
    }
}
