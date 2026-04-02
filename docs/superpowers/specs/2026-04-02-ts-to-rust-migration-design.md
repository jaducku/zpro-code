# atom-code: TypeScript → Rust Full Migration Design

## Overview

Migrate the reverse-engineered Claude Code CLI (`claude-code-working`, ~520K LOC TypeScript) to a Rust-based CLI called `atom-code`. This is a 1:1 feature port with no feature loss. Architecture changes are deferred to a later phase.

**Key decisions:**
- Single binary distribution via `cargo install`
- Open LLM provider (OpenAI-compatible API — OpenRouter, vLLM, local models)
- Full MCP support (stdio/SSE/WebSocket)
- Ratatui-based TUI (replaces React + custom Ink renderer)
- Architecture mirrors original TS structure

## 1. Project Structure

Single crate with modules mirroring the original `src/` layout. Only `ink/` is renamed to `tui/` (framework change). All other directory names preserved as-is.

```
atom-code/
├── Cargo.toml
├── src/
│   ├── main.rs                        # Binary entry (original main.tsx, 4,683 LOC)
│   ├── setup.rs                       # Session init, bootstrap (original setup.ts, 477 LOC)
│   ├── tool.rs                        # Tool trait (original Tool.ts)
│   ├── tools.rs                       # Tool registration/routing (original tools.ts, 387 LOC)
│   ├── query.rs                       # Query loop (original query.ts, 1,732 LOC)
│   ├── query_engine.rs                # Conversation state (original QueryEngine.ts, 1,300 LOC)
│   ├── history.rs                     # History management (original history.ts, 464 LOC)
│   ├── cost_tracker.rs                # Token/cost tracking (original cost-tracker.ts, 323 LOC)
│   ├── commands_registry.rs           # Command registration (original commands.ts, 754 LOC)
│   ├── context.rs                     # Context initialization (original context.ts, 189 LOC)
│   ├── task.rs                        # Task type definition (original Task.ts, 125 LOC)
│   ├── interactive_helpers.rs         # Interactive helper functions (365 LOC)
│   ├── dialog_launchers.rs            # Dialog launcher functions (132 LOC)
│   │
│   ├── tools/                         # ~52 tool implementations
│   ├── query/                         # Query subsystem
│   │   ├── config.rs                  # Query configuration
│   │   ├── deps.rs                    # Query dependencies
│   │   ├── stop_hooks.rs              # Stop hook handlers
│   │   ├── token_budget.rs            # Token budget calculation
│   │   └── transitions.rs             # State transitions
│   ├── services/
│   │   ├── api/                       # LLM Provider trait + OpenAI-compatible client
│   │   ├── mcp/                       # MCP client (JSON-RPC, stdio/SSE/WS)
│   │   ├── compact/                   # Token budget, auto-compact
│   │   ├── session_memory/            # CLAUDE.md memory management
│   │   ├── session_transcript/        # Session persistence
│   │   ├── oauth/                     # OAuth token management
│   │   ├── plugins/                   # Plugin discovery, loading
│   │   ├── skill_search/              # Skill recommendation
│   │   ├── policy_limits/             # Enterprise policy enforcement
│   │   ├── analytics/                 # Event logging, feature flags
│   │   ├── extract_memories/          # Auto memory extraction
│   │   ├── lsp/                       # Language Server Protocol integration (2,464 LOC)
│   │   ├── tools/                     # Tool orchestration (StreamingToolExecutor, 3,129 LOC)
│   │   ├── team_memory_sync/          # Team memory sync with secret scanning (2,167 LOC)
│   │   ├── remote_managed_settings/   # Enterprise remote settings (950 LOC)
│   │   ├── settings_sync/             # Settings synchronization (648 LOC)
│   │   ├── auto_dream/               # Auto-dream/consolidation (552 LOC)
│   │   ├── prompt_suggestion/         # Prompt suggestions, speculative execution (1,515 LOC)
│   │   ├── magic_docs/               # Documentation feature (381 LOC)
│   │   ├── tips/                      # User tips/onboarding (772 LOC)
│   │   ├── context_collapse/          # Context collapsing for display (73 LOC)
│   │   ├── tool_use_summary/          # Tool use summary generation (112 LOC)
│   │   └── agent_summary/             # Agent summary generation (180 LOC)
│   ├── utils/                         # Utilities (auth, config, formatting, etc.)
│   ├── tui/                           # Ratatui-based TUI (replaces ink/, 19,836 LOC)
│   ├── components/                    # UI widgets (593 files, 82,101 LOC)
│   ├── screens/                       # Screen components (51 files, 6,076 LOC)
│   ├── hooks/                         # 83 hooks (UI state, integration, tool use)
│   ├── commands/                      # ~110 CLI commands (93 dirs + 15 files)
│   ├── context/                       # Context providers (10 files)
│   ├── state/                         # App state management (8 files)
│   ├── types/                         # Shared type definitions (32 files)
│   ├── constants/                     # Constants, prompts, XML tags (30 files)
│   ├── keybindings/                   # Keybinding system (16 files, 3,168 LOC)
│   ├── vim/                           # VIM state machine (5 files, 1,513 LOC)
│   │   ├── motions.rs
│   │   ├── operators.rs
│   │   ├── text_objects.rs
│   │   ├── transitions.rs
│   │   └── types.rs
│   ├── memdir/                        # Memory directory system (9 files, 1,743 LOC)
│   ├── skills/                        # Built-in skills (24 files)
│   ├── tasks/                         # Background task types (14 files)
│   ├── bridge/                        # Remote bridge
│   ├── buddy/                         # Companion feature
│   ├── assistant/                     # Assistant mode
│   ├── bootstrap/                     # Bootstrap utilities
│   ├── cli/                           # CLI helpers
│   ├── entrypoints/                   # Entry point helpers
│   ├── coordinator/                   # Multi-agent coordinator (2 files, 373 LOC)
│   ├── remote/                        # Remote session management (4 files, 1,132 LOC)
│   ├── server/                        # Headless/direct-connect server (11 files, 392 LOC)
│   ├── ssh/                           # SSH session management (2 files, 59 LOC)
│   ├── daemon/                        # Background daemon (2 files)
│   ├── migrations/                    # Settings migrations (12 files, 605 LOC)
│   ├── upstreamproxy/                 # Proxy relay (2 files, 740 LOC)
│   ├── schemas/                       # Hook schemas (2 files, 225 LOC)
│   ├── plugins/                       # Top-level plugin module (2 files, 182 LOC)
│   ├── output_styles/                 # Output style loading (1 file, 98 LOC)
│   └── file_index/                    # File indexing system (from native-ts/, 411 LOC)
│
├── packages/                          # Native modules (separate crates)
│   ├── color-diff/
│   ├── audio-capture/
│   ├── image-processor/
│   ├── modifiers/
│   ├── url-handler/
│   └── ant/                           # @ant/ packages
│       ├── claude-for-chrome-mcp/
│       ├── computer-use-input/
│       ├── computer-use-mcp/
│       └── computer-use-swift/
└── tests/                             # Integration tests
```

