# Batch 1: Core Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 5 essential tools (WebFetch, WebSearch, AskUserQuestion, Sleep, NotebookEdit) and refactor permission UI from modal to inline — matching the original Claude Code UX where prompts appear in the chat flow.

**Architecture:** Each tool is a standalone Rust file implementing the `Tool` trait. SleepTool and NotebookEditTool are self-contained. WebFetchTool uses reqwest+readability+htmd. WebSearchTool calls Tavily API. AskUserQuestionTool and the permission system both use inline prompts — an `InputMode` enum controls what the input area shows (normal text input, permission options, or question options).

**Tech Stack:** Rust, reqwest, readability (0.3), htmd (0.5), serde_json, tokio, ratatui

**Spec:** `docs/superpowers/specs/2026-04-04-batch1-core-tools-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/tools/sleep_tool.rs` | **New** — SleepTool |
| `src/tools/notebook_edit_tool.rs` | **New** — NotebookEditTool |
| `src/tools/web_fetch_tool.rs` | **New** — WebFetchTool |
| `src/tools/web_search_tool.rs` | **New** — WebSearchTool |
| `src/tools/ask_user_question_tool.rs` | **New** — AskUserQuestionTool |
| `src/tools/mod.rs` | Register all 5 new tools |
| `src/tool.rs` | Add `tool_use_id`, `question_tx` to ToolUseContext |
| `src/query/types.rs` | Add `UserQuestionAsk`, `QuestionOption`, `UserQuestionReply`; add `question_tx` to QueryContext |
| `src/query/engine.rs` | Pass `tool_use_id`, `question_tx` in execute_tool() |
| `src/tui/app.rs` | Replace `PermissionDialog` modal with `InputMode` enum for inline prompts |
| `src/tui/run.rs` | Refactor permission handling to inline, add question channel |
| `src/tui/ui.rs` | Remove modal render, add inline prompt in input area |
| `src/services/config/mod.rs` | Add `tavily_api_key` to AtomConfig |
| `Cargo.toml` | Add `readability`, `htmd` dependencies |

---

### Task 1: Add dependencies to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add readability and htmd to [dependencies]**

Add after `regex = "1"`:
```toml
readability = "0.3"
htmd = "0.5"
```

- [ ] **Step 2: Verify dependencies resolve**

Run: `cargo check 2>&1 | tail -5`
Expected: No new errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add readability and htmd for WebFetchTool"
```

---

### Task 2: Implement SleepTool

**Files:**
- Create: `src/tools/sleep_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create `src/tools/sleep_tool.rs`**

```rust
use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct SleepTool;

#[async_trait]
impl Tool for SleepTool {
    fn name(&self) -> &str { "Sleep" }

    fn description(&self) -> &str {
        "Pause execution for a specified duration. The user can interrupt at any time."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "duration_ms": {
                    "type": "integer",
                    "description": "Duration to sleep in milliseconds (max 60000)"
                }
            },
            "required": ["duration_ms"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let duration_ms = ctx.input.get("duration_ms")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Sleep".into(),
                message: "Missing required 'duration_ms' parameter".into(),
            })?;

        let duration_ms = duration_ms.min(60_000);
        let duration = std::time::Duration::from_millis(duration_ms);

        tokio::select! {
            _ = tokio::time::sleep(duration) => {
                Ok(ToolResult { content: format!("Slept for {}ms", duration_ms), is_error: false })
            }
            _ = ctx.abort_signal.cancelled() => {
                Ok(ToolResult { content: "Sleep interrupted".into(), is_error: false })
            }
        }
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() { assert_eq!(SleepTool.name(), "Sleep"); }

    #[tokio::test]
    async fn test_sleep_short() {
        let ctx = ToolUseContext::test(serde_json::json!({ "duration_ms": 10 }));
        let result = SleepTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("10ms"));
    }

    #[tokio::test]
    async fn test_sleep_clamps_max() {
        let ctx = ToolUseContext::test(serde_json::json!({ "duration_ms": 999999 }));
        let result = SleepTool.execute(ctx).await.unwrap();
        assert!(result.content.contains("60000ms"));
    }

    #[tokio::test]
    async fn test_missing_param() {
        let ctx = ToolUseContext::test(serde_json::json!({}));
        assert!(SleepTool.execute(ctx).await.is_err());
    }
}
```

- [ ] **Step 2: Register in `src/tools/mod.rs`** — add `pub mod sleep_tool;` and `Box::new(sleep_tool::SleepTool),`

- [ ] **Step 3: Run tests and commit**

Run: `cargo test sleep_tool 2>&1 | tail -10`

