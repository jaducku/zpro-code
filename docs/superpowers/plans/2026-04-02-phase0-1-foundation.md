# Phase 0+1: Foundation — Project Scaffolding + Core Types

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Set up Cargo workspace and define all core types that every subsequent phase depends on.

**Architecture:** Single Rust crate with module hierarchy mirroring the original TS `src/` layout. `packages/` as workspace member crates. All core types use `serde` for serialization, matching the original TS interfaces field-by-field.

**Tech Stack:** Rust, serde, serde_json, uuid, tokio, thiserror

**Spec:** `docs/superpowers/specs/2026-04-02-ts-to-rust-migration-design.md`

---

### Task 1: Initialize Cargo workspace

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `.rustfmt.toml`
- Create: `.clippy.toml`
- Modify: `.gitignore`

- [ ] **Step 1: Initialize Cargo project**

```bash
cd /Users/zpro/02_dev/02_oss/cc-workspace/atom-code-rs
cargo init --name atom-code
```

- [ ] **Step 2: Configure workspace Cargo.toml**

Replace `Cargo.toml` with:

```toml
[workspace]
members = [
    ".",
]
resolver = "2"

# NOTE: packages/* workspace members are added in Task 2 after their Cargo.toml files exist.
# Requires Rust 1.85+ (edition 2024).

[package]
name = "atom-code"
version = "0.1.0"
edition = "2021"
description = "Open LLM-powered coding assistant CLI"
license = "MIT"

[[bin]]
name = "atom-code"
path = "src/main.rs"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["rt"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"

# CLI
clap = { version = "4", features = ["derive"] }

# TUI
ratatui = "0.29"
crossterm = "0.28"

# HTTP
reqwest = { version = "0.12", features = ["json", "stream"] }
reqwest-eventsource = "0.6"

# WebSocket
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }

# Error handling
thiserror = "2"
anyhow = "1"

# Utilities
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
semver = "1"
globset = "0.4"
ignore = "0.4"
similar = "2"
syntect = "5"
pulldown-cmark = "0.12"
shell-words = "1"
strip-ansi-escapes = "0.2"
lru = "0.12"
fuzzy-matcher = "0.3"
notify = "7"
async-trait = "0.1"
pin-project-lite = "0.2"
futures = "0.3"
tracing = "0.1"
tracing-subscriber = "0.3"

# Image
image = "0.25"

# Platform
plist = "1"
qrcode = "0.14"

# OpenTelemetry
opentelemetry = "0.27"

[dev-dependencies]
tokio-test = "0.4"
pretty_assertions = "1"
tempfile = "3"
```

- [ ] **Step 3: Create lib.rs with module declarations**

```rust
// src/lib.rs
pub mod types;
pub mod state;
pub mod utils;
pub mod constants;
```

- [ ] **Step 4: Create minimal main.rs**

```rust
// src/main.rs
use clap::Parser;

#[derive(Parser)]
#[command(name = "atom-code", version)]
struct Cli {
    /// Initial prompt
    prompt: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    println!("atom-code v{}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
```

- [ ] **Step 5: Create rustfmt and clippy configs**

`.rustfmt.toml`:
```toml
edition = "2021"
max_width = 100
use_field_init_shorthand = true
```

`.clippy.toml`:
```toml
too-many-arguments-threshold = 10
```

- [ ] **Step 6: Update .gitignore for Rust**

Append to `.gitignore`:
```
# Rust
/target
```

Note: `Cargo.lock` is committed for binary crates (Rust convention).

- [ ] **Step 7: Build and verify**

Run: `cargo build`
Expected: Compiles successfully

Run: `cargo run`
Expected: Prints `atom-code v0.1.0`

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml src/main.rs src/lib.rs .rustfmt.toml .clippy.toml .gitignore
git commit -m "feat: initialize Cargo workspace with core dependencies"
```

---

### Task 2: Create directory structure

**Files:**
- Create: module directories and `mod.rs` files

- [ ] **Step 1: Create all source directories**

```bash
cd /Users/zpro/02_dev/02_oss/cc-workspace/atom-code-rs

# Core modules
mkdir -p src/types
mkdir -p src/state
mkdir -p src/utils/settings
mkdir -p src/utils/permissions
mkdir -p src/utils/auth
mkdir -p src/constants

# Service modules
mkdir -p src/services/api
mkdir -p src/services/mcp
mkdir -p src/services/compact
mkdir -p src/services/session_memory
mkdir -p src/services/session_transcript
mkdir -p src/services/oauth
mkdir -p src/services/plugins
mkdir -p src/services/skill_search
mkdir -p src/services/policy_limits
mkdir -p src/services/analytics
mkdir -p src/services/extract_memories
mkdir -p src/services/lsp
mkdir -p src/services/tools
mkdir -p src/services/team_memory_sync
mkdir -p src/services/remote_managed_settings
mkdir -p src/services/settings_sync
mkdir -p src/services/auto_dream
mkdir -p src/services/prompt_suggestion
mkdir -p src/services/magic_docs
mkdir -p src/services/tips
mkdir -p src/services/context_collapse
mkdir -p src/services/tool_use_summary
mkdir -p src/services/agent_summary