**Dependency direction (unidirectional):**
```
main.rs → tui/ → query engine → tools → services/api (LLM)
                      ↓              ↓
                  services/mcp   utils/permissions
                      ↓
                  services/tools (orchestration)
```

## 2. Core Types & Data Structures

Direct mapping from TS types to Rust, using `serde` for serialization.

### Message types (original: `src/types/message.ts`)

7 core message types with system message subtypes (ToolUseSummary is a display-only alias, not a distinct message type):

```rust
enum Message {
    User(UserMessage),
    Assistant(AssistantMessage),
    System(SystemMessage),
    Attachment(AttachmentMessage),
    Progress(ProgressMessage),
    GroupedToolUse(GroupedToolUseMessage),
    CollapsedReadSearch(CollapsedReadSearchMessage),
    ToolUseSummary(ToolUseSummaryMessage),
}

/// System message subtypes (16 variants)
enum SystemMessage {
    LocalCommand(SystemLocalCommandMessage),
    CompactBoundary(SystemCompactBoundaryMessage),
    MicrocompactBoundary(SystemMicrocompactBoundaryMessage),
    ApiError(SystemAPIErrorMessage),
    FileSnapshot(SystemFileSnapshotMessage),
    AgentsKilled(SystemAgentsKilledMessage),
    ApiMetrics(SystemApiMetricsMessage),
    AwaySummary(SystemAwaySummaryMessage),
    BridgeStatus(SystemBridgeStatusMessage),
    Informational(SystemInformationalMessage),
    MemorySaved(SystemMemorySavedMessage),
    PermissionRetry(SystemPermissionRetryMessage),
    ScheduledTaskFire(SystemScheduledTaskFireMessage),
    StopHookSummary(SystemStopHookSummaryMessage),
    TurnDuration(SystemTurnDurationMessage),
    Thinking(SystemThinkingMessage),
}

struct UserMessage {
    uuid: Uuid,
    content: Vec<ContentBlock>,
}

struct AssistantMessage {
    uuid: Uuid,
    source_agent_id: Option<String>,
    content: Vec<ContentBlock>,
}

enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String },
    Image { source: ImageSource },
}
```

