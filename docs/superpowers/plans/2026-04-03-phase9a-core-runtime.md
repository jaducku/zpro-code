# Phase 9A: Core Runtime Services Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make atom-code a working conversational CLI — multi-provider LLM connection (Codex OAuth + OpenAI-compatible), real query execution in TUI, session persistence, and token management.

**Architecture:** Add provider factory with Codex token reading, wire the query engine into the TUI event loop via `Arc<dyn LlmProvider>` + spawned tasks, add session storage and token estimation as independent service modules.

**Tech Stack:** tokio, reqwest, serde_json, chrono, futures, async-trait

**Spec:** `docs/superpowers/specs/2026-04-03-phase9a-core-runtime-design.md`

---

### Task 1: Codex auth token reader

**Files:**
- Create: `src/services/api/codex_auth.rs`
- Modify: `src/services/api/mod.rs`

- [ ] **Step 1: Write tests for codex_auth**

```rust
// src/services/api/codex_auth.rs — at bottom
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_auth_json() {
        let json = r#"{"access_token":"sk-test-12345","refresh_token":"rt-xyz"}"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let token = value["access_token"].as_str().unwrap();
        assert_eq!(token, "sk-test-12345");
    }

    #[test]
    fn test_parse_auth_json_missing_token() {
        let json = r#"{"refresh_token":"rt-xyz"}"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        assert!(value["access_token"].as_str().is_none());
    }

    #[test]
    fn test_read_codex_token_from_env() {
        std::env::set_var("CODEX_TOKEN", "env-token-123");
        let result = read_codex_token();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "env-token-123");
        std::env::remove_var("CODEX_TOKEN");
    }

    #[test]
    fn test_read_codex_token_empty_env() {
        std::env::set_var("CODEX_TOKEN", "");
        // With empty env, falls through to file check (which won't exist in test)
        // Just verify it doesn't return the empty string
        let result = read_codex_token();
        if let Ok(token) = &result {
            assert!(!token.is_empty());
        }
        std::env::remove_var("CODEX_TOKEN");
    }
}
```

- [ ] **Step 2: Implement codex_auth.rs**

```rust
// src/services/api/codex_auth.rs

/// Read Codex CLI OAuth token from environment or standard file locations.
///
/// Search order:
/// 1. CODEX_TOKEN environment variable
/// 2. ~/.codex/auth.json
/// 3. ~/.config/codex/auth.json
pub fn read_codex_token() -> anyhow::Result<String> {
    // 1. Environment variable
    if let Ok(token) = std::env::var("CODEX_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // 2-3. File-based token
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();

    let candidates = [
        format!("{}/.codex/auth.json", home),
        format!("{}/.config/codex/auth.json", home),
    ];

    for path in &candidates {
        if let Some(token) = read_token_from_file(path) {
            return Ok(token);
        }
    }

    Err(anyhow::anyhow!(
        "Codex OAuth token not found.\n\
         Run 'codex auth login' first, or set CODEX_TOKEN environment variable."
    ))
}

fn read_token_from_file(path: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json["access_token"].as_str().map(|s| s.to_string())
}
```