# Feature modules
mkdir -p src/tools
mkdir -p src/tui/widgets
mkdir -p src/tui/screens
mkdir -p src/components
mkdir -p src/screens
mkdir -p src/hooks
mkdir -p src/commands
mkdir -p src/context
mkdir -p src/keybindings
mkdir -p src/vim
mkdir -p src/memdir
mkdir -p src/skills
mkdir -p src/tasks
mkdir -p src/bridge
mkdir -p src/buddy
mkdir -p src/assistant
mkdir -p src/bootstrap
mkdir -p src/cli
mkdir -p src/entrypoints
mkdir -p src/coordinator
mkdir -p src/remote
mkdir -p src/server
mkdir -p src/ssh
mkdir -p src/daemon
mkdir -p src/migrations
mkdir -p src/upstreamproxy
mkdir -p src/schemas
mkdir -p src/plugins
mkdir -p src/output_styles
mkdir -p src/file_index
mkdir -p src/query

# Package crates
mkdir -p packages/color-diff/src
mkdir -p packages/audio-capture/src
mkdir -p packages/image-processor/src
mkdir -p packages/modifiers/src
mkdir -p packages/url-handler/src
```

- [ ] **Step 2: Create package Cargo.toml files**

`packages/color-diff/Cargo.toml`:
```toml
[package]
name = "atom-color-diff"
version = "0.1.0"
edition = "2024"

[dependencies]
```

`packages/audio-capture/Cargo.toml`:
```toml
[package]
name = "atom-audio-capture"
version = "0.1.0"
edition = "2024"

[dependencies]
```

`packages/image-processor/Cargo.toml`:
```toml
[package]
name = "atom-image-processor"
version = "0.1.0"
edition = "2024"

[dependencies]
image = "0.25"
```

`packages/modifiers/Cargo.toml`:
```toml
[package]
name = "atom-modifiers"
version = "0.1.0"
edition = "2024"

[dependencies]
```

`packages/url-handler/Cargo.toml`:
```toml
[package]
name = "atom-url-handler"
version = "0.1.0"
edition = "2021"

[dependencies]
```

Each with `src/lib.rs`:
```rust
// placeholder
```

- [ ] **Step 3: Add workspace members to root Cargo.toml**

Update `[workspace]` section in root `Cargo.toml`:
```toml
[workspace]
members = [
    ".",
    "packages/color-diff",
    "packages/audio-capture",
    "packages/image-processor",
    "packages/modifiers",
    "packages/url-handler",
]
resolver = "2"
```

- [ ] **Step 4: Build workspace**

Run: `cargo build --workspace`
Expected: All crates compile

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: create full directory structure and package crates"
```

---

### Task 3: Define error types

**Files:**
- Create: `src/types/error.rs`
- Create: `src/types/mod.rs`

- [ ] **Step 1: Write error type tests**

