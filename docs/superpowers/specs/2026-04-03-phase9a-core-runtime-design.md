# Phase 9A: Core Runtime Services — Design Spec

## Overview

Make atom-code actually usable: multi-provider LLM connection, real query execution in the TUI, streaming tool execution, session persistence, and token management.

**Scope:**
- Multi-provider support (Codex OAuth token reuse + OpenAI-compatible)
- TUI query loop wiring (provider.create_message actually called)
- StreamingToolExecutor (concurrent tool execution with progress)
- Auto-compact (token budget management)
- Session transcript persistence
- Token estimation

**Not in scope:** session_memory (CLAUDE.md), plugins, skills, analytics, enterprise services.

---

## 1. Multi-Provider

### Provider selection via `--provider` flag

```rust
// CLI flag
#[arg(long)]
provider: Option<String>,  // "codex", "openai"
```

### Provider factory

```rust
// src/services/api/provider_factory.rs

pub fn create_provider(
    provider_type: Option<&str>,
    api_base: Option<&str>,
    api_key: Option<&str>,
    model: Option<&str>,
) -> Result<Box<dyn LlmProvider>> {
    match provider_type {
        Some("codex") => {
            let token = read_codex_token()?;
            let model = model.unwrap_or("gpt-4o");
            Ok(Box::new(OpenAiCompatibleProvider::new(
                "https://api.openai.com/v1".into(),
                Some(token),
                model.into(),
            )))
        }
        Some("openai") | None => {
            let base = api_base
                .ok_or_else(|| anyhow::anyhow!("--api-base required for openai provider"))?;
            Ok(Box::new(OpenAiCompatibleProvider::new(
                base.into(),
                api_key.map(|s| s.to_string()),
                model.unwrap_or("default").into(),
            )))
        }
        Some(other) => Err(anyhow::anyhow!("Unknown provider: {}. Use 'codex' or 'openai'.", other)),
    }
}
```

### Codex token reading

```rust
// src/services/api/codex_auth.rs

pub fn read_codex_token() -> Result<String> {
    // 1) CODEX_TOKEN env var
    if let Ok(token) = std::env::var("CODEX_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // 2) ~/.codex/auth.json
    // 3) ~/.config/codex/auth.json
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{}/.codex/auth.json", home),
        format!("{}/.config/codex/auth.json", home),
    ];

    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(token) = json["access_token"].as_str() {
                    return Ok(token.to_string());
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Codex OAuth token not found.\n\
         Run 'codex auth login' first, or set CODEX_TOKEN env var."
    ))
}
```

No token refresh — delegate to Codex CLI. On 401 error, show "Token expired. Run 'codex auth login'."

---

## 2. TUI Query Loop Wiring

Currently `run.rs` has the provider but never calls it. Wire it up:

### Changes to run.rs

```rust
// When user submits a message (not a slash command):
// 1. Add user message to display + api_messages
// 2. Set is_loading = true
// 3. Spawn query task:
tokio::spawn(async move {
    let result = query(ctx, &*provider, event_tx, cancel).await;
    // Query events stream back via event_tx → app.handle_query_event()
});
```

The query engine (`src/query/engine.rs`) already implements the full loop:
- Build CreateMessageRequest
- Call provider.create_message()
- Stream events → emit QueryEvent
- Handle tool calls → loop

### Provider ownership

Provider needs to be shared across the async boundary:
```rust
let provider: Arc<dyn LlmProvider> = Arc::new(created_provider);
```

The `LlmProvider` trait already requires `Send + Sync`. Use `Arc` to share between the event loop and spawned query tasks.

---

## 3. StreamingToolExecutor

Port of `src/services/tools/StreamingToolExecutor`. Manages concurrent tool execution during the query loop.

```rust
// src/services/tools/executor.rs

pub struct StreamingToolExecutor {
    tools: Vec<Box<dyn Tool>>,
    max_concurrent: usize,
}

impl StreamingToolExecutor {
    pub fn new(tools: Vec<Box<dyn Tool>>, max_concurrent: usize) -> Self;

    /// Execute a batch of tool calls concurrently
    pub async fn execute_batch(
        &self,
        tool_calls: Vec<ToolCall>,
        ctx: ToolUseContext,
        progress_tx: mpsc::Sender<ToolProgress>,
    ) -> Vec<ToolResult>;
}

pub enum ToolProgress {
    Started { tool_use_id: String, name: String },
    Completed { tool_use_id: String, result: ToolResult },
    Failed { tool_use_id: String, error: String },
}
```