```bash
git add src/tools/sleep_tool.rs src/tools/mod.rs
git commit -m "feat: implement SleepTool"
```

---

### Task 3: Implement NotebookEditTool

**Files:**
- Create: `src/tools/notebook_edit_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create `src/tools/notebook_edit_tool.rs`**

Implements insert/replace/delete on `.ipynb` cells. Replace preserves existing metadata/outputs. See full implementation in the previous plan revision — the code is unchanged. Key points:
- Read .ipynb as `serde_json::Value`, modify `cells` array, write back
- `source_to_lines(source)` splits into Jupyter line-array format
- `make_cell(cell_type, source)` creates proper cell object
- Permission via `file_path` in `permission_metadata()`
- Tests: insert into empty notebook, delete out-of-bounds, source line splitting

- [ ] **Step 2: Register in `src/tools/mod.rs`**

- [ ] **Step 3: Run tests and commit**

```bash
git add src/tools/notebook_edit_tool.rs src/tools/mod.rs
git commit -m "feat: implement NotebookEditTool"
```

---

### Task 4: Implement WebFetchTool

**Files:**
- Create: `src/tools/web_fetch_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create `src/tools/web_fetch_tool.rs`**

Key points:
- `reqwest::Client` with 30s timeout, "atom-code/0.1" user-agent
- URL validation (http/https only)
- Content-Type check: HTML → readability + htmd pipeline, text → as-is, binary → error
- 5MB max body size
- 20,000 char output truncation
- `readability::extractor::extract()` for main content extraction, fallback to raw htmd
- Optional `prompt` parameter prepended as instruction

**Important:** Read the `readability` crate's actual API before implementing. It may need `url::Url` type — if so, add `url = "2"` to Cargo.toml. Adapt `html_to_markdown()` to match the real API.

- [ ] **Step 2: Register in `src/tools/mod.rs`**

- [ ] **Step 3: Run tests and commit**

```bash
git add src/tools/web_fetch_tool.rs src/tools/mod.rs Cargo.toml Cargo.lock
git commit -m "feat: implement WebFetchTool with readability + htmd"
```

---

### Task 5: Add tavily_api_key to AtomConfig + implement WebSearchTool

**Files:**
- Modify: `src/services/config/mod.rs`
- Create: `src/tools/web_search_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Add `tavily_api_key: Option<String>` to AtomConfig** (after `permission_mode` field, with `#[serde(default)]`)

- [ ] **Step 2: Create `src/tools/web_search_tool.rs`**

Key points:
- Load API key via `AtomConfig::load().tavily_api_key`
- Missing key → clear error with setup instructions
- POST `https://api.tavily.com/search` with `Authorization: Bearer <key>`
- Parse `TavilyResponse { results: Vec<TavilyResult> }` where each result has `title`, `url`, `content`
- Format as `### [title](url)\n\ncontent` separated by `---`
- num_results default 5, max 10

- [ ] **Step 3: Register, test, commit**

```bash
git add src/services/config/mod.rs src/tools/web_search_tool.rs src/tools/mod.rs
git commit -m "feat: implement WebSearchTool with Tavily API"
```

---

### Task 6: Refactor permission dialog from modal to inline