Create `src/types/error.rs`:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AtomError {
    #[error("API error: {message}")]
    Api { message: String, status: Option<u16> },

    #[error("Tool error: {tool_name}: {message}")]
    Tool { tool_name: String, message: String },

    #[error("MCP error: {server_name}: {message}")]
    Mcp { server_name: String, message: String },

    #[error("Permission denied: {tool_name}")]
    PermissionDenied { tool_name: String },

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Token budget exceeded: {used}/{limit}")]
    TokenBudget { used: u64, limit: u64 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Operation cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, AtomError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_display() {
        let err = AtomError::Api {
            message: "rate limited".into(),
            status: Some(429),
        };
        assert_eq!(err.to_string(), "API error: rate limited");
    }

    #[test]
    fn test_tool_error_display() {
        let err = AtomError::Tool {
            tool_name: "BashTool".into(),
            message: "command failed".into(),
        };
        assert_eq!(err.to_string(), "Tool error: BashTool: command failed");
    }

    #[test]
    fn test_token_budget_display() {
        let err = AtomError::TokenBudget {
            used: 200000,
            limit: 128000,
        };
        assert_eq!(err.to_string(), "Token budget exceeded: 200000/128000");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: AtomError = io_err.into();
        assert!(matches!(err, AtomError::Io(_)));
    }
}
```

- [ ] **Step 2: Create types/mod.rs**

```rust
// src/types/mod.rs
pub mod error;

pub use error::{AtomError, Result};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code types::error`
Expected: 4 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/types/
git commit -m "feat: define core error types with thiserror"
```

---

### Task 4: Define content block types

**Files:**
- Create: `src/types/content.rs`

- [ ] **Step 1: Write content block types with tests**

```rust
// src/types/content.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: ToolResultContent,
        #[serde(default)]
        is_error: bool,
    },

    #[serde(rename = "image")]
    Image { source: ImageSource },

    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl Default for ToolResultContent {
    fn default() -> Self {
        ToolResultContent::Text(String::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ImageSource {
    #[serde(rename = "base64")]
    Base64 {
        media_type: String,
        data: String,
    },
    #[serde(rename = "url")]
    Url { url: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_block_serde() {
        let block = ContentBlock::Text {
            text: "hello".into(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "hello");

        let parsed: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, block);
    }

    #[test]
    fn test_tool_use_block_serde() {
        let block = ContentBlock::ToolUse {
            id: "tu_123".into(),
            name: "BashTool".into(),
            input: serde_json::json!({"command": "ls"}),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_use");
        assert_eq!(json["name"], "BashTool");
        assert_eq!(json["input"]["command"], "ls");

        let parsed: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, block);
    }

    #[test]
    fn test_tool_result_block_serde() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "tu_123".into(),
            content: ToolResultContent::Text("output".into()),
            is_error: false,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_result");

        let parsed: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, block);
    }

    #[test]
    fn test_image_base64_serde() {
        let block = ContentBlock::Image {
            source: ImageSource::Base64 {
                media_type: "image/png".into(),
                data: "iVBOR...".into(),
            },
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "image");
        assert_eq!(json["source"]["type"], "base64");
    }

    #[test]
    fn test_thinking_block_serde() {
        let block = ContentBlock::Thinking {
            thinking: "Let me analyze...".into(),
            signature: Some("sig_abc".into()),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "thinking");
    }
}
```

- [ ] **Step 2: Add to types/mod.rs**

```rust
pub mod content;
pub mod error;

pub use error::{AtomError, Result};
pub use content::{ContentBlock, ImageSource, ToolResultContent};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code types::content`
Expected: 5 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/types/content.rs src/types/mod.rs
git commit -m "feat: define content block types (text, tool_use, tool_result, image, thinking)"
```

---

### Task 5: Define message types

**Files:**
- Create: `src/types/message.rs`

- [ ] **Step 1: Write message types with tests**

```rust
// src/types/message.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::content::ContentBlock;

/// API-level message (for LLM communication)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMessage {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// Internal message types used throughout the application
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "user")]
    User(UserMessage),

    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),

    #[serde(rename = "system")]
    System(SystemMessage),

    #[serde(rename = "attachment")]
    Attachment(AttachmentMessage),

    #[serde(rename = "progress")]
    Progress(ProgressMessage),

    #[serde(rename = "grouped_tool_use")]
    GroupedToolUse(GroupedToolUseMessage),

    #[serde(rename = "collapsed_read_search")]
    CollapsedReadSearch(CollapsedReadSearchMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub uuid: Uuid,
    pub message: ApiMessage,
    #[serde(default)]
    pub is_meta: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub uuid: Uuid,
    pub message: ApiMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_agent_id: Option<String>,
    #[serde(default)]
    pub is_compact_summary: bool,
}

/// System messages with subtypes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype")]
pub enum SystemMessage {
    #[serde(rename = "local_command")]
    LocalCommand {
        uuid: Uuid,
        #[serde(flatten)]
        data: serde_json::Value,
    },
    #[serde(rename = "compact_boundary")]
    CompactBoundary { uuid: Uuid },
    #[serde(rename = "microcompact_boundary")]
    MicrocompactBoundary { uuid: Uuid },
    #[serde(rename = "api_error")]
    ApiError {
        uuid: Uuid,
        error_message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        status_code: Option<u16>,
    },
    #[serde(rename = "file_snapshot")]
    FileSnapshot {
        uuid: Uuid,
        #[serde(flatten)]
        data: serde_json::Value,
    },
    #[serde(rename = "agents_killed")]
    AgentsKilled { uuid: Uuid },
    #[serde(rename = "api_metrics")]
    ApiMetrics {
        uuid: Uuid,
        #[serde(flatten)]
        metrics: serde_json::Value,
    },
    #[serde(rename = "away_summary")]
    AwaySummary {
        uuid: Uuid,
        summary: String,
    },
    #[serde(rename = "bridge_status")]
    BridgeStatus {
        uuid: Uuid,
        status: String,
    },
    #[serde(rename = "informational")]
    Informational {
        uuid: Uuid,
        text: String,
    },
    #[serde(rename = "memory_saved")]
    MemorySaved {
        uuid: Uuid,
        memory_path: String,
    },
    #[serde(rename = "permission_retry")]
    PermissionRetry { uuid: Uuid },
    #[serde(rename = "scheduled_task_fire")]
    ScheduledTaskFire {
        uuid: Uuid,
        task_id: String,
    },
    #[serde(rename = "stop_hook_summary")]
    StopHookSummary {
        uuid: Uuid,
        #[serde(flatten)]
        data: serde_json::Value,
    },
    #[serde(rename = "turn_duration")]
    TurnDuration {
        uuid: Uuid,
        duration_ms: u64,
    },
    #[serde(rename = "thinking")]
    Thinking {
        uuid: Uuid,
        thinking: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMessage {
    pub uuid: Uuid,
    pub attachment: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressMessage {
    pub uuid: Uuid,
    pub tool_use_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedToolUseMessage {
    pub uuid: Uuid,
    pub tool_name: String,
    pub messages: Vec<Message>,
    pub results: Vec<Message>,
    pub display_message: Box<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollapsedReadSearchMessage {
    pub uuid: Uuid,
    #[serde(default)]
    pub search_count: u32,
    #[serde(default)]
    pub read_count: u32,
    #[serde(default)]
    pub list_count: u32,
    #[serde(default)]
    pub repl_count: u32,
    #[serde(default)]
    pub memory_search_count: u32,
    #[serde(default)]
    pub memory_read_count: u32,
    #[serde(default)]
    pub memory_write_count: u32,
    pub read_file_paths: Vec<String>,
    pub search_args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_display_hint: Option<String>,
    pub messages: Vec<Message>,
    pub display_message: Box<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_call_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_server_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bash_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_total_ms: Option<u32>,
}

/// Stop reason from LLM provider (unified across providers)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_message_creation() {
        let msg = UserMessage {
            uuid: Uuid::new_v4(),
            message: ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "hello".into(),
                }],
            },
            is_meta: false,
            tool_use_result: None,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["message"]["role"], "user");
        assert_eq!(json["message"]["content"][0]["text"], "hello");
    }

