# Batch 1: Core Tools Implementation Design

## Problem

atom-code-rs has only 9 builtin tools (Bash, Read, Edit, Write, Glob, Grep, Agent, Skill + stub). A coding assistant needs web access, user interaction, sleep, and notebook editing to be usable for everyday tasks.

## Scope

5 tools in priority order:

1. **WebFetchTool** — fetch URL content as markdown
2. **WebSearchTool** — web search via Tavily API
3. **AskUserQuestionTool** — interactive questions to user
4. **SleepTool** — timed pause with interrupt support
5. **NotebookEditTool** — Jupyter notebook cell editing

## Design

### 1. WebFetchTool

**Tool name:** `WebFetch`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "url": { "type": "string", "description": "The URL to fetch" },
    "prompt": { "type": "string", "description": "Optional instruction for what to extract from the page" }
  },
  "required": ["url"]
}
```

**Execution flow:**
1. Validate URL (must be http/https)
2. `reqwest::get(url)` with timeout (30s), User-Agent header
3. `readability` crate to extract main content from HTML
4. `htmd` crate to convert to markdown
5. Truncate to ~20,000 chars if too long (prevent context overflow)
6. If `prompt` is provided, prepend it as instruction context
7. Return markdown content as `ToolResult`

**Permission:** `url` field exposed via `permission_metadata()`. Not read-only (fetches external resources).

**Error handling:** Invalid URL, timeout, non-200 status, non-HTML content type (return raw text for plain text, error for binary).

**Dependencies:** `reqwest` (existing), `readability` crate, `htmd` crate (new Cargo.toml entries).

---

### 2. WebSearchTool

**Tool name:** `WebSearch`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "query": { "type": "string", "description": "The search query" },
    "num_results": { "type": "integer", "description": "Number of results (default 5, max 10)" }
  },
  "required": ["query"]
}
```

**Execution flow:**
1. Read Tavily API key from `~/.atom/config.json` field `tavily_api_key`
2. If no key, return error with setup instructions
3. POST to `https://api.tavily.com/search` with:
   ```json
   {
     "api_key": "<key>",
     "query": "<query>",
     "max_results": <num_results>,
     "include_answer": false,
     "include_raw_content": false
   }
   ```
4. Parse response, format each result as:
   ```
   ### [title](url)

   content
   ```
5. Return formatted markdown

**Permission:** Not read-only. Permission check on tool name.

**Error handling:** Missing API key (clear setup message), API errors (rate limit, invalid key), network failures.

**Dependencies:** `reqwest` (existing), `serde` for Tavily response parsing.

**Config integration:** `AtomConfig` in `src/services/config.rs` needs `tavily_api_key: Option<String>` field. Key stored in `~/.atom/config.json`.

---

### 3. AskUserQuestionTool

**Tool name:** `AskUserQuestion`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "question": { "type": "string", "description": "The question to ask the user" },
    "options": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Optional multiple-choice options"
    }
  },
  "required": ["question"]
}
```

**Execution flow:**
1. Send a `UserQuestionAsk` through a new channel (similar to `PermissionAsk`)
2. TUI receives it, renders a dialog:
   - With `options`: list UI (up/down to select, Enter to confirm)
   - Without `options`: text input field
3. User responds, reply sent back via oneshot channel
4. Return user's answer as `ToolResult`

**New types (in `src/query/types.rs`):**
```rust
pub struct UserQuestionAsk {
    pub tool_use_id: String,
    pub question: String,
    pub options: Vec<String>,
    pub reply_tx: tokio::sync::oneshot::Sender<UserQuestionReply>,
}

pub enum UserQuestionReply {
    Answer(String),
    Dismissed,
}
```

**Channel plumbing:**
- Add `question_tx: mpsc::UnboundedSender<UserQuestionAsk>` to `ToolUseContext`
- Add `question_tx` to `QueryContext`
- TUI's event loop handles `UserQuestionAsk` like `PermissionAsk`

**Permission:** read-only (only asks user a question, no side effects).

**Dependencies:** None new. Reuses existing TUI dialog infrastructure.

---

### 4. SleepTool

**Tool name:** `Sleep`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "duration_ms": {
      "type": "integer",
      "description": "Duration to sleep in milliseconds (max 60000)"
    }
  },
  "required": ["duration_ms"]
}
```

**Execution flow:**
1. Clamp `duration_ms` to 0..60000
2. `tokio::select!` between `tokio::time::sleep(duration)` and `abort_signal.cancelled()`
3. Return `"Slept for Xms"` or `"Sleep interrupted"`

**Properties:** `is_read_only() = true`, `is_concurrent_safe() = true`.

**Dependencies:** None (tokio only).

---

### 5. NotebookEditTool

**Tool name:** `NotebookEdit`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "notebook_path": { "type": "string", "description": "Path to the .ipynb file" },
    "command": { "type": "string", "enum": ["insert", "replace", "delete"] },
    "cell_index": { "type": "integer", "description": "0-based cell index" },
    "cell_type": { "type": "string", "enum": ["code", "markdown"], "description": "Cell type (for insert/replace)" },
    "source": { "type": "string", "description": "Cell content (for insert/replace)" }
  },
  "required": ["notebook_path", "command", "cell_index"]
}
```

**Execution flow:**
1. Read `.ipynb` file as JSON (`serde_json::Value`)
2. Validate it has `cells` array
3. Execute command:
   - **insert:** Create new cell object, insert at `cell_index`
   - **replace:** Replace cell at `cell_index` with new content
   - **delete:** Remove cell at `cell_index`
4. Write modified JSON back to file (preserve formatting with 1-space indent)
5. Return summary: `"Inserted code cell at index 3"` etc.

**Cell format:**
```json
{
  "cell_type": "code",
  "source": ["line1\n", "line2\n"],
  "metadata": {},
  "outputs": [],
  "execution_count": null
}
```

Note: `source` is an array of lines (Jupyter format). The tool splits the input `source` string by newlines.

**Permission:** File write permission (same as FileWriteTool). `file_path` exposed in `permission_metadata()`.

**Error handling:** File not found, invalid JSON, invalid notebook format, cell_index out of bounds, missing source for insert/replace.

**Dependencies:** `serde_json` (existing).

---

## Files Changed

| File | Change |
|------|--------|
| `src/tools/web_fetch_tool.rs` | **New** |
| `src/tools/web_search_tool.rs` | **New** |
| `src/tools/ask_user_question_tool.rs` | **New** |
| `src/tools/sleep_tool.rs` | **New** |
| `src/tools/notebook_edit_tool.rs` | **New** |
| `src/tools/mod.rs` | Register 5 new tools |
| `src/query/types.rs` | Add `UserQuestionAsk`, `UserQuestionReply` |
| `src/tool.rs` | Add `question_tx` to `ToolUseContext` |
| `src/query/engine.rs` | Pass `question_tx` through |
| `src/tui/run.rs` | Handle `UserQuestionAsk` in event loop |
| `src/tui/app.rs` | Add `UserQuestionDialog` state |
| `src/services/config.rs` | Add `tavily_api_key` field |
| `Cargo.toml` | Add `readability`, `htmd` dependencies |

## Non-goals

- Preapproved URL lists for WebFetchTool (add later)
- Tavily onboarding wizard (add in Batch 6 with ConfigTool)
- WebFetch caching
- NotebookEdit output cell manipulation