### App state (original: `src/state/AppStateStore.ts`)

```rust
pub struct AppState {
    // Session
    pub settings: SettingsJson,
    pub messages: Vec<Message>,
    pub main_loop_model: String,
    pub session_id: String,

    // Permissions
    pub tool_permission_context: ToolPermissionContext,

    // UI
    pub expanded_view: ExpandedView,
    pub selected_ip_agent_index: usize,

    // Features
    pub kairos_enabled: bool,
    pub repl_bridge_enabled: bool,

    // Background
    pub tasks: Vec<TaskState>,
    pub mcp_clients: Vec<McpServerConnection>,

    // History & Attribution
    pub file_history: Vec<FileHistoryEntry>,
    pub attribution_snapshots: Vec<AttributionSnapshot>,

    // Agent Swarms
    pub swarm_agents: Vec<SwarmAgent>,
    pub coordinator_mode: bool,

    // Cost tracking
    pub token_usage: TokenUsage,
    pub session_cost: f64,

    // ... remaining fields mapped 1:1 from original AppStateStore.ts
    // Full enumeration deferred to Phase 1 implementation where
    // each field is verified against the original source.
}
```

Note: `AppState` will be wrapped in `Arc<RwLock<AppState>>` for shared mutable access across async tasks (see Section 14: Rust-Specific Challenges).

### Settings (original: `src/utils/settings/types.ts`)

```rust
#[derive(Deserialize, Serialize)]
struct SettingsJson {
    permissions: Option<PermissionSettings>,
    mcp_servers: Option<HashMap<String, McpServerConfig>>,
    hooks: Option<HooksConfig>,
    // ...
}
```

## 3. External Dependency Mapping

| TS Dependency | Rust Crate | Purpose |
|---|---|---|
| `@anthropic-ai/sdk` | `reqwest` + custom impl | OpenAI-compatible API client |
| `@modelcontextprotocol/sdk` | Custom impl (JSON-RPC) | MCP protocol |
| `commander.js` | `clap` | CLI parsing |
| React + react-reconciler | `ratatui` + `crossterm` | TUI |
| `zod` | `serde` + `serde_json` | Schema validation |
| `ws` | `tokio-tungstenite` | WebSocket |
| `axios` / `undici` | `reqwest` | HTTP |
| `chokidar` | `notify` | File watching |
| `chalk` | `crossterm::style` | Terminal colors |
| `marked` / `turndown` | `pulldown-cmark` | Markdown |
| `fuse.js` | `fuzzy-matcher` | Fuzzy search |
| `lodash-es` | std library | Utilities |
| `semver` | `semver` | Version management |
| `yaml` | `serde_yaml` | YAML parsing |
| `jsonc-parser` | `json-comments` | JSONC parsing |
| `sharp` | `image` | Image processing |
| `diff` | `similar` | Text diff |
| `lru-cache` | `lru` | LRU cache |
| `ignore` | `ignore` | Gitignore matching |
| `picomatch` | `globset` | Glob matching |
| `shell-quote` | `shell-words` | Shell quoting |
| `strip-ansi` | `strip-ansi-escapes` | ANSI removal |
| `highlight.js` | `syntect` | Syntax highlighting |
| `@opentelemetry/*` | `opentelemetry` | Observability |
| `google-auth-library` | `gcp-auth` | GCP auth |
| `@aws-sdk/*` | `aws-sdk-rust` | AWS auth |
| `plist` | `plist` | macOS plist |
| `qrcode` | `qrcode` | QR codes |
| `uuid` (implicit) | `uuid` | UUID generation |

## 4. LLM Provider Abstraction

Open provider model instead of Anthropic-only. OpenAI-compatible API as the common interface (covers OpenRouter, vLLM, local models).

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn create_message(&self, request: CreateMessageRequest) -> Result<MessageStream>;
    fn model_info(&self) -> &ModelInfo;
    fn capabilities(&self) -> &Capabilities;
}