- [ ] **Step 3: Add `pub mod codex_auth;` to `src/services/api/mod.rs`**

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code services::api::codex_auth`

- [ ] **Step 5: Commit**

```bash
git add src/services/api/codex_auth.rs src/services/api/mod.rs
git commit -m "feat(api): add Codex CLI OAuth token reader"
```

---

### Task 2: Provider factory

**Files:**
- Create: `src/services/api/provider_factory.rs`
- Modify: `src/services/api/mod.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_provider_openai_requires_api_base() {
        let result = create_provider(Some("openai"), None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_provider_openai_with_base() {
        let result = create_provider(
            Some("openai"),
            Some("https://api.example.com/v1"),
            Some("sk-test"),
            Some("gpt-4"),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_none_requires_api_base() {
        let result = create_provider(None, None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_provider_none_with_base() {
        let result = create_provider(None, Some("http://localhost:8080/v1"), None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_unknown() {
        let result = create_provider(Some("bedrock"), None, None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown provider"));
    }

    #[test]
    fn test_create_provider_codex_with_env() {
        std::env::set_var("CODEX_TOKEN", "test-token-for-factory");
        let result = create_provider(Some("codex"), None, None, None);
        std::env::remove_var("CODEX_TOKEN");
        assert!(result.is_ok());
        let provider = result.unwrap();
        assert_eq!(provider.model_info().id, "gpt-4o");
    }
}
```

- [ ] **Step 2: Implement provider_factory.rs**

```rust
// src/services/api/provider_factory.rs
use super::codex_auth::read_codex_token;
use super::openai_provider::OpenAiCompatibleProvider;
use super::provider::LlmProvider;

/// Create an LLM provider based on the provider type and configuration.
///
/// Provider types:
/// - "codex": Uses Codex CLI OAuth token with OpenAI API
/// - "openai" or None: Uses OpenAI-compatible API with --api-base
pub fn create_provider(
    provider_type: Option<&str>,
    api_base: Option<&str>,
    api_key: Option<&str>,
    model: Option<&str>,
) -> anyhow::Result<Box<dyn LlmProvider>> {
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
            let base = api_base.ok_or_else(|| {
                anyhow::anyhow!("--api-base is required. Example: --api-base https://openrouter.ai/api/v1")
            })?;
            Ok(Box::new(OpenAiCompatibleProvider::new(
                base.into(),
                api_key.map(|s| s.to_string()),
                model.unwrap_or("default").into(),
            )))
        }
        Some(other) => Err(anyhow::anyhow!(
            "Unknown provider: '{}'. Available: codex, openai",
            other
        )),
    }
}
```

- [ ] **Step 3: Add to mod.rs and export**

```rust
pub mod provider_factory;
pub use provider_factory::create_provider;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code services::api::provider_factory`

- [ ] **Step 5: Commit**

```bash
git add src/services/api/provider_factory.rs src/services/api/mod.rs
git commit -m "feat(api): add multi-provider factory (codex + openai-compatible)"
```

---

### Task 3: Wire provider into main.rs

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add `--provider` CLI flag**

Add to the Cli struct:
```rust
/// LLM provider type: "codex" (OpenAI via Codex OAuth) or "openai" (OpenAI-compatible endpoint)
#[arg(long)]
provider: Option<String>,
```

- [ ] **Step 2: Replace provider creation with factory**

Replace the manual `OpenAiCompatibleProvider::new(...)` in main.rs with:
```rust
use atom_code::services::api::create_provider;

// In the None (TUI) branch:
let provider = create_provider(
    cli.provider.as_deref(),
    cli.api_base.as_deref(),
    cli.api_key.as_deref(),
    cli.model.as_deref(),
);

match provider {
    Ok(p) => {
        // Wrap in Arc for sharing across async tasks
        let provider = std::sync::Arc::new(p);
        // pass to run_app
    }
    Err(e) if cli.provider.is_some() || cli.api_base.is_some() => {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    Err(_) => {
        // No provider specified — run without LLM (browsing mode)
        // pass None to run_app
    }
}
```

- [ ] **Step 3: Update run_app signature to accept `Arc`**

Change `provider: Option<Box<dyn LlmProvider>>` to `provider: Option<std::sync::Arc<Box<dyn LlmProvider>>>`. Actually simpler: `provider: Option<std::sync::Arc<dyn LlmProvider>>`.

Read `src/tui/run.rs` to verify the current signature and update it.

- [ ] **Step 4: Run `cargo build -p atom-code`**

- [ ] **Step 5: Test manually**

Run: `CODEX_TOKEN=test cargo run -p atom-code -- --provider codex`
Expected: Starts TUI with "Connected to model: gpt-4o"

Run: `cargo run -p atom-code -- --provider openai --api-base http://localhost:11434/v1`
Expected: Starts TUI

Run: `cargo run -p atom-code`
Expected: Starts TUI with "No LLM provider connected"

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/tui/run.rs
git commit -m "feat(cli): add --provider flag with codex/openai multi-provider support"
```

---

### Task 4: Wire query engine into TUI event loop

**Files:**
- Modify: `src/tui/run.rs`
- Modify: `src/tui/app.rs` (minor — add Arc provider field)

This is the key task that makes LLM conversations actually work.

- [ ] **Step 1: Store provider Arc in App or run loop**

In run.rs, the `provider: Option<Arc<dyn LlmProvider>>` needs to be available for spawning query tasks. Store it alongside the event loop.

- [ ] **Step 2: Update `submit_query` to spawn actual query task**

Replace the placeholder in `submit_query` with real query execution:

```rust
fn submit_query(
    app: &mut App,
    input: &str,
    provider: &Option<Arc<dyn LlmProvider>>,
    tools: &[Box<dyn crate::tool::Tool>],
    system_prompt: &Option<Vec<SystemBlock>>,
    query_event_tx: &mpsc::UnboundedSender<QueryEvent>,
) {
    let Some(provider) = provider.clone() else {
        app.push_message(DisplayMessage {
            role: DisplayRole::System,
            content: "No LLM provider connected. Use --provider or --api-base.".into(),
            tool_name: None,
        });
        return;
    };

    // Add user message
    app.push_message(DisplayMessage {
        role: DisplayRole::User,
        content: input.to_string(),
        tool_name: None,
    });

    app.api_messages.push(ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text { text: input.to_string() }],
    });

    app.is_loading = true;

    // Build query context
    let ctx = QueryContext {
        messages: app.api_messages.clone(),
        tools: vec![], // Tools need to be Arc-wrapped for Send; simplify for now
        system_prompt: system_prompt.clone(),
        model: provider.model_info().id.clone(),
        max_tokens: provider.model_info().max_output_tokens,
    };

    let event_tx = query_event_tx.clone();
    let cancel = CancellationToken::new();

    // Spawn query task
    tokio::spawn(async move {
        match crate::query::engine::query(ctx, &*provider, event_tx.clone(), cancel).await {
            Ok(result) => {
                // Update api_messages with the full conversation
                // (handled via QueryEvent::ResponseComplete)
            }
            Err(e) => {
                let _ = event_tx.send(QueryEvent::Error(e.to_string()));
            }
        }
    });
}
```

- [ ] **Step 3: Update the event loop to pass provider and tools**

Change the `handle_submit` call to pass `provider` and `query_event_tx`. The query events are already handled by `app.handle_query_event()` in the `tokio::select!` branch.

- [ ] **Step 4: Handle api_messages update after response**

After `QueryEvent::ResponseComplete`, update `app.api_messages` with the assistant's response so the conversation continues correctly. Add to `handle_query_event` in app.rs:

```rust
QueryEvent::ResponseComplete { .. } => {
    self.is_loading = false;
    // The assistant's message was already accumulated via TextDelta
    // Add it to api_messages for the next turn
    if let Some(last) = self.messages.last() {
        if last.role == DisplayRole::Assistant {
            self.api_messages.push(ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::Text { text: last.content.clone() }],
            });
        }
    }
}
```

- [ ] **Step 5: Run `cargo check -p atom-code`**

- [ ] **Step 6: Test with a real provider** (manual)

Run: `cargo run -p atom-code -- --api-base https://openrouter.ai/api/v1 --api-key $OPENROUTER_KEY --model openai/gpt-4o-mini`
Type a message, verify streaming response appears.

- [ ] **Step 7: Commit**

```bash
git add src/tui/run.rs src/tui/app.rs
git commit -m "feat(tui): wire query engine into event loop for real LLM conversations"
```

---

### Task 5: Token estimation service

**Files:**
- Create: `src/services/token_estimation/mod.rs`
- Modify: `src/services/mod.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_english() {
        // ~4 chars per token
        assert!(estimate_tokens("hello world") > 0);
        assert!(estimate_tokens("hello world") <= 5);
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_message_tokens() {
        use crate::types::content::ContentBlock;
        use crate::types::message::{ApiMessage, Role};

        let messages = vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text { text: "Hello, how are you?".into() }],
        }];
        let tokens = estimate_message_tokens(&messages);
        assert!(tokens > 0);
        assert!(tokens < 100);
    }
}
```

- [ ] **Step 2: Implement**

```rust
// src/services/token_estimation/mod.rs

use crate::types::content::ContentBlock;
use crate::types::message::ApiMessage;

/// Estimate token count for a text string.
/// Uses a simple heuristic: ~4 characters per token.
pub fn estimate_tokens(text: &str) -> u64 {
    let len = text.len() as u64;
    if len == 0 { return 0; }
    (len + 3) / 4
}

/// Estimate total tokens for a list of API messages.
pub fn estimate_message_tokens(messages: &[ApiMessage]) -> u64 {
    let content_tokens: u64 = messages.iter().map(|m| {
        m.content.iter().map(|c| match c {
            ContentBlock::Text { text } => estimate_tokens(text),
            ContentBlock::ToolUse { input, .. } => estimate_tokens(&input.to_string()),
            ContentBlock::ToolResult { content, .. } => {
                match content {
                    crate::types::content::ToolResultContent::Text(t) => estimate_tokens(t),
                    crate::types::content::ToolResultContent::Blocks(blocks) => {
                        blocks.iter().map(|b| {
                            if let ContentBlock::Text { text } = b { estimate_tokens(text) } else { 0 }
                        }).sum()
                    }
                }
            }
            ContentBlock::Image { .. } => 1000,
            _ => 0,
        }).sum::<u64>()
    }).sum();

    // Add per-message overhead (~4 tokens each for role, formatting)
    content_tokens + (messages.len() as u64 * 4)
}
```

Note: Read `src/types/content.rs` first to verify the `ToolResultContent` enum shape. If it's just a `String`, simplify accordingly.

- [ ] **Step 3: Add `pub mod token_estimation;` to `src/services/mod.rs`**

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code services::token_estimation`

- [ ] **Step 5: Commit**

```bash
git add src/services/token_estimation/ src/services/mod.rs
git commit -m "feat(services): add token estimation service"
```

---

### Task 6: Session transcript persistence

**Files:**
- Create: `src/services/session_transcript/mod.rs`
- Modify: `src/services/mod.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_store_save_and_load() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::with_base_dir(dir.path().to_path_buf());

        let transcript = SessionTranscript {
            session_id: "test-session-1".into(),
            messages: vec![],
            model: "gpt-4o".into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        store.save(&transcript).unwrap();
        let loaded = store.load("test-session-1").unwrap();
        assert_eq!(loaded.session_id, "test-session-1");
        assert_eq!(loaded.model, "gpt-4o");
    }

    #[test]
    fn test_session_store_list() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::with_base_dir(dir.path().to_path_buf());

        for id in &["s1", "s2", "s3"] {
            store.save(&SessionTranscript {
                session_id: id.to_string(),
                messages: vec![],
                model: "test".into(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }).unwrap();
        }

        let sessions = store.list().unwrap();
        assert_eq!(sessions.len(), 3);
    }

    #[test]
    fn test_session_store_load_nonexistent() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::with_base_dir(dir.path().to_path_buf());
        assert!(store.load("nonexistent").is_err());
    }
}
```

- [ ] **Step 2: Implement**

```rust
// src/services/session_transcript/mod.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::types::message::ApiMessage;

#[derive(Serialize, Deserialize)]
pub struct SessionTranscript {
    pub session_id: String,
    pub messages: Vec<ApiMessage>,
    pub model: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct SessionSummary {
    pub session_id: String,
    pub model: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub message_count: usize,
}

pub struct SessionStore {
    base_dir: PathBuf,
}

impl SessionStore {
    /// Create store at default location (~/.claude/sessions/)
    pub fn new() -> anyhow::Result<Self> {
        let home = std::env::var("HOME").unwrap_or_default();
        let base_dir = PathBuf::from(home).join(".claude").join("sessions");
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    /// Create store at a custom directory (for testing)
    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&base_dir).ok();
        Self { base_dir }
    }

    pub fn save(&self, transcript: &SessionTranscript) -> anyhow::Result<()> {
        let path = self.base_dir.join(format!("{}.json", transcript.session_id));
        let json = serde_json::to_string_pretty(transcript)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(&self, session_id: &str) -> anyhow::Result<SessionTranscript> {
        let path = self.base_dir.join(format!("{}.json", session_id));
        let content = std::fs::read_to_string(&path)
            .map_err(|_| anyhow::anyhow!("Session '{}' not found", session_id))?;
        let transcript: SessionTranscript = serde_json::from_str(&content)?;
        Ok(transcript)
    }

    pub fn list(&self) -> anyhow::Result<Vec<SessionSummary>> {
        let mut summaries = Vec::new();
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(t) = serde_json::from_str::<SessionTranscript>(&content) {
                        summaries.push(SessionSummary {
                            session_id: t.session_id,
                            model: t.model,
                            created_at: t.created_at,
                            message_count: t.messages.len(),
                        });
                    }
                }
            }
        }
        summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(summaries)
    }

    pub fn delete(&self, session_id: &str) -> anyhow::Result<()> {
        let path = self.base_dir.join(format!("{}.json", session_id));
        std::fs::remove_file(path)?;
        Ok(())
    }
}
```

Note: Read `src/types/message.rs` first to verify `ApiMessage` has `Serialize`/`Deserialize`. If not, add `#[derive(Serialize, Deserialize)]` to it.