    #[test]
    fn test_assistant_message_creation() {
        let msg = AssistantMessage {
            uuid: Uuid::new_v4(),
            message: ApiMessage {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Text {
                        text: "Let me help".into(),
                    },
                    ContentBlock::ToolUse {
                        id: "tu_1".into(),
                        name: "BashTool".into(),
                        input: serde_json::json!({"command": "ls"}),
                    },
                ],
            },
            source_agent_id: None,
            is_compact_summary: false,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["message"]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_role_serde() {
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
        assert_eq!(serde_json::to_string(&Role::Assistant).unwrap(), "\"assistant\"");
        assert_eq!(serde_json::to_string(&Role::Tool).unwrap(), "\"tool\"");
    }

    #[test]
    fn test_system_message_serde() {
        let sys_msg = SystemMessage::ApiError {
            uuid: Uuid::new_v4(),
            error_message: "rate limited".into(),
            status_code: Some(429),
        };
        // Wrap in Message enum to test nested tag serialization
        let msg = Message::System(sys_msg);
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "system");
        assert_eq!(json["subtype"], "api_error");
        assert_eq!(json["error_message"], "rate limited");
        assert_eq!(json["status_code"], 429);

        // Roundtrip
        let parsed: Message = serde_json::from_value(json).unwrap();
        match parsed {
            Message::System(SystemMessage::ApiError { error_message, status_code, .. }) => {
                assert_eq!(error_message, "rate limited");
                assert_eq!(status_code, Some(429));
            }
            _ => panic!("Expected System::ApiError"),
        }
    }

    #[test]
    fn test_system_informational_serde() {
        let msg = Message::System(SystemMessage::Informational {
            uuid: Uuid::new_v4(),
            text: "hello".into(),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "system");
        assert_eq!(json["subtype"], "informational");

        let parsed: Message = serde_json::from_value(json).unwrap();
        assert!(matches!(parsed, Message::System(SystemMessage::Informational { .. })));
    }

    #[test]
    fn test_stop_reason() {
        let reasons = vec![
            StopReason::EndTurn,
            StopReason::ToolUse,
            StopReason::MaxTokens,
            StopReason::StopSequence,
        ];
        for reason in reasons {
            let json = serde_json::to_string(&reason).unwrap();
            let parsed: StopReason = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, reason);
        }
    }
}
```

- [ ] **Step 2: Add to types/mod.rs**

```rust
pub mod content;
pub mod error;
pub mod message;

pub use error::{AtomError, Result};
pub use content::{ContentBlock, ImageSource, ToolResultContent};
pub use message::{
    ApiMessage, AssistantMessage, AttachmentMessage, CollapsedReadSearchMessage,
    GroupedToolUseMessage, Message, ProgressMessage, Role, StopReason, SystemMessage, UserMessage,
};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code types::message`
Expected: 4 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/types/message.rs src/types/mod.rs
git commit -m "feat: define message types (user, assistant, system 16 subtypes, attachment, progress, grouped, collapsed)"
```

---

### Task 6: Define permission types

**Files:**
- Create: `src/types/permissions.rs`

- [ ] **Step 1: Write permission types with tests**