Uses `tokio::JoinSet` for concurrent execution. Tool results are sent back as they complete. The query engine collects all results before sending the next API request.

---

## 4. Auto-Compact

Token budget management. When conversation exceeds the model's context window, automatically compress older messages.

```rust
// src/services/compact/mod.rs

pub struct CompactService {
    provider: Arc<dyn LlmProvider>,
}

impl CompactService {
    /// Check if compaction is needed based on token count vs context window
    pub fn needs_compaction(
        &self,
        messages: &[ApiMessage],
        token_count: u64,
    ) -> bool {
        let limit = self.provider.model_info().context_window;
        token_count > (limit * 80 / 100) // 80% threshold
    }

    /// Compact messages by summarizing older ones
    pub async fn compact(
        &self,
        messages: &[ApiMessage],
        instructions: Option<&str>,
    ) -> Result<CompactResult>;
}

pub struct CompactResult {
    pub summary: String,
    pub messages_removed: usize,
    pub tokens_saved: u64,
}
```

Compaction sends a summarization request to the LLM provider, then replaces old messages with the summary.

---

## 5. Session Transcript Persistence

Save and restore conversation sessions to disk.

```rust
// src/services/session_transcript/mod.rs

pub struct SessionTranscript {
    pub session_id: String,
    pub messages: Vec<ApiMessage>,
    pub model: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Session storage directory: ~/.claude/sessions/
pub struct SessionStore {
    base_dir: PathBuf,
}

impl SessionStore {
    pub fn new() -> Result<Self>;
    pub fn save(&self, transcript: &SessionTranscript) -> Result<()>;
    pub fn load(&self, session_id: &str) -> Result<SessionTranscript>;
    pub fn list(&self) -> Result<Vec<SessionSummary>>;
    pub fn delete(&self, session_id: &str) -> Result<()>;
}
```

Format: JSONL file per session (`{session_id}.jsonl`). Each line is a serialized message.

---

## 6. Token Estimation

Estimate token counts without making API calls. Used for budget calculations and auto-compact triggers.

```rust
// src/services/token_estimation/mod.rs

pub fn estimate_tokens(text: &str) -> u64 {
    // Simple heuristic: ~4 chars per token for English, ~2 for CJK
    // This is intentionally approximate — exact tokenization requires
    // model-specific tokenizers which add heavy dependencies
    let char_count = text.chars().count() as u64;
    (char_count + 3) / 4 // round up
}

pub fn estimate_message_tokens(messages: &[ApiMessage]) -> u64 {
    messages.iter().map(|m| {
        m.content.iter().map(|c| match c {
            ContentBlock::Text { text } => estimate_tokens(text),
            ContentBlock::ToolUse { input, .. } => estimate_tokens(&input.to_string()),
            ContentBlock::ToolResult { content, .. } => estimate_tokens(content),
            ContentBlock::Image { .. } => 1000, // rough estimate for images
            _ => 0,
        }).sum::<u64>()
    }).sum::<u64>() + (messages.len() as u64 * 4) // overhead per message
}
```

---

## 7. File Structure

```
src/services/
├── api/
│   ├── mod.rs              # existing
│   ├── provider.rs          # existing (LlmProvider trait)
│   ├── openai_provider.rs   # existing
│   ├── provider_factory.rs  # NEW: create_provider()
│   └── codex_auth.rs        # NEW: read_codex_token()
├── tools/
│   ├── mod.rs               # NEW
│   └── executor.rs          # NEW: StreamingToolExecutor
├── compact/
│   └── mod.rs               # NEW: CompactService
├── session_transcript/
│   └── mod.rs               # NEW: SessionStore
├── token_estimation/
│   └── mod.rs               # NEW: estimate_tokens()
├── mcp/                     # existing
└── mod.rs                   # update: add new modules
```
