# Phase 10: Auxiliary Modules Design

## Overview

Phase 10 implements auxiliary modules that extend the core atom-code CLI with memory management, multi-agent orchestration, fuzzy file search, plugin system, output style customization, and a full-featured hook system. This spec targets **Full Parity** with the original TypeScript Claude Code CLI.

### Scope

| Priority | Module | Description |
|----------|--------|-------------|
| 1st | hooks | Enhance from 2 to 27 event types, 5 command types, matcher system |
| 1st | memdir | Memory directory system (MEMORY.md, relevance search, auto-extraction) |
| 1st | file_index | Fuzzy file search indexer with nucleo-style scoring |
| 2nd | coordinator | Multi-agent coordinator with worker lifecycle management |
| 2nd | plugins | Built-in plugin registry, loading, hot-reload |
| 2nd | output_styles | Markdown-based output style loader and system prompt injection |

### Excluded

- **Remote infrastructure**: remote, server, ssh, daemon, bridge, upstreamproxy
- **Assistant mode (KAIROS)**: depends on remote infrastructure; original is also stub
- **Buddy**: companion sprite system (deferred)

### Migration from MVP Hooks

The existing `src/services/hooks/mod.rs` (MVP with 2 events, command-only) is **replaced entirely**. Migration notes:

- `HookType` enum (2 variants) → replaced by `HookEvent` string constants (27 types)
- `HookConfig { hook_type, tool_name, command }` → replaced by `HookMatcher { matcher, hooks: Vec<HookCommand> }` nested inside settings
- `HookResult { Allow, Deny, Modified, Error }` → replaced by `SyncHookOutput` JSON schema with richer response fields
- `execute_hooks()` (synchronous, sequential) → replaced by async parallel execution with `tokio::JoinSet`
- `.agents/hooks.json` flat array format → **deprecated but supported** for backward compatibility during transition. At load time, flat entries are converted to the new matcher format. New settings.json `"hooks"` key takes precedence.
- `load_hooks()` → replaced by settings source hierarchy (user/project/local/policy/plugin/session/builtin) with deduplication
- Existing tests updated to use new API

### Environment Variable Naming Convention

This project uses two env var prefixes by convention:
- `ATOM_` — runtime context variables injected into hooks (e.g., `ATOM_SESSION_ID`, `ATOM_TOOL_NAME`)
- `ATOM_CODE_` — configuration/feature flags (e.g., `ATOM_CODE_DISABLE_AUTO_MEMORY`, `ATOM_CODE_SIMPLE`)

This mirrors the original TypeScript's `CLAUDE_` vs `CLAUDE_CODE_` convention.

---

## 1. Hooks Enhancement

### 1.1 Hook Events (27 types)

```rust
pub const HOOK_EVENTS: &[&str] = &[
    "PreToolUse", "PostToolUse", "PostToolUseFailure",
    "Notification", "UserPromptSubmit",
    "SessionStart", "SessionEnd",
    "Stop", "StopFailure",
    "SubagentStart", "SubagentStop",
    "PreCompact", "PostCompact",
    "PermissionRequest", "PermissionDenied",
    "Setup", "TeammateIdle",
    "TaskCreated", "TaskCompleted",
    "Elicitation", "ElicitationResult",
    "ConfigChange", "InstructionsLoaded",
    "WorktreeCreate", "WorktreeRemove",
    "CwdChanged", "FileChanged",
];
```

### 1.2 Hook Command Types (5 types)

```rust
pub enum HookCommand {
    Command {
        command: String,
        shell: Option<String>,        // "bash" | "zsh" | "powershell"
        timeout: Option<u64>,
        status_message: Option<String>,
        once: Option<bool>,
        r#async: Option<bool>,
        async_rewake: Option<bool>,
        r#if: Option<String>,         // "Bash(git *)" format
    },
    Prompt {
        prompt: String,
        model: Option<String>,
        timeout: Option<u64>,
        status_message: Option<String>,
        once: Option<bool>,
        r#if: Option<String>,
    },
    Agent {
        prompt: String,
        model: Option<String>,
        timeout: Option<u64>,
        status_message: Option<String>,
        once: Option<bool>,
        r#if: Option<String>,
    },
    Http {
        url: String,
        headers: Option<HashMap<String, String>>,
        allowed_env_vars: Option<Vec<String>>,
        timeout: Option<u64>,
        status_message: Option<String>,
        once: Option<bool>,
        r#if: Option<String>,
    },
    // Function is INTERNAL-ONLY — never serialized to/from JSON.
    // Registered programmatically at runtime (session hooks, SDK callbacks).
    // When deserializing HookCommand from settings JSON, use #[serde(tag = "type")]
    // on the four JSON-facing variants; Function lives in a separate wrapper or
    // is excluded via #[serde(skip)].
    Function {
        id: Option<String>,
        timeout: Option<u64>,
        callback: Arc<dyn Fn(Vec<Message>, Option<CancellationToken>)
            -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>,
        error_message: String,
        status_message: Option<String>,
    },
}
```