```rust
// src/types/permissions.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionMode {
    #[serde(rename = "acceptEdits")]
    AcceptEdits,
    #[serde(rename = "bypassPermissions")]
    BypassPermissions,
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "dontAsk")]
    DontAsk,
    #[serde(rename = "plan")]
    Plan,
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "bubble")]
    Bubble,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermissionContext {
    pub mode: PermissionMode,
    #[serde(default)]
    pub additional_working_directories: HashMap<String, String>,
    #[serde(default)]
    pub always_allow_rules: Vec<PermissionRule>,
    #[serde(default)]
    pub always_deny_rules: Vec<PermissionRule>,
    #[serde(default)]
    pub always_ask_rules: Vec<PermissionRule>,
    #[serde(default)]
    pub is_bypass_permissions_mode_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_auto_mode_available: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub should_avoid_permission_prompts: Option<bool>,
}

impl Default for ToolPermissionContext {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Default,
            additional_working_directories: HashMap::new(),
            always_allow_rules: Vec::new(),
            always_deny_rules: Vec::new(),
            always_ask_rules: Vec::new(),
            is_bypass_permissions_mode_available: false,
            is_auto_mode_available: None,
            should_avoid_permission_prompts: None,
        }
    }
}

/// Result of a permission check
#[derive(Debug, Clone)]
pub enum PermissionResult {
    Allowed,
    Denied,
    Ask(PermissionRequest),
}

/// Information shown to user when asking for permission
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub description: String,
    pub metadata: PermissionMeta,
}

/// Metadata for permission checking (file paths, bash commands, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_mode_serde() {
        let mode = PermissionMode::AcceptEdits;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"acceptEdits\"");

        let parsed: PermissionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, mode);
    }

    #[test]
    fn test_permission_mode_all_variants() {
        let variants = vec![
            (PermissionMode::AcceptEdits, "\"acceptEdits\""),
            (PermissionMode::BypassPermissions, "\"bypassPermissions\""),
            (PermissionMode::Default, "\"default\""),
            (PermissionMode::DontAsk, "\"dontAsk\""),
            (PermissionMode::Plan, "\"plan\""),
            (PermissionMode::Auto, "\"auto\""),
            (PermissionMode::Bubble, "\"bubble\""),
        ];
        for (mode, expected) in variants {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_permission_context_default() {
        let ctx = ToolPermissionContext::default();
        assert_eq!(ctx.mode, PermissionMode::Default);
        assert!(ctx.always_allow_rules.is_empty());
    }

    #[test]
    fn test_permission_rule_serde() {
        let rule = PermissionRule {
            tool_name: "BashTool".into(),
            rule_content: Some("ls *".into()),
        };
        let json = serde_json::to_value(&rule).unwrap();
        assert_eq!(json["tool_name"], "BashTool");
        assert_eq!(json["rule_content"], "ls *");
    }
}
```

- [ ] **Step 2: Add to types/mod.rs**

Add `pub mod permissions;` and re-exports:
```rust
pub use permissions::{
    PermissionBehavior, PermissionMeta, PermissionMode, PermissionRequest,
    PermissionResult, PermissionRule, ToolPermissionContext,
};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code types::permissions`
Expected: 4 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/types/permissions.rs src/types/mod.rs
git commit -m "feat: define permission types (mode, rules, context, request)"
```

---

### Task 7: Define settings types

**Files:**
- Create: `src/utils/settings/types.rs`
- Create: `src/utils/settings/mod.rs`
- Create: `src/utils/mod.rs`

- [ ] **Step 1: Write settings types with tests**

```rust
// src/utils/settings/types.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsJson {
    #[serde(rename = "$schema")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_helper: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_credential_export: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gcp_auth_refresh: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_suggestion: Option<FileSuggestion>,

    #[serde(default = "default_true")]
    pub respect_gitignore: bool, // NOTE: custom Default impl below sets this to true

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_period_days: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<Attribution>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_models: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mcp_servers: Option<Vec<McpServerEntry>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub denied_mcp_servers: Option<Vec<McpServerEntry>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_mode: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_thinking_enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_all_project_mcp_servers: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_memory_enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_memory_directory: Option<String>,

    /// Passthrough for unknown fields
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

fn default_true() -> bool {
    true
}

