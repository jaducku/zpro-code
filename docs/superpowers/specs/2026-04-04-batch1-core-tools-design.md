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

## Prerequisites

### Add `tool_use_id` to `ToolUseContext`

Currently `ToolUseContext` has no `tool_use_id`. The `AskUserQuestionTool` needs it for channel correlation. Add `tool_use_id: String` to `ToolUseContext` in `src/tool.rs` and populate it from `tc.id` in `execute_tool()` in `src/query/engine.rs`.

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
2. `reqwest::get(url)` with timeout (30s), User-Agent header, max body size 5MB
3. Check Content-Type: HTML → readability pipeline, plain text → return as-is, binary → error
4. HTML pipeline: `readability::extractor::extract()` to extract main content → `htmd` to convert to markdown
5. Truncate to ~20,000 chars if too long (prevent context overflow)
6. If `prompt` is provided, prepend it as instruction context
7. Return markdown content as `ToolResult`

**Crate selection:** Use `readability` crate (v0.3+) which takes a `Read` implementor — pass the response body directly. Falls back to raw HTML→htmd if readability extraction fails (some pages resist extraction). Evaluate `readability-rust` as alternative if API is more ergonomic.

**Permission:** `url` field exposed via `permission_metadata()` (uses existing `PermissionMeta.url` field). Not read-only. Redirects followed by default (reqwest default, up to 10).

**Error handling:** Invalid URL, timeout, non-200 status, binary content type, response body > 5MB, non-UTF-8 encoding (reqwest `.text()` handles this).

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
1. Load config via `AtomConfig::load()` (reads `~/.atom/config.json`), get `tavily_api_key`
2. If no key, return error: `"Tavily API key not configured. Add tavily_api_key to ~/.atom/config.json"`
3. POST to `https://api.tavily.com/search` with bearer auth (`Authorization: Bearer <key>`):
   ```json
   {
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

**Config access:** Call `AtomConfig::load()` inside `execute()`. Cheap — reads a small JSON file. No need to add config to `ToolUseContext`.

**Permission:** Not read-only. Permission check on tool name.

**Error handling:** Missing API key (clear setup message), API errors (rate limit, invalid key), network failures, empty results.

**Dependencies:** `reqwest` (existing), `serde` for Tavily response parsing.

**Config integration:** Add `tavily_api_key: Option<String>` to `AtomConfig` in `src/services/config.rs`.

---

### 3. AskUserQuestionTool

**Tool name:** `AskUserQuestion`

**Schema (simplified V1, compatible with TS core usage):**
```json
{
  "type": "object",
  "properties": {
    "question": { "type": "string", "description": "The question to ask the user" },
    "options": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "label": { "type": "string" },
          "description": { "type": "string" }
        },
        "required": ["label"]
      },
      "description": "Optional multiple-choice options with labels and descriptions"
    }
  },
  "required": ["question"]
}
```

V1 simplifications vs TS: single question (not array of 1-4), no `preview`, no `annotations`, no `multiSelect`, no `header`. These are deferred to a future iteration. The schema covers the primary use case (single question with optional structured options).

**Execution flow:**
1. Build `UserQuestionAsk` with `tool_use_id`, `question`, `options`
2. Send through `question_tx` channel (from `ToolUseContext`)
3. `await` reply via oneshot channel
4. On `Answer(text)`: return as `ToolResult`
5. On `Dismissed`: return error `"User dismissed the question"`

**New types (in `src/query/types.rs`):**
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

**Channel plumbing (per-query, same pattern as PermissionAsk):**
- Add `question_tx: mpsc::UnboundedSender<UserQuestionAsk>` to `ToolUseContext` and `QueryContext`
- In `submit_query()` and `submit_query_skill()` (tui/run.rs): create `(question_tx, question_rx)` channel pair, store `question_rx`
- In TUI event loop: add `tokio::select!` branch for `question_rx.recv()`
- In `execute_tool()` (engine.rs): pass `question_tx` from `QueryContext` to `ToolUseContext`
- Update `ToolUseContext::test()`: add throwaway `question_tx`

**TUI dialog (in `src/tui/app.rs`):**
```rust
pub struct UserQuestionDialog {
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub selected: usize,           // for option selection
    pub text_input: String,        // for free-text input
    pub reply_tx: Option<oneshot::Sender<UserQuestionReply>>,
}
```

Rendered like `PermissionDialog` — modal overlay with keyboard navigation.

**Permission:** `is_read_only() = true`.

**Dependencies:** None new.

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
   - **replace:** Replace `source` field of cell at `cell_index`, preserve `metadata`, `outputs`, `execution_count`
   - **delete:** Remove cell at `cell_index`
4. Write modified JSON back to file using notebook's original indent style (detect from file, default 1-space)
5. Return summary: `"Inserted code cell at index 3"` etc.

**Cell format (for insert):**
```json
{
  "cell_type": "code",
  "source": ["line1\n", "line2\n"],
  "metadata": {},
  "outputs": [],
  "execution_count": null
}
```

Note: `source` is an array of lines (Jupyter format). The tool splits the input `source` string by newlines. For **replace**, only `source` and `cell_type` are updated — existing `metadata`, `outputs`, and `execution_count` are preserved.

**Permission:** File write permission (same as FileWriteTool). `file_path` exposed in `permission_metadata()`.

**Error handling:** File not found, invalid JSON, invalid notebook format, cell_index out of bounds, missing source for insert/replace.

**Dependencies:** `serde_json` (existing).

---

## Files Changed

| File | Change |
|------|--------|
| `src/tool.rs` | Add `tool_use_id`, `question_tx` to `ToolUseContext` |
| `src/query/types.rs` | Add `UserQuestionAsk`, `QuestionOption`, `UserQuestionReply`; add `question_tx` to `QueryContext` |
| `src/query/engine.rs` | Pass `tool_use_id` and `question_tx` in `execute_tool()` |
| `src/tools/web_fetch_tool.rs` | **New** |
| `src/tools/web_search_tool.rs` | **New** |
| `src/tools/ask_user_question_tool.rs` | **New** |
| `src/tools/sleep_tool.rs` | **New** |
| `src/tools/notebook_edit_tool.rs` | **New** |
| `src/tools/mod.rs` | Register 5 new tools |
| `src/tui/run.rs` | Create question channel, handle `UserQuestionAsk` in event loop |
| `src/tui/app.rs` | Add `UserQuestionDialog` state |
| `src/services/config.rs` | Add `tavily_api_key` field to `AtomConfig` |
| `Cargo.toml` | Add `readability`, `htmd` dependencies |

## Non-goals

- Preapproved URL lists for WebFetchTool (add later)
- Tavily onboarding wizard (add in Batch 6 with ConfigTool)
- WebFetch caching
- NotebookEdit output cell manipulation
- AskUserQuestion multi-question, preview, annotations, multiSelect (V2)