Note: `CancellationToken` is `tokio_util::sync::CancellationToken`, already used in the codebase (`src/tool.rs` `ToolUseContext.abort`). The four JSON-serializable variants use `#[serde(tag = "type", rename_all = "lowercase")]`; `Function` is `#[serde(skip)]` and only exists in-memory.

### 1.3 Matcher System

Pattern matching logic (matches original `matchesPattern()`):

```rust
fn matches_pattern(matcher: &Option<String>, query: &str) -> bool {
    match matcher {
        None => true,
        Some(m) if m == "*" => true,
        Some(m) => {
            if m.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '|') {
                m.split('|').any(|part| part == query)
            } else {
                Regex::new(m).map(|re| re.is_match(query)).unwrap_or(false)
            }
        }
    }
}
```

### 1.4 `if` Condition Parsing

Permission rule syntax: `"Bash(git *)"` -> tool_name=`"Bash"`, pattern=`"git *"`.

```rust
fn parse_if_condition(condition: &str) -> Option<(String, Option<String>)> {
    if let Some(paren_start) = condition.find('(') {
        let tool_name = &condition[..paren_start];
        let pattern = &condition[paren_start+1..condition.len()-1];
        Some((tool_name.to_string(), Some(pattern.to_string())))
    } else {
        Some((condition.to_string(), None))
    }
}
```

Tool-specific `prepare_permission_matcher()` validates input against pattern.