impl Default for SettingsJson {
    fn default() -> Self {
        Self {
            respect_gitignore: true, // intentionally true, matching serde default
            schema: None,
            api_key_helper: None,
            aws_credential_export: None,
            gcp_auth_refresh: None,
            file_suggestion: None,
            cleanup_period_days: None,
            env: None,
            attribution: None,
            permissions: None,
            model: None,
            available_models: None,
            allowed_mcp_servers: None,
            denied_mcp_servers: None,
            hooks: None,
            worktree: None,
            agent: None,
            fast_mode: None,
            always_thinking_enabled: None,
            enable_all_project_mcp_servers: None,
            auto_memory_enabled: None,
            auto_memory_directory: None,
            extra: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSuggestion {
    #[serde(rename = "type")]
    pub suggestion_type: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribution {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<crate::types::permissions::PermissionRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<crate::types::permissions::PermissionRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<Vec<crate::types::permissions::PermissionRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_directories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sparse_paths: Option<Vec<String>>,
}

/// Where a setting value came from
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsSource {
    Default,
    GlobalFile,
    ProjectFile,
    ManagedPolicy,
    Environment,
    CliFlag,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default() {
        let settings = SettingsJson::default();
        assert!(settings.respect_gitignore); // Custom Default impl sets true
    }

    #[test]
    fn test_settings_parse_minimal() {
        let json = r#"{}"#;
        let settings: SettingsJson = serde_json::from_str(json).unwrap();
        assert!(settings.model.is_none());
        assert!(settings.extra.is_empty());
    }

    #[test]
    fn test_settings_parse_with_model() {
        let json = r#"{"model": "claude-sonnet-4-20250514"}"#;
        let settings: SettingsJson = serde_json::from_str(json).unwrap();
        assert_eq!(settings.model.as_deref(), Some("claude-sonnet-4-20250514"));
    }

    #[test]
    fn test_settings_passthrough_unknown_fields() {
        let json = r#"{"model": "test", "customField": 42}"#;
        let settings: SettingsJson = serde_json::from_str(json).unwrap();
        assert_eq!(settings.extra["customField"], 42);
    }

    #[test]
    fn test_settings_roundtrip() {
        let settings = SettingsJson {
            model: Some("test-model".into()),
            fast_mode: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: SettingsJson = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, settings.model);
        assert_eq!(parsed.fast_mode, settings.fast_mode);
    }

    #[test]
    fn test_mcp_server_entry() {
        let json = r#"{"server_name": "test", "server_command": ["npx", "mcp-server"]}"#;
        let entry: McpServerEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.server_name.as_deref(), Some("test"));
        assert_eq!(entry.server_command.as_ref().unwrap().len(), 2);
    }
}
```

- [ ] **Step 2: Create mod files**

`src/utils/settings/mod.rs`:
```rust
pub mod types;

pub use types::{SettingsJson, SettingsSource};
```

`src/utils/mod.rs`:
```rust
pub mod settings;
```

- [ ] **Step 3: Update lib.rs**

Ensure `pub mod utils;` is present in `src/lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code utils::settings`
Expected: 6 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/utils/ src/lib.rs
git commit -m "feat: define settings types with serde (SettingsJson, MCP, permissions, worktree)"
```

---

### Task 8: Define tool trait and types

**Files:**
- Create: `src/tool.rs`

- [ ] **Step 1: Write tool trait and types**

```rust
// src/tool.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::types::permissions::PermissionMeta;

/// Core tool trait — all 52 built-in tools + MCP tools implement this
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name as sent to the LLM
    fn name(&self) -> &str;

    /// Tool description for LLM context
    fn description(&self) -> &str;

    /// JSON schema for tool input
    fn input_schema(&self) -> serde_json::Value;

    /// Execute the tool with given context
    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult>;

    /// Check if the tool can be used in current context
    fn can_use(&self, _ctx: &ToolUseContext) -> ValidationResult {
        ValidationResult::Ok
    }

    /// Permission metadata for this tool invocation
    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        let _ = input;
        PermissionMeta {
            file_path: None,
            command: None,
            url: None,
        }
    }

    /// Whether the tool is read-only (no side effects)
    fn is_read_only(&self) -> bool {
        false
    }

    /// Whether tool calls can run concurrently
    fn is_concurrent_safe(&self) -> bool {
        false
    }

    /// Aliases for tool search
    fn aliases(&self) -> &[&str] {
        &[]
    }

    /// Whether this is an MCP tool
    fn is_mcp(&self) -> bool {
        false
    }
}

/// Context passed to tool execution
pub struct ToolUseContext {
    pub input: serde_json::Value,
    pub abort_signal: CancellationToken,
    /// Channel for sending UI updates during execution
    pub progress_tx: tokio::sync::mpsc::UnboundedSender<ToolProgress>,
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

/// Progress update from a running tool
#[derive(Debug, Clone)]
pub enum ToolProgress {
    Output(String),
    Partial(String),
    Complete,
}

/// Validation result for can_use check
#[derive(Debug)]
pub enum ValidationResult {
    Ok,
    Disabled(String),
    Error(String),
}

/// Tool definition sent to LLM API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A tool call extracted from assistant response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Tool result block sent back to LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

impl ToolResultBlock {
    pub fn success(tool_use_id: String, content: String) -> Self {
        Self {
            tool_use_id,
            content,
            is_error: false,
        }
    }

    pub fn error(tool_use_id: String, message: String) -> Self {
        Self {
            tool_use_id,
            content: message,
            is_error: true,
        }
    }
}

impl From<&dyn Tool> for ToolDefinition {
    fn from(tool: &dyn Tool) -> Self {
        ToolDefinition {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_block_success() {
        let block = ToolResultBlock::success("tu_1".into(), "output".into());
        assert!(!block.is_error);
        assert_eq!(block.content, "output");
    }

    #[test]
    fn test_tool_result_block_error() {
        let block = ToolResultBlock::error("tu_1".into(), "failed".into());
        assert!(block.is_error);
        assert_eq!(block.content, "failed");
    }

    #[test]
    fn test_tool_definition_serde() {
        let def = ToolDefinition {
            name: "BashTool".into(),
            description: "Run bash commands".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"}
                },
                "required": ["command"]
            }),
        };
        let json = serde_json::to_value(&def).unwrap();
        assert_eq!(json["name"], "BashTool");
        assert_eq!(json["input_schema"]["type"], "object");
    }

    #[test]
    fn test_tool_call_serde() {
        let call = ToolCall {
            id: "tu_abc123".into(),
            name: "FileReadTool".into(),
            input: serde_json::json!({"file_path": "/tmp/test.rs"}),
        };
        let json = serde_json::to_string(&call).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "FileReadTool");
    }
}
```

- [ ] **Step 2: Add to lib.rs**

Add `pub mod tool;` to `src/lib.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code tool`
Expected: 4 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/tool.rs src/lib.rs
git commit -m "feat: define Tool trait, ToolUseContext, ToolResult, ToolDefinition"
```

---

### Task 9: Define LLM provider trait

**Files:**
- Create: `src/services/api/provider.rs`
- Create: `src/services/api/types.rs`
- Create: `src/services/api/mod.rs`
- Create: `src/services/mod.rs`

- [ ] **Step 1: Write provider types**

`src/services/api/types.rs`:
```rust
use serde::{Deserialize, Serialize};

use crate::tool::ToolDefinition;
use crate::types::message::{ApiMessage, StopReason};
use crate::types::content::ContentBlock;

/// Request to create a message (sent to LLM provider)
#[derive(Debug, Clone)]
pub struct CreateMessageRequest {
    pub model: String,
    pub messages: Vec<ApiMessage>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub max_tokens: u32,
    pub system: Option<Vec<SystemBlock>>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

/// System prompt block
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SystemBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

/// Provider capabilities
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub tool_use: bool,
    pub vision: bool,
    pub streaming: bool,
    pub extended_thinking: bool,
}

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_window: u64,
    pub max_output_tokens: u32,
    pub capabilities: Capabilities,
}

/// Streaming event from LLM
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Message creation started
    MessageStart { id: String },
    /// New content block started
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    /// Delta for current content block
    ContentBlockDelta { index: usize, delta: ContentDelta },
    /// Content block completed
    ContentBlockStop { index: usize },
    /// Message completed
    MessageStop { stop_reason: StopReason },
    /// Usage info
    Usage(UsageInfo),
}

/// Content delta in a streaming response
#[derive(Debug, Clone)]
pub enum ContentDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
}