This is a prerequisite for AskUserQuestionTool and establishes the inline prompt pattern for all future interactive tools.

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/run.rs`
- Modify: `src/tui/ui.rs`

- [ ] **Step 1: Add `InputMode` enum and refactor `App` in `src/tui/app.rs`**

Replace `PermissionDialog` with a general-purpose inline prompt system:

```rust
/// What the input area is currently showing
pub enum InputMode {
    /// Normal text input
    Normal,
    /// Permission prompt: [y] Allow  [n] Deny  [a] Always allow
    Permission {
        tool_name: String,
        description: String,
        reply_tx: Option<tokio::sync::oneshot::Sender<PermissionReply>>,
    },
}
```

In `App`:
- Remove `permission_dialog: Option<PermissionDialog>` field
- Remove `PermissionDialog` struct
- Add `input_mode: InputMode` field (default `InputMode::Normal`)
- Remove `handle_permission_key()` method

- [ ] **Step 2: Update key handling in `src/tui/run.rs`**

Replace the modal permission handling block (lines ~122-137) with inline handling:

```rust
// In the Key event handler:
match &app.input_mode {
    InputMode::Permission { .. } => {
        match key.code {
            KeyCode::Char('y') => {
                if let InputMode::Permission { reply_tx, .. } = std::mem::replace(&mut app.input_mode, InputMode::Normal) {
                    if let Some(tx) = reply_tx {
                        let _ = tx.send(PermissionReply::Allow);
                    }
                }
                app.push_message(DisplayMessage { role: DisplayRole::System, content: "Allowed".into(), tool_name: None });
            }
            KeyCode::Char('n') => {
                if let InputMode::Permission { reply_tx, .. } = std::mem::replace(&mut app.input_mode, InputMode::Normal) {
                    if let Some(tx) = reply_tx {
                        let _ = tx.send(PermissionReply::Deny);
                    }
                }
                app.push_message(DisplayMessage { role: DisplayRole::System, content: "Denied".into(), tool_name: None });
            }
            KeyCode::Char('a') => {
                if let InputMode::Permission { tool_name, reply_tx, .. } = std::mem::replace(&mut app.input_mode, InputMode::Normal) {
                    if let Some(tx) = reply_tx {
                        let _ = tx.send(PermissionReply::AllowAlways { tool_name, rule_content: None });
                    }
                }
                app.push_message(DisplayMessage { role: DisplayRole::System, content: "Always allowed".into(), tool_name: None });
            }
            _ => {} // ignore other keys
        }
    }
    InputMode::Normal => {
        // ... existing normal key handling ...
    }
}
```

Update the permission_rx handler (line ~229) to set `InputMode::Permission` instead of setting `permission_dialog`:

```rust
ask = async { ... } => {
    // Show the tool action as a chat message
    app.push_message(DisplayMessage {
        role: DisplayRole::Tool,
        content: format!("{}: {}", ask.tool_name, ask.description),
        tool_name: Some(ask.tool_name.clone()),
    });
    // Switch input mode to permission prompt
    app.input_mode = InputMode::Permission {
        tool_name: ask.tool_name,
        description: ask.description,
        reply_tx: Some(ask.reply_tx),
    };
}
```

- [ ] **Step 3: Update `src/tui/ui.rs`**

Remove `render_permission_dialog()` and its call in the main render function.

Instead, modify the input area rendering to show the prompt based on `InputMode`:

In the input area rendering function (where the text input box is drawn), check `app.input_mode`:
- `Normal` → render the text input as usual
- `Permission` → render `[y] Allow  [n] Deny  [a] Always allow` in the input area with styled text

```rust
// In the input area section:
match &app.input_mode {
    InputMode::Normal => {
        // existing input rendering
    }
    InputMode::Permission { .. } => {
        let prompt = Line::from(vec![
            Span::styled("[y]", Style::default().fg(Color::Green).bold()),
            Span::raw(" Allow  "),
            Span::styled("[n]", Style::default().fg(Color::Red).bold()),
            Span::raw(" Deny  "),
            Span::styled("[a]", Style::default().fg(Color::Yellow).bold()),
            Span::raw(" Always allow"),
        ]);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        frame.render_widget(Paragraph::new(prompt).block(block), input_area);
    }
}
```

- [ ] **Step 4: Run cargo check and verify**

Run: `cargo check 2>&1 | tail -10`
Expected: Clean compilation.

- [ ] **Step 5: Commit**

```bash
git add src/tui/app.rs src/tui/run.rs src/tui/ui.rs
git commit -m "refactor: replace permission modal with inline prompt in chat flow"
```

---

### Task 7: Add question channel + implement AskUserQuestionTool (inline)

**Files:**
- Modify: `src/tool.rs`
- Modify: `src/query/types.rs`
- Modify: `src/query/engine.rs`
- Modify: `src/tui/app.rs`
- Modify: `src/tui/run.rs`
- Modify: `src/tui/ui.rs`
- Modify: `src/entrypoints/print.rs`
- Create: `src/tools/ask_user_question_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Add types to `src/query/types.rs`**

```rust
pub struct UserQuestionAsk {
    pub tool_use_id: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub reply_tx: tokio::sync::oneshot::Sender<UserQuestionReply>,
}

pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

pub enum UserQuestionReply {
    Answer(String),
    Dismissed,
}
```

Add `question_tx: mpsc::UnboundedSender<UserQuestionAsk>` to `QueryContext`.

- [ ] **Step 2: Add `tool_use_id` and `question_tx` to `ToolUseContext` in `src/tool.rs`**

```rust
    pub tool_use_id: String,
    pub question_tx: tokio::sync::mpsc::UnboundedSender<crate::query::types::UserQuestionAsk>,
```

Update `ToolUseContext::test()` with:
```rust
    let (question_tx, _) = tokio::sync::mpsc::unbounded_channel();
    // in Self:
    tool_use_id: String::new(),
    question_tx,
```

- [ ] **Step 3: Update `execute_tool()` in `src/query/engine.rs`**

Pass `tc.id.clone()` as `tool_use_id` and `ctx.question_tx.clone()` as `question_tx`.

- [ ] **Step 4: Extend `InputMode` in `src/tui/app.rs`**