- [ ] **Step 3: Add `pub mod session_transcript;` to `src/services/mod.rs`**

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code services::session_transcript`

- [ ] **Step 5: Commit**

```bash
git add src/services/session_transcript/ src/services/mod.rs
git commit -m "feat(services): add session transcript persistence"
```

---

### Task 7: Auto-compact service

**Files:**
- Create: `src/services/compact/mod.rs`
- Modify: `src/services/mod.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_compaction_under_threshold() {
        assert!(!needs_compaction(1000, 128_000));
    }

    #[test]
    fn test_needs_compaction_over_threshold() {
        // 80% of 128K = 102,400
        assert!(needs_compaction(110_000, 128_000));
    }

    #[test]
    fn test_needs_compaction_at_boundary() {
        assert!(needs_compaction(102_401, 128_000));
        assert!(!needs_compaction(102_399, 128_000));
    }
}
```

- [ ] **Step 2: Implement**

```rust
// src/services/compact/mod.rs

use crate::types::message::ApiMessage;

/// Check if compaction is needed based on token count vs context window.
/// Triggers at 80% capacity.
pub fn needs_compaction(current_tokens: u64, context_window: u64) -> bool {
    let threshold = context_window * 80 / 100;
    current_tokens > threshold
}

/// Result of a compaction operation
pub struct CompactResult {
    pub summary: String,
    pub messages_removed: usize,
    pub tokens_before: u64,
    pub tokens_after: u64,
}