/// Token usage information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_block_serde() {
        let block = SystemBlock::Text {
            text: "You are helpful".into(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "You are helpful");
    }

    #[test]
    fn test_usage_info_default() {
        let usage = UsageInfo::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert!(usage.cache_creation_input_tokens.is_none());
    }

    #[test]
    fn test_capabilities_default() {
        let caps = Capabilities::default();
        assert!(!caps.tool_use);
        assert!(!caps.vision);
    }
}
```

- [ ] **Step 2: Write provider trait**

`src/services/api/provider.rs`:
```rust
use async_trait::async_trait;
use futures::stream::BoxStream;

use super::types::*;

/// Stream of events from LLM provider
pub type MessageStream = BoxStream<'static, crate::types::Result<StreamEvent>>;

/// Core LLM provider trait — all providers implement this
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Create a streaming message
    async fn create_message(
        &self,
        request: CreateMessageRequest,
    ) -> crate::types::Result<MessageStream>;

    /// Get model information
    fn model_info(&self) -> &ModelInfo;

    /// Get provider capabilities
    fn capabilities(&self) -> &Capabilities;
}
```

- [ ] **Step 3: Create mod files**

`src/services/api/mod.rs`:
```rust
pub mod provider;
pub mod types;

pub use provider::{LlmProvider, MessageStream};
pub use types::*;
```

`src/services/mod.rs`:
```rust
pub mod api;
```

- [ ] **Step 4: Update lib.rs**

Add `pub mod services;` to `src/lib.rs`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p atom-code services::api`
Expected: 3 tests pass

- [ ] **Step 6: Commit**

```bash
git add src/services/ src/lib.rs
git commit -m "feat: define LlmProvider trait, MessageStream, streaming types"
```

---

### Task 10: Define app state

**Files:**
- Create: `src/state/app_state.rs`
- Create: `src/state/mod.rs`

- [ ] **Step 1: Write app state with tests**