Add `Question` variant:
```rust
pub enum InputMode {
    Normal,
    Permission { ... },
    /// User question: options → number keys or ↑↓, free text → type + Enter
    Question {
        question: String,
        options: Vec<(String, Option<String>)>,  // (label, description)
        selected: usize,
        text_input: String,
        reply_tx: Option<tokio::sync::oneshot::Sender<UserQuestionReply>>,
    },
}
```

- [ ] **Step 5: Add question key handling and channel wiring in `src/tui/run.rs`**

Create question channel per-query (same pattern as permission channel):
```rust
let (question_tx, question_rx) = mpsc::unbounded_channel();
// store question_rx, pass question_tx to QueryContext
```

Handle question_rx in event loop:
```rust
ask = async { question_rx... } => {
    app.push_message(DisplayMessage {
        role: DisplayRole::System,
        content: ask.question.clone(),
        tool_name: None,
    });
    app.input_mode = InputMode::Question {
        question: ask.question,
        options: ask.options.into_iter().map(|o| (o.label, o.description)).collect(),
        selected: 0,
        text_input: String::new(),
        reply_tx: Some(ask.reply_tx),
    };
}
```

Handle key events for Question mode:
- With options: `↑/↓/j/k` to navigate, `Enter` to select, `Esc` to dismiss
- Without options: type text, `Enter` to submit, `Esc` to dismiss

- [ ] **Step 6: Add question input rendering in `src/tui/ui.rs`**

Add `InputMode::Question` branch in input area:
- With options: show numbered/highlighted option list in input area
- Without options: show text input with cursor

- [ ] **Step 7: Update `src/entrypoints/print.rs`** — add `question_tx` to QueryContext

- [ ] **Step 8: Create `src/tools/ask_user_question_tool.rs`**

```rust
use async_trait::async_trait;
use crate::query::types::{QuestionOption, UserQuestionAsk, UserQuestionReply};
use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct AskUserQuestionTool;

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &str { "AskUserQuestion" }

    fn description(&self) -> &str {
        "Ask the user a question. Can present multiple-choice options or accept free-text input."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "The question to ask" },
                "options": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": { "type": "string" },
                            "description": { "type": "string" }
                        },
                        "required": ["label"]
                    }
                }
            },
            "required": ["question"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let question = ctx.input.get("question").and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "AskUserQuestion".into(),
                message: "Missing 'question'".into(),
            })?;

        let options: Vec<QuestionOption> = ctx.input.get("options")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|item| {
                Some(QuestionOption {
                    label: item.get("label")?.as_str()?.to_string(),
                    description: item.get("description").and_then(|d| d.as_str()).map(String::from),
                })
            }).collect())
            .unwrap_or_default();

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        ctx.question_tx.send(UserQuestionAsk {
            tool_use_id: ctx.tool_use_id.clone(),
            question: question.to_string(),
            options,
            reply_tx,
        }).map_err(|_| crate::types::AtomError::Tool {
            tool_name: "AskUserQuestion".into(),
            message: "UI channel closed".into(),
        })?;

        match reply_rx.await {
            Ok(UserQuestionReply::Answer(answer)) => Ok(ToolResult { content: answer, is_error: false }),
            Ok(UserQuestionReply::Dismissed) => Ok(ToolResult { content: "User dismissed the question".into(), is_error: true }),
            Err(_) => Ok(ToolResult { content: "Channel closed".into(), is_error: true }),
        }
    }

    fn is_read_only(&self) -> bool { true }
}
```

- [ ] **Step 9: Register in `src/tools/mod.rs`**

- [ ] **Step 10: Run tests and commit**

```bash
git add src/tool.rs src/query/types.rs src/query/engine.rs src/tui/app.rs src/tui/run.rs src/tui/ui.rs src/entrypoints/print.rs src/tools/ask_user_question_tool.rs src/tools/mod.rs
git commit -m "feat: implement AskUserQuestionTool with inline prompt"
```

---

### Task 8: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All new tests pass.

- [ ] **Step 2: Run clippy on changed files**

Run: `cargo clippy -- -D warnings 2>&1 | grep -E "sleep_tool|notebook_edit|web_fetch|web_search|ask_user|tool\.rs|engine\.rs|app\.rs|run\.rs|ui\.rs" | head -20`
Expected: No warnings in our files.

- [ ] **Step 3: Verify tool count**

Run: `cargo test test_get_builtin_tools 2>&1 | tail -5`
Update assertion to match new count (was 8, now 13: +Sleep, +NotebookEdit, +WebFetch, +WebSearch, +AskUserQuestion).

- [ ] **Step 4: Commit any fixes**