pub struct CreateMessageRequest {
    pub model: String,
    pub messages: Vec<ApiMessage>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub max_tokens: u32,
    pub system: Option<Vec<SystemBlock>>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

pub struct Capabilities {
    pub tool_use: bool,
    pub vision: bool,
    pub streaming: bool,
    pub extended_thinking: bool,
}

// Single implementation covers OpenRouter, vLLM, local models
pub struct OpenAiCompatibleProvider {
    base_url: String,
    api_key: Option<String>,
    model: String,
    client: reqwest::Client,
}
```

**Internal format unification:**

| Difference | Anthropic | OpenAI Compatible | Handling |
|---|---|---|---|
| Tool calls | `tool_use` content block | `tool_calls` field | Provider converts to internal format |
| Tool results | `tool_result` in user msg | `tool` role message | Provider converts |
| Stop reason | `stop_reason: "end_turn"` | `finish_reason: "stop"` | Provider unifies to `StopReason` enum |
| Streaming | `content_block_delta` | `chat.completion.chunk` | Provider unifies |
| System prompt | `system` array | `system` role message | Provider converts |

## 5. Tool System

Direct port of `src/Tool.ts` + `src/tools/`. ~52 built-in tools.

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    async fn execute(&self, ctx: ToolUseContext) -> Result<ToolResult>;
    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult { ValidationResult::Ok }
    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta;
}

pub struct ToolUseContext {
    pub input: serde_json::Value,
    pub app_state: Arc<RwLock<AppState>>,
    pub abort_signal: CancellationToken,
    pub file_state_cache: Arc<FileStateCache>,
}

pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}
```

Tool registration (`tools.rs`) mirrors original `getTools()` with conditional enablement based on feature flags and app state.

Tool orchestration layer (`services/tools/`) handles `StreamingToolExecutor` — concurrent tool execution with streaming progress updates.

## 6. MCP Client

Full port of `src/services/mcp/`. Three transport types: stdio, SSE, WebSocket.

```rust
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&self, message: JsonRpcMessage) -> Result<()>;
    async fn receive(&mut self) -> Result<JsonRpcMessage>;
    async fn close(&self) -> Result<()>;
}

pub struct McpClient {
    transport: Box<dyn McpTransport>,
    server_info: ServerInfo,
    capabilities: ServerCapabilities,
}

pub struct McpConnectionManager {
    clients: HashMap<String, McpClient>,
    configs: HashMap<String, McpServerConfig>,
}
```

MCP tools are wrapped as `McpToolWrapper` implementing the `Tool` trait, merged with built-in tools in the query loop.

## 7. Query Engine

Direct port of `src/query.ts` (1,732 LOC) + `src/QueryEngine.ts` (1,300 LOC) + `src/query/` subsystem (655 LOC).

Core loop:
1. Calculate token budget (`query/token_budget.rs`)
2. Auto-compact if budget exceeded
3. Build API request (messages + tools + system prompt, configured via `query/config.rs`)
4. Stream response from LLM provider
5. Extract tool calls from response
6. If tool calls: execute tools via `services/tools/StreamingToolExecutor` (with permission checks + hooks), loop back to step 3
7. If no tool calls: evaluate stop hooks (`query/stop_hooks.rs`), apply transitions (`query/transitions.rs`), return to TUI

```rust
pub async fn query(
    user_message: UserMessage,
    context: QueryContext,
    provider: &dyn LlmProvider,
) -> Result<QueryResult>;
```

Dependencies for query execution are managed through `query/deps.rs` (provider, MCP clients, file state cache, tool permission context, abort controller).

## 8. Permission System

Direct port of `src/utils/permissions/` (~9,434 LOC).

```rust
pub enum PermissionMode { Default, Auto, Bypass }

pub struct ToolPermissionContext {
    pub mode: PermissionMode,
    pub always_allow_rules: Vec<PermissionRule>,
    pub always_deny_rules: Vec<PermissionRule>,
}

pub enum PermissionResult {
    Allowed,
    Denied,
    Ask(PermissionRequest),
}
```

Flow: Bypass check → deny rules → allow rules → YOLO classifier (auto mode) → ask user.

## 9. TUI

Ratatui-based terminal UI replacing React + custom Ink renderer. Same functionality and UX as original.

### Architecture

The original uses React reconciler + Yoga flexbox layout + cell-diff rendering (104 files, ~20K LOC). Ratatui uses constraint-based layout with immediate-mode rendering. The user-facing behavior must be identical; the rendering internals differ.

**Event loop** replaces React's render cycle:

```rust
pub struct App {
    pub state: Arc<RwLock<AppState>>,
    pub query_engine: QueryEngine,
    pub input: InputState,
    pub message_list: MessageListState,
    pub permission_dialog: Option<PermissionRequest>,
    // ... per-widget state structs replacing React component state
}

// tokio::select! event loop replaces React useEffect + event handlers
loop {
    terminal.draw(|frame| render(frame, &app))?;
    tokio::select! {
        event = event_stream.next() => { /* keyboard/mouse */ }
        chunk = query_engine.next_chunk() => { /* LLM streaming */ }
        progress = tool_progress.next() => { /* tool execution */ }
    }
}
```

### Hooks migration strategy

The original has 83 React hooks in `src/hooks/`. These are NOT tool-use hooks — they are UI state and lifecycle hooks (`useTextInput`, `useVimInput`, `useTerminalSize`, `useMergedTools`, etc.).

In Rust, each hook becomes one of:
- **State struct field** in `App` or per-widget state (replaces `useState`)
- **Channel receiver** in the event loop (replaces `useEffect` with subscriptions)
- **Function** called during render or event handling (replaces computed hooks)
- **Background task** spawned via `tokio::spawn` (replaces effect-based side effects)

The hooks are ported alongside the component/screen they serve, not as a separate phase.

### Component mapping

Original React components (593 files) are mapped to Ratatui widget structs with `Widget` trait implementations. Layout uses Ratatui's `Layout::vertical/horizontal` with `Constraint` instead of Yoga flexbox. Complex layouts may require nested layout splits.

## 10. CLI

`clap` replaces `commander.js`. All subcommands and slash commands ported.

**Subcommands:** `mcp`, `auth`, `plugin`, `doctor`, `update`, `agents`, etc.
**Slash commands:** ~110 commands (93 directories + 15 files in `src/commands/`), including:
- Core: `help`, `exit`, `clear`, `status`, `session`
- Git/Dev: `branch`, `diff`, `review`, `pr_comments`
- Config: `config`, `keybindings`, `settings`, `permissions`, `theme`
- AI: `plan`, `assistant`, `tasks`, `summary`, `skills`, `hooks`
- Integration: `ide`, `chrome`, `mobile`, `desktop`, `mcp`
- System: `doctor`, `login`, `logout`, `upgrade`, `install-*`
- Analytics: `cost`, `usage`, `stats`, `effort`
- Advanced: `debug-tool-call`, `heapdump`, `workflows`, `agents`

Command registration is handled in `commands_registry.rs` (original `commands.ts`, 830 LOC), with conditional enablement based on feature flags.

## 11. Services Layer

1:1 port of all 24 `src/services/` modules:

| Service | LOC | Purpose |
|---|---|---|
| `api/` | ~10,647 | LLM Provider (see Section 4) |
| `mcp/` | ~12,363 | MCP protocol (see Section 6) |
| `tools/` | ~3,129 | Tool orchestration, StreamingToolExecutor |
| `compact/` | — | Token budget, auto/micro/API compression |
| `session_memory/` | — | CLAUDE.md memory management |
| `session_transcript/` | — | Session persistence to disk |
| `oauth/` | — | OAuth token management |
| `plugins/` | — | Plugin discovery, loading, versioning |
| `skill_search/` | — | Skill recommendation classifier |
| `policy_limits/` | — | Enterprise policy enforcement |
| `analytics/` | — | Event logging, feature flags |
| `extract_memories/` | — | Automatic memory extraction |
| `lsp/` | ~2,464 | Language Server Protocol integration |
| `team_memory_sync/` | ~2,167 | Team memory sync with secret scanning |
| `remote_managed_settings/` | ~950 | Enterprise remote settings fetch/cache |
| `settings_sync/` | ~648 | Settings synchronization |
| `auto_dream/` | ~552 | Auto-dream/consolidation |
| `prompt_suggestion/` | ~1,515 | Prompt suggestions, speculative execution |
| `magic_docs/` | ~381 | Documentation feature |
| `tips/` | ~772 | User tips/onboarding hints |
| `context_collapse/` | ~73 | Context collapsing for display |
| `tool_use_summary/` | ~112 | Tool use summary generation |
| `agent_summary/` | ~180 | Agent summary generation |
| `notifier` | — | Terminal notifications |
| `prevent_sleep` | ~165 | Prevent system sleep |
| `voice` | ~525 | Voice mode (feature flag off) |
| `voice_stream_stt` | ~544 | Voice streaming STT |
| `voice_keyterms` | ~106 | Voice key terms |
| `claude_ai_limits` | ~515 | Rate limit handling |
| `rate_limit_messages` | ~344 | Rate limit UI messages |
| `mock_rate_limits` | ~882 | Rate limit mocking/testing |
| `token_estimation` | ~495 | Token estimation |
| `vcr` | ~406 | Recording/replay |
| `diagnostic_tracking` | ~397 | Diagnostics tracking |
| `internal_logging` | ~90 | Internal logging |
| `away_summary` | ~74 | Away summary generation |
| `mcp_server_approval` | ~40 | MCP server approval UI |

## 12. Build & Dev Environment

- **Cargo workspace** — single main crate + `packages/` as workspace members
- **mold linker** — fast link times for large project
- **cargo-nextest** — parallel test execution
- **clippy + rustfmt** — lint and format (replaces biome)
- **cargo-deny** — dependency security audit

## 13. Migration Phases (Bottom-up + Validation Milestones)

```
Phase 0:  Project scaffolding (Cargo workspace, full directory structure, Cargo.toml deps)
Phase 1:  Core types + utilities (all message types, AppState, settings, auth, errors, migrations)
Phase 2:  LLM Provider abstraction (OpenAI-compatible trait + SSE streaming client)
Phase 3:  Tool system (Tool trait + services/tools orchestration + ~52 tool implementations)
Phase 4:  Query engine (message loop, query/ subsystem, token budget, compression, hooks)
Phase 5:  MCP client (stdio/SSE/WebSocket transports, JSON-RPC, connection manager)
Phase 6:  Permission system (rules, matching, YOLO classifier, dialog types)
Phase 7:  TUI (Ratatui app, components, screens, hooks migration, VIM mode)
Phase 8:  CLI commands (~110 commands + command registry + clap subcommands)
Phase 9:  Services (all 24 services: session, plugins, skills, OAuth, LSP, analytics, etc.)
Phase 10: Auxiliary modules (memdir, coordinator, remote, server, ssh, daemon, bridge, buddy,
          assistant, upstreamproxy, schemas, plugins top-level, output_styles, file_index)
Phase 11: Packages (color-diff, audio-capture, image-processor, modifiers, url-handler,
          @ant/computer-use-*, @ant/claude-for-chrome-mcp)
Phase 12: Integration testing + polish
```

Each phase is independently testable before proceeding to the next.

## 14. Rust-Specific Challenges

### Mutable state sharing

The original freely shares mutable state via React state + closures (`getAppState()`, `setAppState()`). In Rust, `AppState` is wrapped in `Arc<RwLock<AppState>>` and passed to async tasks. State updates go through a channel-based message passing pattern to avoid lock contention:

```rust
// Event-driven state updates (avoids holding RwLock across await points)
enum StateUpdate {
    AddMessage(Message),
    UpdatePermissions(ToolPermissionContext),
    SetModel(String),
    // ...
}

// State update channel
let (state_tx, state_rx) = tokio::sync::mpsc::unbounded_channel::<StateUpdate>();
```

### Streaming + tool execution concurrency

The original's `StreamingToolExecutor` runs tools concurrently while streaming LLM output. In Rust:
- Tool futures are boxed: `Pin<Box<dyn Future<Output = Result<ToolResult>> + Send>>`
- `tokio::JoinSet` manages concurrent tool execution
- `CancellationToken` (from `tokio-util`) replaces `AbortController`
- All tool implementations must be `Send + Sync`

### Dynamic tool dispatch

`Box<dyn Tool>` is the primary dispatch mechanism. The `#[async_trait]` macro handles boxing of async return types. Tool registry stores `Vec<Box<dyn Tool>>`. No associated types or `Self` bounds needed in the trait — all methods use `&self`.

### Closure-heavy patterns

TS `ToolUseContext` contains callbacks (`setToolJSX`, `appendSystemMessage`, `sendOSNotification`). In Rust, these become:
- Channel senders (`mpsc::Sender<UIEvent>`) for UI updates
- `Arc<dyn Fn(...) + Send + Sync>` for stateless callbacks
- Trait objects for complex callback groups