```rust
// src/state/app_state.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::message::Message;
use crate::types::permissions::ToolPermissionContext;
use crate::utils::settings::SettingsJson;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    // Session
    pub settings: SettingsJson,
    pub session_id: String,
    #[serde(default)]
    pub verbose: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_loop_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_loop_model_for_session: Option<String>,

    // Messages (core — the entire query engine depends on this)
    #[serde(default)]
    pub messages: Vec<crate::types::message::Message>,

    // UI state
    #[serde(default = "default_expanded_view")]
    pub expanded_view: ExpandedView,
    #[serde(default)]
    pub is_brief_only: bool,
    #[serde(default)]
    pub selected_ip_agent_index: i32,
    #[serde(default)]
    pub coordinator_task_index: i32,
    #[serde(default)]
    pub view_selection_mode: ViewSelectionMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer_selection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_line_text: Option<String>,

    // Permissions
    #[serde(default)]
    pub tool_permission_context: ToolPermissionContext,

    // Feature flags
    #[serde(default)]
    pub kairos_enabled: bool,
    #[serde(default)]
    pub fast_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_enabled: Option<bool>,
    #[serde(default)]
    pub thinking_disabled: bool,
    #[serde(default)]
    pub prompt_suggestion_enabled: bool,

    // Remote/Bridge
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_session_url: Option<String>,
    #[serde(default = "default_connection_status")]
    pub remote_connection_status: ConnectionStatus,
    #[serde(default)]
    pub remote_background_task_count: u32,
    #[serde(default)]
    pub repl_bridge_enabled: bool,
    #[serde(default)]
    pub repl_bridge_explicit: bool,
    #[serde(default)]
    pub repl_bridge_outbound_only: bool,
    #[serde(default)]
    pub repl_bridge_connected: bool,
    #[serde(default)]
    pub repl_bridge_session_active: bool,
    #[serde(default)]
    pub repl_bridge_reconnecting: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repl_bridge_error: Option<String>,
    #[serde(default)]
    pub show_remote_callout: bool,

    // Tasks & Agents
    #[serde(default)]
    pub tasks: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub agent_name_registry: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground_task_id: Option<String>,

    // Speculation
    #[serde(default)]
    pub speculation_session_time_saved_ms: u64,

    // Auth
    #[serde(default)]
    pub auth_version: u32,

    // Cost tracking
    #[serde(default)]
    pub session_cost: f64,
    #[serde(default)]
    pub token_usage: TokenUsage,

    // Agents
    #[serde(default)]
    pub coordinator_mode: bool,
    #[serde(default)]
    pub swarm_agents: Vec<serde_json::Value>,

    // Advisor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisor_model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_message: Option<serde_json::Value>,

    #[serde(default)]
    pub active_overlays: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExpandedView {
    #[default]
    None,
    Tasks,
    Teammates,
}

fn default_expanded_view() -> ExpandedView {
    ExpandedView::None
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ViewSelectionMode {
    #[default]
    None,
    SelectingAgent,
    ViewingAgent,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Reconnecting,
    #[default]
    Disconnected,
}

fn default_connection_status() -> ConnectionStatus {
    ConnectionStatus::Disconnected
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            settings: SettingsJson::default(),
            session_id: uuid::Uuid::new_v4().to_string(),
            verbose: false,
            main_loop_model: None,
            main_loop_model_for_session: None,
            messages: Vec::new(),
            expanded_view: ExpandedView::None,
            is_brief_only: false,
            selected_ip_agent_index: 0,
            coordinator_task_index: 0,
            view_selection_mode: ViewSelectionMode::None,
            footer_selection: None,
            status_line_text: None,
            tool_permission_context: ToolPermissionContext::default(),
            kairos_enabled: false,
            fast_mode: false,
            thinking_enabled: None,
            thinking_disabled: false,
            prompt_suggestion_enabled: false,
            remote_session_url: None,
            remote_connection_status: ConnectionStatus::Disconnected,
            remote_background_task_count: 0,
            repl_bridge_enabled: false,
            repl_bridge_explicit: false,
            repl_bridge_outbound_only: false,
            repl_bridge_connected: false,
            repl_bridge_session_active: false,
            repl_bridge_reconnecting: false,
            repl_bridge_error: None,
            show_remote_callout: false,
            tasks: HashMap::new(),
            agent_name_registry: HashMap::new(),
            foreground_task_id: None,
            speculation_session_time_saved_ms: 0,
            auth_version: 0,
            session_cost: 0.0,
            token_usage: TokenUsage::default(),
            coordinator_mode: false,
            swarm_agents: Vec::new(),
            advisor_model: None,
            initial_message: None,
            active_overlays: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert_eq!(state.expanded_view, ExpandedView::None);
        assert!(!state.kairos_enabled);
        assert!(!state.fast_mode);
        assert!(state.tasks.is_empty());
    }

    #[test]
    fn test_app_state_serde_roundtrip() {
        let state = AppState {
            main_loop_model: Some("test-model".into()),
            fast_mode: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: AppState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.main_loop_model, state.main_loop_model);
        assert_eq!(parsed.fast_mode, state.fast_mode);
    }

    #[test]
    fn test_expanded_view_serde() {
        assert_eq!(
            serde_json::to_string(&ExpandedView::Tasks).unwrap(),
            "\"tasks\""
        );
        assert_eq!(
            serde_json::to_string(&ExpandedView::None).unwrap(),
            "\"none\""
        );
    }

    #[test]
    fn test_connection_status_serde() {
        assert_eq!(
            serde_json::to_string(&ConnectionStatus::Connected).unwrap(),
            "\"connected\""
        );
    }
}
```

- [ ] **Step 2: Create state/mod.rs**

```rust
// src/state/mod.rs
pub mod app_state;

pub use app_state::{AppState, ConnectionStatus, ExpandedView, ViewSelectionMode};
```

- [ ] **Step 3: Update lib.rs**

Ensure `pub mod state;` is present.

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code state`
Expected: 4 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/state/ src/lib.rs
git commit -m "feat: define AppState with all fields from original AppStateStore.ts"
```

---

### Task 11: Run full test suite and verify

- [ ] **Step 1: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass (error: 4, content: 5, message: 6, permissions: 4, settings: 6, tool: 4, api types: 3, state: 4 = ~36 tests total)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 3: Run rustfmt check**

Run: `cargo fmt --check`
Expected: No formatting issues

- [ ] **Step 4: Commit any fixes if needed**

```bash
git add -A
git commit -m "chore: fix clippy warnings and formatting"
```

---

### Task 12: Final verification build

- [ ] **Step 1: Clean build**

Run: `cargo build --workspace --release`
Expected: Compiles successfully in release mode

- [ ] **Step 2: Run binary**

Run: `./target/release/atom-code --version`
Expected: `atom-code 0.1.0`

- [ ] **Step 3: Final commit if needed**

```bash
git add -A
git commit -m "chore: phase 0+1 complete — project scaffolding and core types"
```