/// Build a compaction prompt to summarize conversation history.
/// The summary is sent to the LLM, and the result replaces older messages.
pub fn build_compact_prompt(
    messages: &[ApiMessage],
    custom_instructions: Option<&str>,
) -> String {
    let base = "Summarize the conversation so far in a concise way that preserves \
                all important context, decisions made, files modified, and current goals. \
                The summary will replace the older messages to free up context space.";

    match custom_instructions {
        Some(instructions) => format!("{}\n\nAdditional instructions: {}", base, instructions),
        None => base.to_string(),
    }
}
```

- [ ] **Step 3: Add `pub mod compact;` to `src/services/mod.rs`**

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code services::compact`

- [ ] **Step 5: Commit**

```bash
git add src/services/compact/ src/services/mod.rs
git commit -m "feat(services): add auto-compact service with token budget management"
```

---

### Task 8: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace -- --include-ignored`

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`

- [ ] **Step 3: Verify --provider codex**

Run: `CODEX_TOKEN=fake cargo run -p atom-code -- --provider codex --help`
Expected: Shows help (provider created but no API call made)

- [ ] **Step 4: Verify --api-base**

Run: `cargo run -p atom-code -- --api-base http://localhost:11434/v1 --model llama3`
Expected: Starts TUI with "Connected to model: llama3"

- [ ] **Step 5: Verify no-provider mode**

Run: `cargo run -p atom-code`
Expected: Starts TUI with "No LLM provider connected"

- [ ] **Step 6: Fix any issues and commit**

```bash
git commit -m "fix: address clippy warnings and test failures in Phase 9A"
```