### 1.5 Settings Structure

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "validate-bash.sh", "timeout": 10 }
        ]
      },
      {
        "matcher": "Write|Edit",
        "hooks": [
          { "type": "command", "command": "lint-check.sh", "if": "Write(*.rs)" }
        ]
      }
    ],
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "http",
            "url": "https://api.example.com/notify",
            "headers": { "Authorization": "Bearer $API_TOKEN" },
            "allowedEnvVars": ["API_TOKEN"]
          }
        ]
      }
    ]
  }
}
```

### 1.6 Settings Source Hierarchy (highest to lowest)

1. **userSettings** — `~/.atom/settings.json`
2. **projectSettings** — `.agents/settings.json`
3. **localSettings** — `.agents/settings.local.json`
4. **policySettings** — managed-settings.json (enterprise)
5. **pluginHook** — `~/.atom/plugins/*/hooks/`
6. **sessionHook** — runtime registered (in-memory)
7. **builtinHook** — code-embedded

Settings are pre-merged into a snapshot via `get_hooks_config_from_snapshot()` before reaching hook execution.

Note: The existing `SettingsSource` enum (`src/utils/settings/types.rs`) needs extension to include `Plugin`, `Session`, and `Builtin` variants for hook source tracking. These are separate from the settings file hierarchy — they represent how hooks are *registered*, not where settings files live.

### 1.7 Deduplication

Key: `{plugin_root_or_skill_root}\0{shell}\0{command}\0{if_condition}`

```rust
fn hook_dedup_key(hook: &MatchedHook) -> String {
    let root = hook.plugin_root.as_ref()
        .or(hook.skill_root.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("");
    let shell = hook.shell.as_deref().unwrap_or(DEFAULT_HOOK_SHELL);
    format!("{}\0{}\0{}\0{}", root, shell, hook.command, hook.if_condition.as_deref().unwrap_or(""))
}
```

Different shells or `if` conditions produce distinct hooks.

### 1.8 Execution Flow

```
1. Check workspace trust (required in interactive mode)
2. Check disableAllHooks / allowManagedHooksOnly
3. getMatchingHooks(): matcher + if condition filtering
4. Execute matching hooks IN PARALLEL (tokio JoinSet / futures::join_all)
   - Unbounded concurrency (all matching hooks run simultaneously)
   - Each hook has its own timeout (per-hook `timeout` field)
   - If a hook exceeds its timeout: treated as non-blocking error
5. Aggregate results (wait for ALL hooks to complete, no short-circuit):
   - Any blocking → whole result is blocking
   - Accumulate additionalContext from all hooks
   - Merge watchPaths (SessionStart/CwdChanged/FileChanged)
6. Return aggregated result
```

### 1.9 Exit Code Semantics

| Code | Meaning | Behavior |
|------|---------|----------|
| 0 | Success | Event-specific (continue, show output, etc.) |
| 2 | Blocking error | stderr sent to model |
| Other | Non-blocking error | stderr shown to user only |

### 1.10 Hook Response JSON Schema

```rust
pub struct SyncHookOutput {
    pub r#continue: Option<bool>,
    pub decision: Option<String>,            // "approve" | "block"
    pub reason: Option<String>,
    pub suppress_output: Option<bool>,
    pub stop_reason: Option<String>,
    pub system_message: Option<String>,
    pub hook_specific_output: Option<HookSpecificOutput>,
}

pub enum HookSpecificOutput {
    PreToolUse {
        updated_input: Option<Value>,
        permission_decision: Option<String>,
        permission_decision_reason: Option<String>,
        additional_context: Option<String>,
    },
    PostToolUse {
        additional_context: Option<String>,
        updated_mcp_tool_output: Option<Value>,
    },
    PostToolUseFailure {
        additional_context: Option<String>,
    },
    SessionStart {
        initial_user_message: Option<String>,
        watch_paths: Option<Vec<String>>,
        additional_context: Option<String>,
    },
    UserPromptSubmit {
        additional_context: Option<String>,
    },
    Setup {
        additional_context: Option<String>,
    },
    SubagentStart {
        additional_context: Option<String>,
    },
    PermissionRequest {
        decision: Option<PermissionHookDecision>,
        // PermissionHookDecision:
        // { behavior: "allow" | "deny", message: Option<String>,
        //   interrupt: Option<bool>, updated_input: Option<Value>,
        //   updated_permissions: Option<Vec<PermissionUpdate>> }
    },
    PermissionDenied {
        retry: Option<bool>,
    },
    Notification {
        additional_context: Option<String>,
    },
    Elicitation {
        action: String,
        content: Option<Value>,
    },
    ElicitationResult {
        action: String,
        content: Option<Value>,
    },
    CwdChanged {
        watch_paths: Option<Vec<String>>,
    },
    FileChanged {
        watch_paths: Option<Vec<String>>,
    },
    WorktreeCreate {
        worktree_path: Option<String>,
    },
}
```

### 1.11 Command Hook Execution

Shell command spawned via configured shell (default `sh`):

- Environment variables set:
  - **Base (all hooks):** `ATOM_SESSION_ID`, `ATOM_TRANSCRIPT_PATH`, `ATOM_CWD`, `ATOM_PERMISSION_MODE`, `ATOM_AGENT_ID`, `ATOM_AGENT_TYPE`
  - **Tool hooks:** `ATOM_TOOL_NAME`, `ATOM_TOOL_INPUT` (JSON), `ATOM_TOOL_OUTPUT` (PostToolUse only)
  - **Event-specific:** `ATOM_SOURCE` (SessionStart), `ATOM_TRIGGER` (Setup), `ATOM_NOTIFICATION_TYPE` (Notification)
- stdin: JSON input
- stdout/stderr captured
- Timeout enforced (per-hook `timeout` field)

### 1.12 Prompt Hook Execution

1. Replace `$ARGUMENTS` placeholder with JSON input
2. Call LLM via small/fast model (provider's cheapest model; e.g., Haiku for Anthropic, gpt-4o-mini for OpenAI). Falls back to default model if no small model configured. On LLM failure, treated as non-blocking error.
3. Expected response: `{"ok": true}` or `{"ok": false, "reason": "..."}`
4. `ok: false` → blocking

### 1.13 Agent Hook Execution

1. Create multi-turn agent session (max 50 turns)
2. All tools available EXCEPT Agent, VerifyPlanExecution
3. Add `SyntheticOutputTool` for structured output
4. Agent must call SyntheticOutputTool with `{ok: boolean, reason?: string}`
5. `ok: false` → blocking

### 1.14 HTTP Hook Execution

1. Enforce URL allowlist (`allowed_http_hook_urls` in settings)
2. Interpolate env vars in headers (only from `allowed_env_vars`)
3. Sanitize headers (strip CR/LF/NUL — injection prevention)
4. POST to URL with timeout (default 10 minutes)
5. Parse response as JSON (empty body = `{}`)
6. Status < 200 or >= 300 → blocking

### 1.15 Async Hook Registry

```rust
pub struct AsyncHookRegistry {
    pending: HashMap<String, PendingAsyncHook>,
}

impl AsyncHookRegistry {
    // async: true → background spawn, return immediately
    fn register_pending(&mut self, hook: PendingAsyncHook);

    // Called each turn — check for completed hooks
    fn check_for_responses(&mut self) -> Vec<AsyncHookResponse> {
        // Split stdout by newlines, iterate lines
        // For each line: if trimmed starts with '{', try JSON parse
        //   - If parsed JSON contains "async" key → skip (this is the initial async ack)
        //   - Otherwise → use as sync response, break (first valid JSON wins)
        // If no '{' line found → response is empty {}
        // If JSON spans multiple lines → not supported (each response must be single-line JSON)
    }

    // async_rewake: true + exit code 2 → enqueue task notification, wake model
    fn handle_rewake(&self, hook_id: &str, stderr: &str);

    // Shutdown: kill remaining processes
    fn finalize_pending(&mut self);
}
```

### 1.16 Special Cases

- **SessionStart/Setup**: Always fire regardless of `includeHookEvents` setting
- **SessionEnd**: Timeout 1.5s default, configurable via `ATOM_CODE_SESSIONEND_HOOKS_TIMEOUT_MS`
- **HTTP hooks**: Disabled for SessionStart/Setup (sandbox ask callback deadlock in headless mode)
- **`allowManagedHooksOnly`**: Only managed (policy) hooks execute; user/project/plugin blocked

---

## 2. Memdir (Memory Directory System)

### 2.1 Directory Structure

```
~/.atom/projects/{sanitized-git-root}/memory/
├── MEMORY.md              # Index file (200 line / 25KB cap)
├── {topic}.md             # Individual memory files with YAML frontmatter
└── team/                  # Team memory (when TEAMMEM enabled)
    ├── MEMORY.md
    └── {topic}.md
```

### 2.2 Path Resolution

Priority chain:

1. `ATOM_COWORK_MEMORY_PATH_OVERRIDE` env var (full-path override)
2. `autoMemoryDirectory` in settings.json (trusted sources only: policy/user, NOT project)
3. `~/.atom/projects/{sanitized_git_root}/memory/`

Git worktree handling: `find_canonical_git_root()` ensures all worktrees share one memory directory.

Path is memoized per project root.

### 2.3 Memory File Format

```markdown
---
name: {{memory name}}
description: {{one-line description — used for relevance matching}}
type: {{user | feedback | project | reference}}
---

{{memory content}}
```

File naming: semantic, topic-based (e.g., `user_role.md`, `feedback_testing.md`).

### 2.4 MEMORY.md Index

- No frontmatter (unlike individual files)
- One line per pointer, ~150 chars: `- [Title](file.md) — one-line hook`
- Hard limits: **200 lines / 25,000 bytes**

Truncation algorithm:

```rust
struct EntrypointTruncation {
    content: String,
    line_count: usize,
    byte_count: usize,
    was_line_truncated: bool,
    was_byte_truncated: bool,
}

fn truncate_entrypoint_content(raw: &str) -> EntrypointTruncation {
    // 1. Trim leading/trailing whitespace
    // 2. If line_count > 200 → slice to first 200 lines, set was_line_truncated
    // 3. If byte_count > 25_000 → cut at last newline before cap, set was_byte_truncated
    // 4. Append warning naming which caps fired and why
}
```

### 2.5 Memory Types (closed 4-type system)

```rust
pub const MEMORY_TYPES: &[&str] = &["user", "feedback", "project", "reference"];
```

- **user**: Role, preferences, knowledge, goals. Always private.
- **feedback**: Work approach guidance (corrections + confirmations). Structure: rule → `Why:` → `How to apply:`.
- **project**: Ongoing work, decisions, deadlines. Convert relative dates to absolute. Bias toward team.
- **reference**: Pointers to external systems. Usually team.

Explicit exclusions: code patterns, git history, debugging solutions, AGENTS.md duplicates, ephemeral task state.

Unrecognized types in frontmatter: files with unknown `type` values are loaded but logged as warnings. They appear in the memory manifest without type classification and are eligible for relevance search.

### 2.6 Memory Scanning

```rust
async fn scan_memory_files(memory_dir: &Path) -> Vec<MemoryHeader> {
    // Read .md files, exclude MEMORY.md
    // Max 200 files, newest-first by mtime
    // Extract frontmatter from first 30 lines
    // Return: { filename, file_path, mtime_ms, description, type }
}

fn format_memory_manifest(memories: &[MemoryHeader]) -> String {
    // "[type] filename (timestamp): description" per line
}
```

### 2.7 Relevant Memory Search (Sonnet-based)

```rust
async fn find_relevant_memories(
    query: &str,
    memory_dir: &Path,
    recent_tools: &[String],
    already_surfaced: &HashSet<String>,
) -> Vec<RelevantMemory> {
    // 1. scan_memory_files() for candidates
    // 2. Exclude already-surfaced files
    // 3. Send manifest + query to Sonnet → select up to 5 files
    // 4. Attach freshness caveat for memories >1 day old
}
```

Selection prompt: "Select memories useful for processing user's query. Up to 5. If unsure, don't include. Empty list is valid."

Model selection: Uses the configured provider's "small/fast" model (equivalent to Haiku). If the provider does not expose a small model, falls back to the default configured model. On LLM call failure, returns empty list (graceful degradation — memory prefetch is best-effort).

### 2.8 System Prompt Integration

```rust
fn load_memory_prompt() -> Option<String> {
    // Called once per session, cached via system_prompt_section("memory")
    // Returns: memory instructions + MEMORY.md content (truncated)
    // Placed after dynamic boundary in system prompt
}
```

Per-turn injection:

```rust
fn prefetch_relevant_memories(query: &str) -> Vec<MemoryAttachment> {
    // find_relevant_memories() → attachment messages (system reminder)
    // Per-file cap: 4,096 bytes
    // Session cumulative cap: 60KB
}
```

### 2.9 Memory Auto-Extraction (Background Agent)

Runs fire-and-forget at turn end via stop hooks:

```rust
async fn execute_extract_memories(context: &HookContext) {
    // Gate checks:
    //   - Feature enabled? Auto memory enabled? Main agent only?
    //   - hasMemoryWritesSince() → skip if main agent wrote directly
    //   - Throttle: every N eligible turns (default 1 = every turn)
    //
    // Execution:
    //   - scan_memory_files() → existing manifest
    //   - Fork agent with restricted tools via custom canUseTool():
    //     Read/Grep/Glob: unrestricted (no path restrictions)
    //     Bash: allowed but outputs are read-only context (agent cannot use bash to modify files)
    //     Edit/Write: path validated against is_auto_mem_path() — only memory dir writes allowed
    //     All other tools: denied by canUseTool returning false
    //   - Max 5 turns
    //   - On success, advance cursor (lastMemoryMessageUuid)
    //
    // Coalescing:
    //   - If extraction in-progress and new context arrives, stash for trailing run
    //   - drainPendingExtraction() before shutdown
}
```

### 2.10 Enable/Disable

```rust
fn is_auto_memory_enabled() -> bool {
    // 1. ATOM_CODE_DISABLE_AUTO_MEMORY=1 → false
    // 2. --bare flag (ATOM_CODE_SIMPLE) → false
    // 3. settings.json autoMemoryEnabled: false → false
    // 4. Default: true
}
```

### 2.11 Write Permission Bypass

Memory files can be written without dangerous-mode permission prompts:

- `is_auto_mem_path(path)` returns true
- projectSettings excluded from autoMemoryDirectory override (malicious repo prevention)
- Path validation rejects: relative paths, root/near-root, UNC paths, null bytes, `~/../`

### 2.12 Team Memory

- Path: `{auto_mem_path}/team/`
- Symlink-aware: `realpath_deepest_existing()` resolves symlinks on ancestor dirs
- Security: rejects null bytes, URL-encoded traversals, Unicode normalization attacks
- Scope guidance: user=always private, project=bias team, reference=usually team

### 2.13 Limits Summary

| Item | Value |
|------|-------|
| MEMORY.md line cap | 200 |
| MEMORY.md byte cap | 25,000 |
| Frontmatter read depth | 30 lines |
| Max scanned files | 200 |
| Per-turn injected files | 5 |
| Per-file injection bytes | 4,096 |
| Session cumulative bytes | 61,440 (60KB) |

---

## 3. File Index (Fuzzy File Search)

### 3.1 Data Structures

```rust
pub struct FileIndex {
    paths: Vec<String>,
    lower_paths: Vec<String>,
    char_bits: Vec<u32>,               // 26-bit bitmap (a-z) per path
    path_lens: Vec<u16>,
    top_level_cache: Option<Vec<SearchResult>>,
    ready_count: usize,                // Searchable range during async build
}

pub struct SearchResult {
    pub path: String,
    pub score: f64,                    // 0-1, lower is better
}
```

### 3.2 Scoring Constants

```rust
const SCORE_MATCH: i32 = 16;
const BONUS_BOUNDARY: i32 = 8;
const BONUS_CAMEL: i32 = 6;
const BONUS_CONSECUTIVE: i32 = 4;
const BONUS_FIRST_CHAR: i32 = 8;
const PENALTY_GAP_START: i32 = 3;
const PENALTY_GAP_EXTENSION: i32 = 1;
const MAX_QUERY_LEN: usize = 64;
const TOP_LEVEL_CACHE_LIMIT: usize = 100;
const CHUNK_MS: u64 = 4;
```

### 3.3 Fuzzy Matching Algorithm

```rust
fn score_path(&self, query: &str, path_idx: usize) -> Option<f64> {
    // 1. Smart Case Detection:
    //    - Query has uppercase → case-sensitive
    //    - All lowercase → case-insensitive
    //
    // 2. Bitmap Fast-Reject:
    //    - Precomputed 26-bit bitmap per path (a-z)
    //    - If path missing any query letter → reject (~89% filtered)
    //
    // 3. Multi-Start Position Strategy:
    //    - Try leftmost occurrence of first character
    //    - Also try every word-boundary position where first char occurs
    //    - Boundary chars: '/', '\\', '-', '_', '.', ' '
    //
    // 4. Per-start scoring:
    //    - gap == 0 → +BONUS_CONSECUTIVE
    //    - gap > 0  → -(PENALTY_GAP_START + gap * PENALTY_GAP_EXTENSION)
    //    - Position bonus via score_bonus_at():
    //      - Start of string: BONUS_FIRST_CHAR (8)
    //      - After boundary char: BONUS_BOUNDARY (8)
    //      - CamelCase transition (lower→upper): BONUS_CAMEL (6)
    //      - Otherwise: 0
    //
    // 5. length_bonus = max(0, 32 - (path_len >> 2))
    //
    // 6. Test file penalty: if "test" in path → score * 1.05 (cap 1.0)
    //
    // 7. Normalization: position_score = 1.0 - (raw_score / max_possible_score)
    //    where max_possible_score = query_len * SCORE_MATCH + all bonuses
    //    Result is 0-1 where lower is better (inverted from raw scoring).
    //    The Top-K array stores these normalized scores in ascending order.
    //
    // 8. Return best score across all start positions
}
```

### 3.4 Top-K Search

```rust
fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
    // Empty query → return top_level_cache (unique first path segments, max 100)
    // Maintain sorted-ascending array of best `limit` matches
    // Binary search insertion to keep threshold updated
    // Only scan up to ready_count (supports partial results during async build)
    // Reverse result (best first) before returning
}
```

### 3.5 Async Indexing

```rust
const CHUNK_MS: u64 = 4;

async fn load_from_file_list_async(&mut self, file_list: Vec<String>) {
    // Phase 1: Deduplication (HashSet)
    //   - Every 256 items: check elapsed time, yield if > 4ms
    //
    // Phase 2: Bitmap + lowercase construction
    //   - Every 256 items: check elapsed, yield if > 4ms
    //   - After first chunk: update ready_count (partial search enabled)
    //   - On completion: set ready_count to total
}
```

### 3.6 Integration: `@` File Mention

```
TUI '@' keystroke:
  1. start_background_cache_refresh()
     - Check .git/index mtime for changes
     - If changed: git ls-files (tracked + untracked) → rebuild FileIndex
     - Signature-based dedup to skip rebuild when paths unchanged
  2. search(partial_path, 15) → up to 15 suggestions
  3. ready_count enables partial results during build
  4. Re-search on build completion for upgraded results
```

---

## 4. Coordinator (Multi-Agent)

### 4.1 Mode Detection

```rust
fn is_coordinator_mode() -> bool {
    // Feature gate check + ATOM_CODE_COORDINATOR_MODE env var
    // Read env var LIVE on every call (no caching — supports session resume)
}
```

### 4.2 Coordinator Tool Restriction

Coordinator has only 4 tools (+ MCP subscription tools):

```rust
const COORDINATOR_MODE_ALLOWED_TOOLS: &[&str] = &[
    "Agent",
    "TaskStop",
    "SendMessage",
    "SyntheticOutput",
];
```

### 4.3 Worker Tool Pool (ASYNC_AGENT_ALLOWED_TOOLS)

```rust
const ASYNC_AGENT_ALLOWED_TOOLS: &[&str] = &[
    "Read", "Write", "Edit", "Glob", "Grep", "Bash",
    "WebSearch", "WebFetch", "TodoWrite", "NotebookEdit",
    "Skill", "ToolSearch", "SyntheticOutput",
    "EnterWorktree", "ExitWorktree",
];
```

In SIMPLE mode (`ATOM_CODE_SIMPLE`): only `Bash`, `Read`, `Edit`.

Blocked for all async agents: `Agent` (recursion prevention), `TaskStop`, `AskUserQuestion`, `TaskOutput`.

Internal-only tools (coordinator only, not workers): `TeamCreate`, `TeamDelete`, `SendMessage`, `SyntheticOutput`.

### 4.4 Agent Tool Input Schema

```rust
pub struct AgentToolInput {
    pub prompt: String,                      // Task description for the worker
    pub subagent_type: Option<String>,       // Agent type (e.g., "general-purpose", "Explore")
    pub description: Option<String>,         // Short description (3-5 words)
    pub model: Option<String>,               // Model override
    pub run_in_background: Option<bool>,     // Async execution
    pub name: Option<String>,                // Named agent (for SendMessage continuation)
    pub team_name: Option<String>,           // Team membership
    pub isolation: Option<String>,           // "worktree" for git isolation
    pub cwd: Option<String>,                 // Working directory override
}
```

Supporting tool schemas:

```rust
// SendMessage — coordinator sends message to a named/running worker
pub struct SendMessageInput {
    pub to: String,      // Agent ID or name
    pub message: String,
}

// TaskStop — coordinator stops a running worker
pub struct TaskStopInput {
    pub task_id: String,
}

// SyntheticOutput — structured output tool (agent hooks + workers)
pub struct SyntheticOutputInput {
    pub ok: bool,
    pub reason: Option<String>,
}
```

### 4.5 Worker Spawn Flow

```rust
async fn spawn_agent(input: AgentToolInput) -> ToolResult {
    // 1. Input validation:
    //    - team_name → check agent swarm enabled
    //    - Teammate spawning teammate → error (flat roster)
    //    - In-process teammate + background → error
    //
    // 2. Agent type resolution (explicit subagent_type or default general-purpose)
    //
    // 3. Permission check (MCP requirements, deny rules)
    //
    // 4. Tool pool assembly:
    //    worker_permission_context = { mode: agent.permission_mode ?? "acceptEdits" }
    //    worker_tools = assemble_tool_pool(worker_permission_context, mcp_tools)
    //
    // 5. Sync vs Async:
    //    - Sync: run_agent() → return result directly
    //    - Async: register_async_agent() → background task
    //            return { status: "async_launched", agent_id, output_file }
}
```

### 4.6 Async Agent Lifecycle

```rust
async fn run_async_agent_lifecycle(params: AsyncAgentParams) {
    // 1. Stream agent messages
    // 2. Update progress in app state
    // 3. Finalize result via finalize_agent_tool()
    // 4. Enqueue task-notification (XML format)
    // 5. Cleanup worktree if isolated
}
```

Task notification format:

```xml
<task-notification>
<task-id>{agent_id}</task-id>
<status>completed|failed|killed</status>
<summary>{human-readable summary}</summary>
<result>{agent's final response}</result>
<usage>
  <total_tokens>N</total_tokens>
  <tool_uses>N</tool_uses>
  <duration_ms>N</duration_ms>
</usage>
</task-notification>
```

Note: `AsyncAgentParams` contains `{ task_id, abort_controller, agent_definition, prompt, tools, system_prompt }`. `finalize_agent_tool()` collects the last assistant message, computes usage stats, and formats the task-notification XML.

Creating an agent worktree fires a `WorktreeCreate` hook event; cleanup fires `WorktreeRemove`.

### 4.7 Worktree Isolation

```rust
if isolation == "worktree" {
    let slug = format!("agent-{}", &agent_id[..8]);
    let worktree_info = create_agent_worktree(&slug).await;
    // Inject path translation notice to worker
    // After completion: delete if no changes, keep if changes exist
}
```

### 4.8 Session Mode Matching

```rust
fn match_session_mode(session_mode: Option<&str>) {
    // On resume: compare stored mode with current is_coordinator_mode()
    // Mismatch → dynamically flip env var (no restart needed)
    // Return warning message about mode switch
}
```

### 4.9 Coordinator Context Injection

```rust
fn get_coordinator_user_context(
    mcp_clients: &[McpClient],
    scratchpad: Option<&Path>,
) -> HashMap<String, String> {
    // "Workers have access to: Read, Write, Edit, Bash, ..."
    // "MCP servers: server1, server2"
    // "Scratchpad: /path — workers can read/write without permission"
}
```

### 4.10 Worker-Coordinator Communication

- **Coordinator → Worker**: Agent tool call with prompt + system prompt injection
- **Worker → Coordinator**: Task-notification XML enqueued as user-role message
- **Continuation**: `SendMessage({ to: agent_id, message: "..." })` via agent name registry

---

## 5. Plugins

### 5.1 Plugin Definition

```rust
pub struct BuiltinPluginDefinition {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub skills: Option<Vec<BundledSkillDefinition>>,
    pub hooks: Option<HooksSettings>,
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,
    pub is_available: Option<fn() -> bool>,
    pub default_enabled: Option<bool>,  // Default: true
}
```

### 5.2 Plugin ID Format

- Built-in: `{name}@builtin`
- Marketplace: `{name}@{marketplace}`
- Session/inline: `{name}@inline`

### 5.3 Two-Phase Loading

```rust
// Phase 1: Full load (with network) — /plugins command, explicit refresh
fn load_all_plugins() -> PluginLoadResult;

// Phase 2: Cache-only — startup path
fn load_all_plugins_cache_only() -> PluginLoadResult;
// Reads installed_plugins.json, no network
```

Both are memoized with separate caches.

### 5.4 Enable/Disable State

```rust
fn is_plugin_enabled(plugin_id: &str, settings: &Settings) -> bool {
    // settings.enabled_plugins[plugin_id] if present → use that value
    // Otherwise → definition.default_enabled (default true)
}
```

### 5.5 Plugin Capabilities

- **Hooks**: `hooks/hooks.json` → uses the same format as settings.json `"hooks"` key (event → matchers → hooks array). Converted to `PluginHookMatcher` (a `HookMatcher` with additional `plugin_root` and `plugin_name` context fields) and registered in hook system via `register_hook_callbacks()`
- **Skills**: `BundledSkillDefinition` → `Command` with source='bundled'
- **MCP Servers**: `mcp_servers` record → integrated with MCP initialization
- **Output Styles**: `output-styles/` directory → namespaced as `pluginName:styleName`
- **Settings**: Plugin settings merged into settings cascade

### 5.6 Hot Reload

```rust
fn setup_plugin_hook_hot_reload() {
    // Subscribe to settings change detector
    // On policySettings change:
    //   - Compute plugin-affecting snapshot (enabledPlugins, marketplaces, blocked)
    //   - If snapshot changed: clear caches + reload
}
```

### 5.7 Plugin Manifest Format (plugin.json)

```json
{
  "name": "plugin-name",
  "version": "1.0.0",
  "description": "What this plugin does",
  "commands": "./commands",
  "agents": "./agents",
  "skills": "./skills",
  "outputStyles": "./output-styles",
  "hooks": "./hooks/hooks.json",
  "mcpServers": "./.mcp.json",
  "dependencies": ["dependency-name@marketplace"],
  "userConfig": {
    "api_key": {
      "type": "string",
      "title": "API Key",
      "sensitive": true
    }
  }
}
```

---

## 6. Output Styles

### 6.1 File Format

```markdown
---
name: "Custom Style Name"
description: "Brief description"
keep-coding-instructions: true
force-for-plugin: false
---

# Output Style Instructions
...
```

Frontmatter fields:

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `name` | string | No | Filename used if omitted |
| `description` | string | No | First markdown line if omitted |
| `keep-coding-instructions` | bool | No | Supports "true"/"false" strings |
| `force-for-plugin` | bool | No | Plugin-only: auto-apply when plugin enabled |

### 6.2 Loading Sources (lowest to highest priority)

1. **Built-in**: `default` (no injection), `Explanatory`, `Learning`
2. **Plugin**: `~/.atom/plugins/*/output-styles/` → namespaced `pluginName:styleName`
3. **User**: `~/.atom/output-styles/`
4. **Project**: `.agents/output-styles/`
5. **Managed**: policy settings

Later sources override earlier ones by style name.

### 6.3 Selection Logic

```rust
fn get_output_style_config() -> Option<OutputStyleConfig> {
    // 1. Forced plugin styles (force_for_plugin=true) → highest priority
    //    Multiple forced → use first, log warning
    // 2. User's selected style from settings.output_style
    // 3. Default: "default" (no injection)
}
```

### 6.4 System Prompt Injection

```
# Output Style: {name}
{prompt content}
```

Cached via `system_prompt_section("output_style")`. Invalidated on `/clear` or `/compact`.

### 6.5 Memoization

- `get_output_style_dir_styles()`: per-cwd cache
- `get_all_output_styles()`: merged cache
- `load_plugin_output_styles()`: separate cache
- Invalidation triggers: plugin change, file change, settings change

### 6.6 OutputStyleConfig Type

```rust
pub struct OutputStyleConfig {
    pub name: String,
    pub description: String,
    pub prompt: String,              // Markdown content (system prompt injection)
    pub source: SettingSource,       // built-in, plugin, userSettings, projectSettings, policySettings
    pub keep_coding_instructions: Option<bool>,
    pub force_for_plugin: Option<bool>,
}
```

---

## Module Dependency Graph

```
hooks ← (standalone, enhances existing services/hooks/)
memdir ← hooks (extract_memories fires from Stop hook)
file_index ← (standalone, integrates with TUI @ mention)
coordinator ← hooks (SubagentStart/SubagentStop), tools (AgentTool)
plugins ← hooks (plugin hooks registration), output_styles (plugin styles)
output_styles ← plugins (plugin-provided styles), context (system prompt injection)
```

### Implementation Order

**1st priority (independent):**
- `hooks` — foundation for memdir extraction and coordinator events
- `file_index` — standalone, TUI integration only
- `memdir` — depends on hooks for extraction trigger

**2nd priority (depends on 1st):**
- `coordinator` — depends on hooks for SubagentStart/Stop events
- `plugins` — depends on hooks for plugin hook registration
- `output_styles` — depends on plugins for plugin-provided styles
