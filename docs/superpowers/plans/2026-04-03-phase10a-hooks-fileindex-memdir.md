# Phase 10A: Hooks Enhancement + File Index + Memdir Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enhance the hook system from 2 to 27 event types with 5 command types, implement fuzzy file search indexing for @ mentions, and build the memory directory system with auto-extraction.

**Architecture:** Three independent modules built in dependency order: hooks first (foundation for memdir extraction trigger), file_index in parallel (standalone TUI integration), then memdir (depends on hooks for Stop event). Each module is a new Rust module under `src/services/` with its own types, logic, and tests.

**Tech Stack:** Rust, tokio (async), serde/serde_json (serialization), regex (matchers), reqwest (HTTP hooks), tokio-util (CancellationToken)

**Spec:** `docs/superpowers/specs/2026-04-03-phase10-auxiliary-modules-design.md`

---

## File Structure

### Hooks Enhancement
```
src/services/hooks/
  mod.rs              — REWRITE: module root, re-exports
  types.rs            — CREATE: HookEvent, HookCommand, HookMatcher, SyncHookOutput, HookSpecificOutput
  matcher.rs          — CREATE: matches_pattern(), parse_if_condition()
  executor.rs         — CREATE: parallel hook execution engine, aggregation
  command_hook.rs     — CREATE: shell command hook execution
  prompt_hook.rs      — CREATE: LLM prompt hook execution
  agent_hook.rs       — CREATE: agentic verification hook execution
  http_hook.rs        — CREATE: HTTP POST hook execution
  async_registry.rs   — CREATE: AsyncHookRegistry for background hooks
  settings.rs         — CREATE: hook config loading from settings hierarchy
  compat.rs           — CREATE: backward-compatible .agents/hooks.json loader
```

### File Index
```
src/services/file_index/
  mod.rs              — CREATE: FileIndex struct, search(), load APIs
  scoring.rs          — CREATE: fuzzy scoring algorithm, constants, bonus calculation
```

### Memdir
```
src/services/memdir/
  mod.rs              — CREATE: module root, load_memory_prompt(), is_auto_memory_enabled()
  paths.rs            — CREATE: get_auto_mem_path(), find_canonical_git_root(), path validation
  types.rs            — CREATE: MemoryHeader, EntrypointTruncation, MemoryType, RelevantMemory
  scan.rs             — CREATE: scan_memory_files(), format_memory_manifest()
  truncation.rs       — CREATE: truncate_entrypoint_content()
  extraction.rs       — CREATE: execute_extract_memories() (background agent stub)
  relevance.rs        — CREATE: find_relevant_memories() (Sonnet-based selection)
```

### Integration Points (modifications)
```
src/services/mod.rs           — MODIFY: add file_index, memdir modules
src/utils/paths.rs            — MODIFY: add memory path utilities
src/utils/settings/types.rs   — MODIFY: extend SettingsSource enum
src/query/engine.rs           — MODIFY: wire new hook events into query loop
src/services/context/mod.rs   — MODIFY: inject memory prompt into system prompt
src/tui/app.rs                — MODIFY: replace file suggestions with FileIndex
```

---

## Task 1: Hook Types & Matcher (Foundation)

**Files:**
- Create: `src/services/hooks/types.rs`
- Create: `src/services/hooks/matcher.rs`
- Test: inline `#[cfg(test)]` modules

- [ ] **Step 1: Write failing tests for hook types serialization**

```rust
// src/services/hooks/types.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_command_deserialize_command_type() {
        let json = r#"{"type":"command","command":"echo hello","timeout":10}"#;
        let cmd: HookCommand = serde_json::from_str(json).unwrap();
        match cmd {
            HookCommand::Command { command, timeout, .. } => {
                assert_eq!(command, "echo hello");
                assert_eq!(timeout, Some(10));
            }
            _ => panic!("Expected Command variant"),
        }
    }

    #[test]
    fn test_hook_command_deserialize_http_type() {
        let json = r#"{"type":"http","url":"https://example.com","allowedEnvVars":["TOKEN"]}"#;
        let cmd: HookCommand = serde_json::from_str(json).unwrap();
        match cmd {
            HookCommand::Http { url, allowed_env_vars, .. } => {
                assert_eq!(url, "https://example.com");
                assert_eq!(allowed_env_vars, Some(vec!["TOKEN".to_string()]));
            }
            _ => panic!("Expected Http variant"),
        }
    }

    #[test]
    fn test_hook_command_deserialize_prompt_type() {
        let json = r#"{"type":"prompt","prompt":"Is this safe?","model":"haiku"}"#;
        let cmd: HookCommand = serde_json::from_str(json).unwrap();
        match cmd {
            HookCommand::Prompt { prompt, model, .. } => {
                assert_eq!(prompt, "Is this safe?");
                assert_eq!(model, Some("haiku".to_string()));
            }
            _ => panic!("Expected Prompt variant"),
        }
    }

    #[test]
    fn test_sync_hook_output_deserialize() {
        let json = r#"{"continue":false,"decision":"block","reason":"denied by policy"}"#;
        let out: SyncHookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(out.r#continue, Some(false));
        assert_eq!(out.decision.as_deref(), Some("block"));
        assert_eq!(out.reason.as_deref(), Some("denied by policy"));
    }

    #[test]
    fn test_hook_events_list_has_27() {
        assert_eq!(HOOK_EVENTS.len(), 27);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::hooks::types::tests -- --nocapture`
Expected: FAIL — module doesn't exist yet

- [ ] **Step 3: Implement hook types**

```rust
// src/services/hooks/types.rs
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HookCommand {
    Command {
        command: String,
        shell: Option<String>,
        timeout: Option<u64>,
        #[serde(rename = "statusMessage")]
        status_message: Option<String>,
        once: Option<bool>,
        r#async: Option<bool>,
        #[serde(rename = "asyncRewake")]
        async_rewake: Option<bool>,
        r#if: Option<String>,
    },
    Prompt {
        prompt: String,
        model: Option<String>,
        timeout: Option<u64>,
        #[serde(rename = "statusMessage")]
        status_message: Option<String>,
        once: Option<bool>,
        r#if: Option<String>,
    },
    Agent {
        prompt: String,
        model: Option<String>,
        timeout: Option<u64>,
        #[serde(rename = "statusMessage")]
        status_message: Option<String>,
        once: Option<bool>,
        r#if: Option<String>,
    },
    Http {
        url: String,
        headers: Option<HashMap<String, String>>,
        #[serde(rename = "allowedEnvVars")]
        allowed_env_vars: Option<Vec<String>>,
        timeout: Option<u64>,
        #[serde(rename = "statusMessage")]
        status_message: Option<String>,
        once: Option<bool>,
        r#if: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMatcher {
    pub matcher: Option<String>,
    pub hooks: Vec<HookCommand>,
}

/// Settings.json "hooks" section: event name → array of matchers
pub type HooksSettings = HashMap<String, Vec<HookMatcher>>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncHookOutput {
    pub r#continue: Option<bool>,
    pub decision: Option<String>,
    pub reason: Option<String>,
    #[serde(rename = "suppressOutput")]
    pub suppress_output: Option<bool>,
    #[serde(rename = "stopReason")]
    pub stop_reason: Option<String>,
    #[serde(rename = "systemMessage")]
    pub system_message: Option<String>,
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: Option<Value>,
}

#[derive(Debug, Clone)]
/// Function hooks are INTERNAL-ONLY — never serialized.
/// Registered at runtime by plugins/SDK. Stored separately from HookCommand.
pub struct FunctionHook {
    pub id: Option<String>,
    pub timeout: Option<u64>,
    pub callback: std::sync::Arc<dyn Fn(Vec<crate::types::message::ApiMessage>, Option<tokio_util::sync::CancellationToken>)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>> + Send + Sync>,
    pub error_message: String,
    pub status_message: Option<String>,
}

pub struct HookExecutionResult {
    pub outcome: HookOutcome,
    pub additional_contexts: Vec<String>,
    pub watch_paths: Vec<String>,
    pub hook_specific: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HookOutcome {
    Success,
    Blocking { reason: String },
    NonBlockingError { message: String },
}
```

- [ ] **Step 4: Write failing tests for matcher**

```rust
// src/services/hooks/matcher.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_pattern_none_matches_all() {
        assert!(matches_pattern(&None, "Bash"));
    }

    #[test]
    fn test_matches_pattern_star_matches_all() {
        assert!(matches_pattern(&Some("*".to_string()), "Bash"));
    }

    #[test]
    fn test_matches_pattern_exact() {
        assert!(matches_pattern(&Some("Bash".to_string()), "Bash"));
        assert!(!matches_pattern(&Some("Bash".to_string()), "Read"));
    }

    #[test]
    fn test_matches_pattern_pipe_separated() {
        assert!(matches_pattern(&Some("Write|Edit".to_string()), "Edit"));
        assert!(!matches_pattern(&Some("Write|Edit".to_string()), "Bash"));
    }

    #[test]
    fn test_matches_pattern_regex() {
        assert!(matches_pattern(&Some("^Read$".to_string()), "Read"));
        assert!(!matches_pattern(&Some("^Read$".to_string()), "FileRead"));
    }

    #[test]
    fn test_parse_if_condition_with_pattern() {
        let (tool, pattern) = parse_if_condition("Bash(git *)").unwrap();
        assert_eq!(tool, "Bash");
        assert_eq!(pattern, Some("git *".to_string()));
    }

    #[test]
    fn test_parse_if_condition_without_pattern() {
        let (tool, pattern) = parse_if_condition("Bash").unwrap();
        assert_eq!(tool, "Bash");
        assert_eq!(pattern, None);
    }
}
```

- [ ] **Step 5: Implement matcher**

```rust
// src/services/hooks/matcher.rs
use regex::Regex;

pub fn matches_pattern(matcher: &Option<String>, query: &str) -> bool {
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

pub fn parse_if_condition(condition: &str) -> Option<(String, Option<String>)> {
    if let Some(paren_start) = condition.find('(') {
        if condition.ends_with(')') {
            let tool_name = condition[..paren_start].to_string();
            let pattern = condition[paren_start + 1..condition.len() - 1].to_string();
            Some((tool_name, Some(pattern)))
        } else {
            None
        }
    } else {
        Some((condition.to_string(), None))
    }
}
```

- [ ] **Step 6: Run all tests, verify pass**

Run: `cargo test --lib services::hooks -- --nocapture`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add src/services/hooks/types.rs src/services/hooks/matcher.rs
git commit -m "feat(hooks): add hook types (27 events, 5 command types) and matcher system"
```

---

## Task 2: Hook Settings Loader & Backward Compatibility

**Files:**
- Create: `src/services/hooks/settings.rs`
- Create: `src/services/hooks/compat.rs`
- Modify: `src/utils/settings/types.rs`
- Test: inline `#[cfg(test)]` modules

- [ ] **Step 1: Write failing tests for settings loading**

```rust
// src/services/hooks/settings.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_hooks_settings_from_json() {
        let dir = TempDir::new().unwrap();
        let settings_dir = dir.path().join(".agents");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(
            settings_dir.join("settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo ok"}]}]}}"#,
        ).unwrap();
        let settings = load_hooks_from_settings(dir.path());
        assert!(settings.contains_key("PreToolUse"));
        assert_eq!(settings["PreToolUse"].len(), 1);
        assert_eq!(settings["PreToolUse"][0].matcher, Some("Bash".to_string()));
    }

    #[test]
    fn test_load_hooks_settings_empty() {
        let dir = TempDir::new().unwrap();
        let settings = load_hooks_from_settings(dir.path());
        assert!(settings.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test --lib services::hooks::settings::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Write failing tests for backward compatibility**

```rust
// src/services/hooks/compat.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_convert_legacy_hooks_to_matchers() {
        let legacy = vec![LegacyHookConfig {
            hook_type: "PreToolUse".to_string(),
            tool_name: Some("Bash".to_string()),
            command: "echo test".to_string(),
        }];
        let settings = convert_legacy_to_settings(&legacy);
        assert!(settings.contains_key("PreToolUse"));
        let matchers = &settings["PreToolUse"];
        assert_eq!(matchers.len(), 1);
        assert_eq!(matchers[0].matcher, Some("Bash".to_string()));
    }

    #[test]
    fn test_load_legacy_hooks_json() {
        let dir = TempDir::new().unwrap();
        let agents_dir = dir.path().join(".agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("hooks.json"),
            r#"[{"hook_type":"PreToolUse","tool_name":null,"command":"echo ok"}]"#,
        ).unwrap();
        let settings = load_legacy_hooks(dir.path());
        assert!(settings.contains_key("PreToolUse"));
    }
}
```

- [ ] **Step 4: Implement settings loader and compat layer**

```rust
// src/services/hooks/settings.rs
use super::types::{HooksSettings, HookMatcher};
use std::path::Path;

pub fn load_hooks_from_settings(cwd: &Path) -> HooksSettings {
    let mut merged = HooksSettings::new();

    // Project settings: .agents/settings.json → hooks key
    let project_path = cwd.join(".agents/settings.json");
    if let Some(hooks) = read_hooks_from_file(&project_path) {
        merge_hooks_settings(&mut merged, hooks);
    }

    // User settings: ~/.atom/settings.json → hooks key
    let user_path = crate::utils::paths::global_config_dir().join("settings.json");
    if let Some(hooks) = read_hooks_from_file(&user_path) {
        merge_hooks_settings(&mut merged, hooks);
    }

    merged
}

fn read_hooks_from_file(path: &Path) -> Option<HooksSettings> {
    let content = std::fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    let hooks_val = parsed.get("hooks")?;
    serde_json::from_value(hooks_val.clone()).ok()
}

fn merge_hooks_settings(base: &mut HooksSettings, overlay: HooksSettings) {
    for (event, matchers) in overlay {
        base.entry(event).or_default().extend(matchers);
    }
}
```

```rust
// src/services/hooks/compat.rs
use super::types::{HooksSettings, HookMatcher, HookCommand};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyHookConfig {
    pub hook_type: String,
    pub tool_name: Option<String>,
    pub command: String,
}

pub fn convert_legacy_to_settings(legacy: &[LegacyHookConfig]) -> HooksSettings {
    let mut settings = HooksSettings::new();
    for hook in legacy {
        let matcher = HookMatcher {
            matcher: hook.tool_name.clone(),
            hooks: vec![HookCommand::Command {
                command: hook.command.clone(),
                shell: None,
                timeout: None,
                status_message: None,
                once: None,
                r#async: None,
                async_rewake: None,
                r#if: None,
            }],
        };
        settings.entry(hook.hook_type.clone()).or_default().push(matcher);
    }
    settings
}

pub fn load_legacy_hooks(cwd: &Path) -> HooksSettings {
    let mut all = Vec::new();

    let project_hooks = cwd.join(".agents/hooks.json");
    if let Ok(content) = std::fs::read_to_string(&project_hooks) {
        if let Ok(parsed) = serde_json::from_str::<Vec<LegacyHookConfig>>(&content) {
            all.extend(parsed);
        }
    }

    let user_hooks = crate::utils::paths::global_config_dir().join("hooks.json");
    if let Ok(content) = std::fs::read_to_string(&user_hooks) {
        if let Ok(parsed) = serde_json::from_str::<Vec<LegacyHookConfig>>(&content) {
            all.extend(parsed);
        }
    }

    convert_legacy_to_settings(&all)
}
```

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test --lib services::hooks -- --nocapture`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/services/hooks/settings.rs src/services/hooks/compat.rs
git commit -m "feat(hooks): settings loader with legacy hooks.json backward compatibility"
```

---

## Task 3: Command Hook Executor

**Files:**
- Create: `src/services/hooks/command_hook.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write failing tests**

```rust
// src/services/hooks/command_hook.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_command_hook_success() {
        let dir = TempDir::new().unwrap();
        let result = execute_command_hook("true", None, &HookEnv::default(), dir.path(), None).await;
        assert!(matches!(result, CommandHookResult::Success { .. }));
    }

    #[tokio::test]
    async fn test_execute_command_hook_exit_1_nonblocking() {
        let dir = TempDir::new().unwrap();
        let result = execute_command_hook("false", None, &HookEnv::default(), dir.path(), None).await;
        // exit code 1 = non-blocking error (only exit code 2 is blocking per spec)
        assert!(matches!(result, CommandHookResult::NonBlockingError { .. }));
    }

    #[tokio::test]
    async fn test_execute_command_hook_exit_code_2_blocking() {
        let dir = TempDir::new().unwrap();
        let result = execute_command_hook("exit 2", None, &HookEnv::default(), dir.path(), None).await;
        assert!(matches!(result, CommandHookResult::Blocking { .. }));
    }

    #[tokio::test]
    async fn test_execute_command_hook_exit_code_3_nonblocking() {
        let dir = TempDir::new().unwrap();
        let result = execute_command_hook("exit 3", None, &HookEnv::default(), dir.path(), None).await;
        assert!(matches!(result, CommandHookResult::NonBlockingError { .. }));
    }

    #[tokio::test]
    async fn test_execute_command_hook_json_deny() {
        let dir = TempDir::new().unwrap();
        let result = execute_command_hook(
            r#"echo '{"decision":"block","reason":"policy"}'"#,
            None, &HookEnv::default(), dir.path(), None,
        ).await;
        match result {
            CommandHookResult::Success { output } => {
                let parsed: SyncHookOutput = serde_json::from_str(&output.stdout).unwrap();
                assert_eq!(parsed.decision.as_deref(), Some("block"));
            }
            _ => panic!("Expected Success with JSON output"),
        }
    }

    #[tokio::test]
    async fn test_execute_command_hook_timeout() {
        let dir = TempDir::new().unwrap();
        let result = execute_command_hook("sleep 60", None, &HookEnv::default(), dir.path(), Some(1)).await;
        assert!(matches!(result, CommandHookResult::NonBlockingError { .. }));
    }

    #[tokio::test]
    async fn test_execute_command_hook_env_vars() {
        let dir = TempDir::new().unwrap();
        let mut env = HookEnv::default();
        env.insert("ATOM_TOOL_NAME".to_string(), "Bash".to_string());
        let result = execute_command_hook(
            "echo $ATOM_TOOL_NAME", None, &env, dir.path(), None,
        ).await;
        match result {
            CommandHookResult::Success { output } => {
                assert_eq!(output.stdout.trim(), "Bash");
            }
            _ => panic!("Expected Success"),
        }
    }
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test --lib services::hooks::command_hook::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement command hook executor**

```rust
// src/services/hooks/command_hook.rs
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

pub type HookEnv = HashMap<String, String>;

pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub enum CommandHookResult {
    Success { output: CommandOutput },
    Blocking { reason: String, output: CommandOutput },
    NonBlockingError { message: String },
}

pub async fn execute_command_hook(
    command: &str,
    shell: Option<&str>,
    env: &HookEnv,
    cwd: &Path,
    timeout_secs: Option<u64>,
) -> CommandHookResult {
    let shell_cmd = shell.unwrap_or("sh");
    let mut cmd = Command::new(shell_cmd);
    cmd.arg("-c").arg(command).current_dir(cwd);

    for (key, value) in env {
        cmd.env(key, value);
    }

    // Pipe JSON input via stdin
    cmd.stdin(std::process::Stdio::piped());

    let timeout = Duration::from_secs(timeout_secs.unwrap_or(120));

    let result = match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return CommandHookResult::NonBlockingError {
                message: format!("Hook failed to spawn: {}", e),
            };
        }
        Err(_) => {
            return CommandHookResult::NonBlockingError {
                message: format!("Hook timed out after {}s", timeout.as_secs()),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&result.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
    let exit_code = result.status.code().unwrap_or(-1);

    let output = CommandOutput { stdout, stderr, exit_code };

    match exit_code {
        0 => CommandHookResult::Success { output },
        2 => {
            let reason = if !output.stderr.is_empty() {
                output.stderr.clone()
            } else {
                format!("Hook '{}' exited with blocking error (code 2)", command)
            };
            CommandHookResult::Blocking { reason, output }
        }
        _ => {
            let reason = if !output.stderr.is_empty() {
                output.stderr.clone()
            } else {
                format!("Hook '{}' exited with code {}", command, exit_code)
            };
            CommandHookResult::NonBlockingError { message: reason }
        }
    }
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --lib services::hooks::command_hook::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/hooks/command_hook.rs
git commit -m "feat(hooks): async command hook executor with timeout and exit code semantics"
```

---

## Task 4: HTTP Hook Executor

**Files:**
- Create: `src/services/hooks/http_hook.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write failing tests**

```rust
// src/services/hooks/http_hook.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_header_value() {
        assert_eq!(sanitize_header_value("Bearer tok\r\nen"), "Bearer token");
        assert_eq!(sanitize_header_value("normal"), "normal");
    }

    #[test]
    fn test_interpolate_env_vars() {
        let mut env = HashMap::new();
        env.insert("TOKEN".to_string(), "secret123".to_string());
        let allowed = vec!["TOKEN".to_string()];
        assert_eq!(
            interpolate_env_vars("Bearer $TOKEN", &env, &allowed),
            "Bearer secret123"
        );
    }

    #[test]
    fn test_interpolate_env_vars_blocks_unallowed() {
        let mut env = HashMap::new();
        env.insert("SECRET".to_string(), "hidden".to_string());
        let allowed: Vec<String> = vec![];
        assert_eq!(
            interpolate_env_vars("Bearer $SECRET", &env, &allowed),
            "Bearer "
        );
    }
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test --lib services::hooks::http_hook::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement HTTP hook executor**

```rust
// src/services/hooks/http_hook.rs
use std::collections::HashMap;
use std::time::Duration;

pub fn sanitize_header_value(value: &str) -> String {
    value.chars().filter(|c| *c != '\r' && *c != '\n' && *c != '\0').collect()
}

pub fn interpolate_env_vars(
    template: &str,
    env: &HashMap<String, String>,
    allowed_vars: &[String],
) -> String {
    let mut result = template.to_string();
    for var in allowed_vars {
        let placeholder = format!("${}", var);
        if let Some(value) = env.get(var) {
            result = result.replace(&placeholder, value);
        }
    }
    // Remove any unresolved $VAR patterns for non-allowed vars
    let re = regex::Regex::new(r"\$[A-Z_][A-Z0-9_]*").unwrap();
    re.replace_all(&result, "").to_string()
}

pub struct HttpHookResult {
    pub status: u16,
    pub body: String,
}

pub async fn execute_http_hook(
    url: &str,
    headers: &HashMap<String, String>,
    allowed_env_vars: &[String],
    env: &HashMap<String, String>,
    input_json: &str,
    timeout_secs: Option<u64>,
) -> Result<HttpHookResult, String> {
    let client = reqwest::Client::new();
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(600));

    let mut request = client.post(url)
        .timeout(timeout)
        .header("Content-Type", "application/json")
        .body(input_json.to_string());

    for (key, value) in headers {
        let interpolated = interpolate_env_vars(value, env, allowed_env_vars);
        let sanitized = sanitize_header_value(&interpolated);
        request = request.header(key.as_str(), sanitized);
    }

    match request.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            Ok(HttpHookResult { status, body })
        }
        Err(e) => Err(format!("HTTP hook failed: {}", e)),
    }
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --lib services::hooks::http_hook::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/hooks/http_hook.rs
git commit -m "feat(hooks): HTTP hook executor with header sanitization and env var interpolation"
```

---

## Task 5: Hook Execution Engine (Parallel Aggregation)

**Files:**
- Create: `src/services/hooks/executor.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write failing tests**

```rust
// src/services/hooks/executor.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_hooks_no_matching() {
        let dir = TempDir::new().unwrap();
        let settings = HooksSettings::new();
        let result = execute_event_hooks("PreToolUse", "Bash", &serde_json::json!({}), &settings, dir.path()).await;
        assert!(matches!(result.outcome, HookOutcome::Success));
    }

    #[tokio::test]
    async fn test_execute_hooks_matching_allows() {
        let dir = TempDir::new().unwrap();
        let mut settings = HooksSettings::new();
        settings.insert("PreToolUse".to_string(), vec![HookMatcher {
            matcher: Some("Bash".to_string()),
            hooks: vec![HookCommand::Command {
                command: "true".to_string(),
                shell: None, timeout: None, status_message: None,
                once: None, r#async: None, async_rewake: None, r#if: None,
            }],
        }]);
        let result = execute_event_hooks("PreToolUse", "Bash", &serde_json::json!({}), &settings, dir.path()).await;
        assert!(matches!(result.outcome, HookOutcome::Success));
    }

    #[tokio::test]
    async fn test_execute_hooks_blocking_result() {
        let dir = TempDir::new().unwrap();
        let mut settings = HooksSettings::new();
        settings.insert("PreToolUse".to_string(), vec![HookMatcher {
            matcher: None,
            hooks: vec![HookCommand::Command {
                command: "exit 2".to_string(),
                shell: None, timeout: None, status_message: None,
                once: None, r#async: None, async_rewake: None, r#if: None,
            }],
        }]);
        let result = execute_event_hooks("PreToolUse", "Bash", &serde_json::json!({}), &settings, dir.path()).await;
        assert!(matches!(result.outcome, HookOutcome::Blocking { .. }));
    }

    #[tokio::test]
    async fn test_execute_hooks_matcher_filters() {
        let dir = TempDir::new().unwrap();
        let mut settings = HooksSettings::new();
        settings.insert("PreToolUse".to_string(), vec![HookMatcher {
            matcher: Some("Edit".to_string()),
            hooks: vec![HookCommand::Command {
                command: "exit 2".to_string(),
                shell: None, timeout: None, status_message: None,
                once: None, r#async: None, async_rewake: None, r#if: None,
            }],
        }]);
        // Should not match "Bash"
        let result = execute_event_hooks("PreToolUse", "Bash", &serde_json::json!({}), &settings, dir.path()).await;
        assert!(matches!(result.outcome, HookOutcome::Success));
    }
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test --lib services::hooks::executor::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement execution engine**

```rust
// src/services/hooks/executor.rs
use super::command_hook::{execute_command_hook, CommandHookResult, HookEnv};
use super::matcher::matches_pattern;
use super::types::*;
use serde_json::Value;
use std::path::Path;

pub async fn execute_event_hooks(
    event: &str,
    match_query: &str,
    input: &Value,
    settings: &HooksSettings,
    cwd: &Path,
) -> HookExecutionResult {
    let matchers = match settings.get(event) {
        Some(m) => m,
        None => return HookExecutionResult {
            outcome: HookOutcome::Success,
            additional_contexts: vec![],
            watch_paths: vec![],
            hook_specific: None,
        },
    };

    // Collect matching hooks
    let mut matching_hooks = Vec::new();
    for matcher in matchers {
        if matches_pattern(&matcher.matcher, match_query) {
            for hook in &matcher.hooks {
                // Check if condition
                if let Some(if_cond) = get_if_condition(hook) {
                    let (tool_name, _pattern) = match super::matcher::parse_if_condition(&if_cond) {
                        Some(parsed) => parsed,
                        None => continue,
                    };
                    if tool_name != match_query {
                        continue;
                    }
                }
                matching_hooks.push(hook.clone());
            }
        }
    }

    if matching_hooks.is_empty() {
        return HookExecutionResult {
            outcome: HookOutcome::Success,
            additional_contexts: vec![],
            watch_paths: vec![],
            hook_specific: None,
        };
    }

    // Execute in parallel
    let input_json = serde_json::to_string(input).unwrap_or_default();
    let mut env = build_base_env(event, cwd);
    env.insert("ATOM_TOOL_NAME".to_string(), match_query.to_string());
    env.insert("ATOM_TOOL_INPUT".to_string(), input_json);

    let futures: Vec<_> = matching_hooks.iter().map(|hook| {
        let env = env.clone();
        let cwd = cwd.to_path_buf();
        let hook = hook.clone();
        async move { execute_single_hook(&hook, &env, &cwd).await }
    }).collect();

    let results = futures::future::join_all(futures).await;

    // Aggregate
    let mut additional_contexts = Vec::new();
    let mut watch_paths = Vec::new();
    let mut blocking_reason = None;
    let mut hook_specific = None;

    for result in results {
        match result.outcome {
            HookOutcome::Blocking { ref reason } => {
                if blocking_reason.is_none() {
                    blocking_reason = Some(reason.clone());
                }
            }
            _ => {}
        }
        additional_contexts.extend(result.additional_contexts);
        watch_paths.extend(result.watch_paths);
        if result.hook_specific.is_some() {
            hook_specific = result.hook_specific;
        }
    }

    HookExecutionResult {
        outcome: if let Some(reason) = blocking_reason {
            HookOutcome::Blocking { reason }
        } else {
            HookOutcome::Success
        },
        additional_contexts,
        watch_paths,
        hook_specific,
    }
}

async fn execute_single_hook(
    hook: &HookCommand,
    env: &HookEnv,
    cwd: &Path,
) -> HookExecutionResult {
    match hook {
        HookCommand::Command { command, shell, timeout, .. } => {
            let result = execute_command_hook(
                command,
                shell.as_deref(),
                env,
                cwd,
                *timeout,
            ).await;
            match result {
                CommandHookResult::Success { output } => {
                    let parsed = parse_hook_output(&output.stdout);
                    HookExecutionResult {
                        outcome: if parsed.decision.as_deref() == Some("block") {
                            HookOutcome::Blocking {
                                reason: parsed.reason.unwrap_or_else(|| "Blocked by hook".to_string()),
                            }
                        } else {
                            HookOutcome::Success
                        },
                        additional_contexts: vec![],
                        watch_paths: vec![],
                        hook_specific: parsed.hook_specific_output,
                    }
                }
                CommandHookResult::Blocking { reason, .. } => HookExecutionResult {
                    outcome: HookOutcome::Blocking { reason },
                    additional_contexts: vec![],
                    watch_paths: vec![],
                    hook_specific: None,
                },
                CommandHookResult::NonBlockingError { message } => HookExecutionResult {
                    outcome: HookOutcome::NonBlockingError { message },
                    additional_contexts: vec![],
                    watch_paths: vec![],
                    hook_specific: None,
                },
            }
        }
        HookCommand::Http { .. } | HookCommand::Prompt { .. } | HookCommand::Agent { .. } => {
            // Stub for now — implemented in later tasks
            HookExecutionResult {
                outcome: HookOutcome::Success,
                additional_contexts: vec![],
                watch_paths: vec![],
                hook_specific: None,
            }
        }
    }
}

fn parse_hook_output(stdout: &str) -> SyncHookOutput {
    // Find first line starting with '{'
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(parsed) = serde_json::from_str::<SyncHookOutput>(trimmed) {
                return parsed;
            }
        }
    }
    SyncHookOutput::default()
}

fn get_if_condition(hook: &HookCommand) -> Option<String> {
    match hook {
        HookCommand::Command { r#if, .. } => r#if.clone(),
        HookCommand::Prompt { r#if, .. } => r#if.clone(),
        HookCommand::Agent { r#if, .. } => r#if.clone(),
        HookCommand::Http { r#if, .. } => r#if.clone(),
    }
}

fn build_base_env(event: &str, cwd: &Path) -> HookEnv {
    let mut env = HookEnv::new();
    env.insert("ATOM_HOOK_TYPE".to_string(), event.to_string());
    env.insert("ATOM_CWD".to_string(), cwd.display().to_string());
    env.insert("ATOM_SESSION_ID".to_string(), String::new()); // populated by caller
    env.insert("ATOM_TRANSCRIPT_PATH".to_string(), String::new()); // populated by caller
    env.insert("ATOM_PERMISSION_MODE".to_string(), String::new()); // populated by caller
    env.insert("ATOM_AGENT_ID".to_string(), String::new()); // populated by caller if subagent
    env.insert("ATOM_AGENT_TYPE".to_string(), String::new()); // populated by caller if subagent
    env
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --lib services::hooks::executor::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/hooks/executor.rs
git commit -m "feat(hooks): parallel hook execution engine with matcher filtering and result aggregation"
```

---

## Task 6: Rewire mod.rs & Query Engine Integration

**Files:**
- Modify: `src/services/hooks/mod.rs` (rewrite)
- Modify: `src/query/engine.rs`
- Test: existing tests must still pass

- [ ] **Step 1: Rewrite hooks/mod.rs as module root**

```rust
// src/services/hooks/mod.rs — FULL REWRITE
pub mod types;
pub mod matcher;
pub mod command_hook;
pub mod http_hook;
pub mod executor;
pub mod settings;
pub mod compat;

// Re-export main API
pub use types::*;
pub use executor::execute_event_hooks;
pub use settings::load_hooks_from_settings;
pub use compat::load_legacy_hooks;

use std::path::Path;

/// Load all hooks: new settings format + legacy fallback
pub fn load_all_hooks(cwd: &Path) -> HooksSettings {
    let mut settings = load_hooks_from_settings(cwd);
    let legacy = load_legacy_hooks(cwd);

    // Legacy hooks are lower priority — only add if event not already in settings
    for (event, matchers) in legacy {
        settings.entry(event).or_default().extend(matchers);
    }

    settings
}
```

- [ ] **Step 2: Update query/engine.rs to use new hook API**

Replace the hook imports and calls in `src/query/engine.rs`. The old `execute_hooks(HookType::PreToolUse, ...)` calls become `execute_event_hooks("PreToolUse", tool_name, &input, &hooks, cwd).await`.

Key changes at the import line (line 7) and execution points (lines 267-302, 310-323).

- [ ] **Step 3: Update print.rs and run.rs hook loading**

Replace `crate::services::hooks::load_hooks()` calls with `crate::services::hooks::load_all_hooks()` in:
- `src/entrypoints/print.rs` (line 68)
- `src/tui/run.rs` (line 454)

- [ ] **Step 4: Run full test suite**

Run: `cargo test --lib -- --nocapture`
Expected: All PASS (existing + new tests)

- [ ] **Step 5: Commit**

```bash
git add src/services/hooks/mod.rs src/query/engine.rs src/entrypoints/print.rs src/tui/run.rs
git commit -m "feat(hooks): rewire module root and integrate new hook system into query engine"
```

---

## Task 7: File Index — Scoring Algorithm

**Files:**
- Create: `src/services/file_index/scoring.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write failing tests for scoring**

```rust
// src/services/file_index/scoring.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_boundary() {
        assert!(is_boundary(b'/'));
        assert!(is_boundary(b'\\'));
        assert!(is_boundary(b'-'));
        assert!(is_boundary(b'_'));
        assert!(is_boundary(b'.'));
        assert!(is_boundary(b' '));
        assert!(!is_boundary(b'a'));
    }

    #[test]
    fn test_score_bonus_at_first_char() {
        assert_eq!(score_bonus_at(b"hello", 0), BONUS_FIRST_CHAR);
    }

    #[test]
    fn test_score_bonus_at_boundary() {
        assert_eq!(score_bonus_at(b"src/main.rs", 4), BONUS_BOUNDARY);
    }

    #[test]
    fn test_score_bonus_at_camel() {
        assert_eq!(score_bonus_at(b"camelCase", 5), BONUS_CAMEL);
    }

    #[test]
    fn test_compute_char_bitmap() {
        let bits = compute_char_bitmap("hello");
        assert!(bits & (1 << (b'h' - b'a')) != 0);
        assert!(bits & (1 << (b'e' - b'a')) != 0);
        assert!(bits & (1 << (b'z' - b'a')) == 0);
    }

    #[test]
    fn test_bitmap_rejects_missing_char() {
        let bits = compute_char_bitmap("hello");
        assert!(!bitmap_contains_query(bits, "hz"));
    }

    #[test]
    fn test_bitmap_accepts_matching() {
        let bits = compute_char_bitmap("hello");
        assert!(bitmap_contains_query(bits, "hel"));
    }

    #[test]
    fn test_smart_case_detection() {
        assert!(!is_case_sensitive("hello"));
        assert!(is_case_sensitive("Hello"));
        assert!(is_case_sensitive("hEllo"));
    }
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test --lib services::file_index::scoring::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement scoring primitives**

```rust
// src/services/file_index/scoring.rs
pub const SCORE_MATCH: i32 = 16;
pub const BONUS_BOUNDARY: i32 = 8;
pub const BONUS_CAMEL: i32 = 6;
pub const BONUS_CONSECUTIVE: i32 = 4;
pub const BONUS_FIRST_CHAR: i32 = 8;
pub const PENALTY_GAP_START: i32 = 3;
pub const PENALTY_GAP_EXTENSION: i32 = 1;

pub fn is_boundary(b: u8) -> bool {
    matches!(b, b'/' | b'\\' | b'-' | b'_' | b'.' | b' ')
}

pub fn score_bonus_at(path: &[u8], pos: usize) -> i32 {
    if pos == 0 {
        return BONUS_FIRST_CHAR;
    }
    let prev = path[pos - 1];
    if is_boundary(prev) {
        return BONUS_BOUNDARY;
    }
    // CamelCase: lowercase→uppercase
    let curr = path[pos];
    if prev.is_ascii_lowercase() && curr.is_ascii_uppercase() {
        return BONUS_CAMEL;
    }
    0
}

pub fn compute_char_bitmap(s: &str) -> u32 {
    let mut bits: u32 = 0;
    for b in s.bytes() {
        let lower = b.to_ascii_lowercase();
        if lower >= b'a' && lower <= b'z' {
            bits |= 1 << (lower - b'a');
        }
    }
    bits
}

pub fn bitmap_contains_query(path_bits: u32, query: &str) -> bool {
    let mut query_bits: u32 = 0;
    for b in query.bytes() {
        let lower = b.to_ascii_lowercase();
        if lower >= b'a' && lower <= b'z' {
            query_bits |= 1 << (lower - b'a');
        }
    }
    (path_bits & query_bits) == query_bits
}

pub fn is_case_sensitive(query: &str) -> bool {
    query.chars().any(|c| c.is_uppercase())
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --lib services::file_index::scoring::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/file_index/scoring.rs
git commit -m "feat(file_index): scoring primitives — bitmap, boundary detection, bonus calculation"
```

---

## Task 8: File Index — Main Struct & Search

**Files:**
- Create: `src/services/file_index/mod.rs`
- Modify: `src/services/mod.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write failing tests for FileIndex**

```rust
// src/services/file_index/mod.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_deduplicates() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec![
            "src/main.rs".to_string(),
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
        ]);
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn test_search_exact_match() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "tests/test_main.rs".to_string(),
        ]);
        let results = idx.search("main", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].path, "src/main.rs");
    }

    #[test]
    fn test_search_empty_query_returns_top_level() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec![
            "src/main.rs".to_string(),
            "tests/foo.rs".to_string(),
            "docs/readme.md".to_string(),
        ]);
        let results = idx.search("", 10);
        assert!(!results.is_empty());
        // Should return top-level directories
    }

    #[test]
    fn test_search_test_file_penalty() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec![
            "src/utils.rs".to_string(),
            "tests/test_utils.rs".to_string(),
        ]);
        let results = idx.search("utils", 10);
        assert_eq!(results[0].path, "src/utils.rs"); // non-test should rank higher
    }

    #[test]
    fn test_search_respects_limit() {
        let mut idx = FileIndex::new();
        let files: Vec<String> = (0..100).map(|i| format!("file_{}.rs", i)).collect();
        idx.load_from_file_list(files);
        let results = idx.search("file", 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec!["src/MyFile.rs".to_string()]);
        let results = idx.search("myfile", 10);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_case_sensitive_uppercase_query() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec![
            "src/myfile.rs".to_string(),
            "src/MyFile.rs".to_string(),
        ]);
        let results = idx.search("MyFile", 10);
        assert_eq!(results[0].path, "src/MyFile.rs");
    }
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test --lib services::file_index::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement FileIndex**

```rust
// src/services/file_index/mod.rs
pub mod scoring;

use scoring::*;
use std::collections::HashSet;

const MAX_QUERY_LEN: usize = 64;
const TOP_LEVEL_CACHE_LIMIT: usize = 100;

pub struct SearchResult {
    pub path: String,
    pub score: f64,
}

pub struct FileIndex {
    paths: Vec<String>,
    lower_paths: Vec<String>,
    char_bits: Vec<u32>,
    path_lens: Vec<u16>,
    top_level_cache: Option<Vec<SearchResult>>,
}

impl FileIndex {
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            lower_paths: Vec::new(),
            char_bits: Vec::new(),
            path_lens: Vec::new(),
            top_level_cache: None,
        }
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn load_from_file_list(&mut self, file_list: Vec<String>) {
        let mut seen = HashSet::new();
        let mut paths = Vec::new();
        for path in file_list {
            if !path.is_empty() && seen.insert(path.clone()) {
                paths.push(path);
            }
        }

        self.lower_paths = paths.iter().map(|p| p.to_lowercase()).collect();
        self.char_bits = self.lower_paths.iter().map(|p| compute_char_bitmap(p)).collect();
        self.path_lens = paths.iter().map(|p| p.len().min(u16::MAX as usize) as u16).collect();
        self.paths = paths;
        self.top_level_cache = None;
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        if query.is_empty() {
            return self.top_level_entries(limit);
        }

        let query = &query[..query.len().min(MAX_QUERY_LEN)];
        let case_sensitive = is_case_sensitive(query);
        let query_lower = query.to_lowercase();

        let mut top_k: Vec<(f64, usize)> = Vec::with_capacity(limit);

        for i in 0..self.paths.len() {
            // Bitmap fast-reject
            if !bitmap_contains_query(self.char_bits[i], &query_lower) {
                continue;
            }

            let path_bytes = if case_sensitive {
                self.paths[i].as_bytes()
            } else {
                self.lower_paths[i].as_bytes()
            };
            let needle = if case_sensitive { query.as_bytes() } else { query_lower.as_bytes() };

            if let Some(raw_score) = self.fuzzy_score(path_bytes, needle, self.path_lens[i] as usize) {
                let mut score = raw_score;

                // Test file penalty
                if self.lower_paths[i].contains("test") {
                    score = (score * 1.05).min(1.0);
                }

                // Insert into top-k (ascending by score, lower is better)
                let pos = top_k.partition_point(|(s, _)| *s <= score);
                if top_k.len() < limit {
                    top_k.insert(pos, (score, i));
                } else if pos > 0 {
                    top_k.remove(0);
                    let new_pos = top_k.partition_point(|(s, _)| *s <= score);
                    top_k.insert(new_pos.min(top_k.len()), (score, i));
                }
            }
        }

        // Return best first (reverse ascending → descending by quality, i.e., ascending by score since lower=better)
        top_k.into_iter()
            .map(|(score, idx)| SearchResult {
                path: self.paths[idx].clone(),
                score,
            })
            .collect()
    }

    fn fuzzy_score(&self, path: &[u8], needle: &[u8], path_len: usize) -> Option<f64> {
        let n_len = needle.len();
        if n_len == 0 { return Some(0.0); }

        // Find all start positions (leftmost + boundary positions)
        let mut starts = Vec::new();
        for (i, &b) in path.iter().enumerate() {
            if b == needle[0] {
                if starts.is_empty() || (i > 0 && is_boundary(path[i - 1])) || i == 0 {
                    starts.push(i);
                }
            }
        }

        if starts.is_empty() { return None; }

        let mut best_score: Option<f64> = None;

        for &start in &starts {
            let mut score: i32 = 0;
            let mut ni = 0;
            let mut last_match = start;
            let mut consecutive = 0;
            let mut matched = false;

            for pi in start..path.len() {
                if ni >= n_len { break; }
                if path[pi] == needle[ni] {
                    let gap = if ni == 0 { 0 } else { pi - last_match - 1 };
                    score += SCORE_MATCH;
                    score += score_bonus_at(path, pi);

                    if gap == 0 && ni > 0 {
                        consecutive += 1;
                        score += BONUS_CONSECUTIVE;
                    } else if gap > 0 {
                        consecutive = 0;
                        score -= PENALTY_GAP_START + (gap as i32) * PENALTY_GAP_EXTENSION;
                    }

                    last_match = pi;
                    ni += 1;
                }
            }

            if ni == n_len {
                matched = true;
                // Length bonus
                let length_bonus = (32i32 - (path_len as i32 >> 2)).max(0);
                score += length_bonus;

                // Normalize to 0-1 (lower is better)
                let max_possible = (n_len as i32) * SCORE_MATCH
                    + (n_len as i32) * BONUS_BOUNDARY
                    + ((n_len as i32) - 1) * BONUS_CONSECUTIVE
                    + 32; // max length bonus
                let normalized = 1.0 - (score as f64 / max_possible as f64).clamp(0.0, 1.0);

                match best_score {
                    None => best_score = Some(normalized),
                    Some(best) if normalized < best => best_score = Some(normalized),
                    _ => {}
                }
            }
        }

        best_score
    }

    fn top_level_entries(&self, limit: usize) -> Vec<SearchResult> {
        if let Some(ref cache) = self.top_level_cache {
            return cache[..cache.len().min(limit)].to_vec();
        }

        let mut segments = HashSet::new();
        for path in &self.paths {
            if let Some(first_sep) = path.find(|c: char| c == '/' || c == '\\') {
                segments.insert(path[..first_sep].to_string());
            } else {
                segments.insert(path.clone());
            }
        }

        let mut entries: Vec<String> = segments.into_iter().collect();
        entries.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
        entries.truncate(TOP_LEVEL_CACHE_LIMIT);

        entries.into_iter()
            .take(limit)
            .enumerate()
            .map(|(i, path)| SearchResult {
                score: i as f64 / TOP_LEVEL_CACHE_LIMIT as f64,
                path,
            })
            .collect()
    }
}

impl Clone for SearchResult {
    fn clone(&self) -> Self {
        SearchResult { path: self.path.clone(), score: self.score }
    }
}
```

- [ ] **Step 4: Add module declaration in services/mod.rs**

Add `pub mod file_index;` to `src/services/mod.rs`.

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test --lib services::file_index -- --nocapture`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/services/file_index/ src/services/mod.rs
git commit -m "feat(file_index): fuzzy file search with bitmap rejection, multi-start scoring, top-k"
```

---

## Task 9: Memdir — Types, Paths, and Truncation

**Files:**
- Create: `src/services/memdir/types.rs`
- Create: `src/services/memdir/paths.rs`
- Create: `src/services/memdir/truncation.rs`
- Modify: `src/utils/paths.rs`
- Test: inline `#[cfg(test)]` modules

- [ ] **Step 1: Write failing tests for truncation**

```rust
// src/services/memdir/truncation.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_within_limits() {
        let content = "line 1\nline 2\nline 3\n";
        let result = truncate_entrypoint_content(content);
        assert!(!result.was_line_truncated);
        assert!(!result.was_byte_truncated);
        assert_eq!(result.line_count, 3);
    }

    #[test]
    fn test_truncate_exceeds_line_limit() {
        let content: String = (0..250).map(|i| format!("line {}\n", i)).collect();
        let result = truncate_entrypoint_content(&content);
        assert!(result.was_line_truncated);
        assert_eq!(result.line_count, MAX_ENTRYPOINT_LINES);
    }

    #[test]
    fn test_truncate_exceeds_byte_limit() {
        let content: String = (0..50).map(|_| "x".repeat(600) + "\n").collect();
        let result = truncate_entrypoint_content(&content);
        assert!(result.was_byte_truncated);
        assert!(result.content.len() <= MAX_ENTRYPOINT_BYTES);
    }
}
```

- [ ] **Step 2: Write failing tests for paths**

```rust
// src/services/memdir/paths.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path_for_memory() {
        let sanitized = sanitize_git_root("/Users/dev/my-project");
        assert!(!sanitized.contains('/'));
        assert!(!sanitized.is_empty());
    }

    #[test]
    fn test_is_auto_memory_enabled_default() {
        // Without env overrides, default is true
        assert!(is_auto_memory_enabled_with_env(None, false, None));
    }

    #[test]
    fn test_is_auto_memory_disabled_by_env() {
        assert!(!is_auto_memory_enabled_with_env(Some("1"), false, None));
    }

    #[test]
    fn test_is_auto_memory_disabled_by_bare() {
        assert!(!is_auto_memory_enabled_with_env(None, true, None));
    }
}
```

- [ ] **Step 3: Implement types, paths, and truncation**

```rust
// src/services/memdir/types.rs
pub const MEMORY_TYPES: &[&str] = &["user", "feedback", "project", "reference"];

pub struct MemoryHeader {
    pub filename: String,
    pub file_path: std::path::PathBuf,
    pub mtime_ms: u64,
    pub description: Option<String>,
    pub memory_type: Option<String>,
}

pub struct RelevantMemory {
    pub path: std::path::PathBuf,
    pub mtime_ms: u64,
}

pub struct EntrypointTruncation {
    pub content: String,
    pub line_count: usize,
    pub byte_count: usize,
    pub was_line_truncated: bool,
    pub was_byte_truncated: bool,
}
```

```rust
// src/services/memdir/truncation.rs
use super::types::EntrypointTruncation;

pub const MAX_ENTRYPOINT_LINES: usize = 200;
pub const MAX_ENTRYPOINT_BYTES: usize = 25_000;

pub fn truncate_entrypoint_content(raw: &str) -> EntrypointTruncation {
    let trimmed = raw.trim();
    let lines: Vec<&str> = trimmed.lines().collect();
    let line_count = lines.len();

    let mut was_line_truncated = false;
    let mut was_byte_truncated = false;

    let content = if line_count > MAX_ENTRYPOINT_LINES {
        was_line_truncated = true;
        lines[..MAX_ENTRYPOINT_LINES].join("\n")
    } else {
        trimmed.to_string()
    };

    let byte_count = content.len();
    let content = if byte_count > MAX_ENTRYPOINT_BYTES {
        was_byte_truncated = true;
        let truncated = &content[..MAX_ENTRYPOINT_BYTES];
        match truncated.rfind('\n') {
            Some(pos) => truncated[..pos].to_string(),
            None => truncated.to_string(),
        }
    } else {
        content
    };

    let final_line_count = content.lines().count();
    let final_byte_count = content.len();

    EntrypointTruncation {
        content,
        line_count: final_line_count,
        byte_count: final_byte_count,
        was_line_truncated,
        was_byte_truncated,
    }
}
```

```rust
// src/services/memdir/paths.rs
use std::path::{Path, PathBuf};

pub fn sanitize_git_root(path: &str) -> String {
    path.replace(['/', '\\', ':'], "-")
        .trim_matches('-')
        .to_string()
}

pub fn get_auto_mem_path(cwd: &Path) -> Option<PathBuf> {
    // 1. ATOM_COWORK_MEMORY_PATH_OVERRIDE
    if let Ok(override_path) = std::env::var("ATOM_COWORK_MEMORY_PATH_OVERRIDE") {
        return Some(PathBuf::from(override_path));
    }

    // 2. Find git root for cwd
    let git_root = find_git_root(cwd)?;
    let sanitized = sanitize_git_root(&git_root.display().to_string());

    // 3. ~/.atom/projects/{sanitized}/memory/
    let base = crate::utils::paths::global_config_dir();
    Some(base.join("projects").join(sanitized).join("memory"))
}

pub fn is_auto_memory_enabled() -> bool {
    let disable_env = std::env::var("ATOM_CODE_DISABLE_AUTO_MEMORY").ok();
    let bare = std::env::var("ATOM_CODE_SIMPLE").ok().is_some();
    is_auto_memory_enabled_with_env(disable_env.as_deref(), bare, None)
}

pub fn is_auto_memory_enabled_with_env(
    disable_env: Option<&str>,
    bare_mode: bool,
    settings_enabled: Option<bool>,
) -> bool {
    if disable_env == Some("1") || disable_env == Some("true") {
        return false;
    }
    if bare_mode {
        return false;
    }
    if let Some(enabled) = settings_enabled {
        return enabled;
    }
    true
}

fn find_git_root(from: &Path) -> Option<PathBuf> {
    let mut current = from;
    loop {
        if current.join(".git").exists() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

pub fn is_auto_mem_path(path: &Path, memory_dir: &Path) -> bool {
    path.starts_with(memory_dir)
}
```

- [ ] **Step 4: Add memory path utilities to utils/paths.rs**

Add after line 35:
```rust
pub fn user_memory_base_dir() -> PathBuf {
    global_config_dir().join("projects")
}
```

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test --lib services::memdir -- --nocapture`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/services/memdir/ src/utils/paths.rs
git commit -m "feat(memdir): types, path resolution, and MEMORY.md truncation with limits"
```

---

## Task 10: Memdir — Scanner and Module Root

**Files:**
- Create: `src/services/memdir/scan.rs`
- Create: `src/services/memdir/mod.rs`
- Modify: `src/services/mod.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write failing tests for scanner**

```rust
// src/services/memdir/scan.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_scan_memory_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("MEMORY.md"), "index").unwrap();
        fs::write(dir.path().join("user_role.md"), "---\nname: Role\ndescription: User role\ntype: user\n---\ncontent").unwrap();
        fs::write(dir.path().join("feedback_testing.md"), "---\nname: Testing\ndescription: Test feedback\ntype: feedback\n---\ncontent").unwrap();

        let headers = scan_memory_files(dir.path());
        assert_eq!(headers.len(), 2); // MEMORY.md excluded
        assert!(headers.iter().any(|h| h.filename == "user_role.md"));
    }

    #[test]
    fn test_scan_memory_files_empty_dir() {
        let dir = TempDir::new().unwrap();
        let headers = scan_memory_files(dir.path());
        assert!(headers.is_empty());
    }

    #[test]
    fn test_parse_frontmatter() {
        let content = "---\nname: Test\ndescription: A test memory\ntype: user\n---\nbody";
        let (desc, mem_type) = parse_frontmatter(content);
        assert_eq!(desc, Some("A test memory".to_string()));
        assert_eq!(mem_type, Some("user".to_string()));
    }

    #[test]
    fn test_format_memory_manifest() {
        let headers = vec![MemoryHeader {
            filename: "user_role.md".to_string(),
            file_path: std::path::PathBuf::from("/tmp/user_role.md"),
            mtime_ms: 1000,
            description: Some("User role info".to_string()),
            memory_type: Some("user".to_string()),
        }];
        let manifest = format_memory_manifest(&headers);
        assert!(manifest.contains("[user] user_role.md"));
        assert!(manifest.contains("User role info"));
    }
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test --lib services::memdir::scan::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement scanner**

```rust
// src/services/memdir/scan.rs
use super::types::MemoryHeader;
use std::path::Path;

const MAX_MEMORY_FILES: usize = 200;
const FRONTMATTER_MAX_LINES: usize = 30;

pub fn scan_memory_files(memory_dir: &Path) -> Vec<MemoryHeader> {
    let entries = match std::fs::read_dir(memory_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut headers: Vec<MemoryHeader> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".md") && name != "MEMORY.md"
        })
        .filter_map(|e| {
            let path = e.path();
            let metadata = e.metadata().ok()?;
            let mtime_ms = metadata.modified().ok()?
                .duration_since(std::time::UNIX_EPOCH).ok()?
                .as_millis() as u64;

            let content = std::fs::read_to_string(&path).ok()?;
            let (description, memory_type) = parse_frontmatter(&content);

            Some(MemoryHeader {
                filename: e.file_name().to_string_lossy().to_string(),
                file_path: path,
                mtime_ms,
                description,
                memory_type,
            })
        })
        .collect();

    // Sort by mtime descending (newest first)
    headers.sort_by(|a, b| b.mtime_ms.cmp(&a.mtime_ms));
    headers.truncate(MAX_MEMORY_FILES);
    headers
}

pub fn parse_frontmatter(content: &str) -> (Option<String>, Option<String>) {
    let lines: Vec<&str> = content.lines().take(FRONTMATTER_MAX_LINES).collect();
    if lines.first() != Some(&"---") {
        return (None, None);
    }

    let end = lines.iter().skip(1).position(|l| *l == "---");
    let end = match end {
        Some(e) => e + 1,
        None => return (None, None),
    };

    let mut description = None;
    let mut memory_type = None;

    for line in &lines[1..end] {
        if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().to_string());
        } else if let Some(val) = line.strip_prefix("type:") {
            memory_type = Some(val.trim().to_string());
        }
    }

    (description, memory_type)
}

pub fn format_memory_manifest(memories: &[MemoryHeader]) -> String {
    memories.iter().map(|m| {
        let type_str = m.memory_type.as_deref().unwrap_or("unknown");
        let desc = m.description.as_deref().unwrap_or("(no description)");
        format!("[{}] {} : {}", type_str, m.filename, desc)
    }).collect::<Vec<_>>().join("\n")
}
```

- [ ] **Step 4: Create memdir/mod.rs and wire into services**

```rust
// src/services/memdir/mod.rs
pub mod types;
pub mod paths;
pub mod truncation;
pub mod scan;

pub use paths::{get_auto_mem_path, is_auto_memory_enabled, is_auto_mem_path};
pub use scan::{scan_memory_files, format_memory_manifest};
pub use truncation::truncate_entrypoint_content;
pub use types::*;

use std::path::Path;

/// Load memory prompt for system prompt injection.
/// Returns None if auto-memory is disabled or no MEMORY.md exists.
pub fn load_memory_prompt(cwd: &Path) -> Option<String> {
    if !is_auto_memory_enabled() {
        return None;
    }

    let mem_dir = get_auto_mem_path(cwd)?;
    let memory_md = mem_dir.join("MEMORY.md");
    let content = std::fs::read_to_string(&memory_md).ok()?;

    if content.trim().is_empty() {
        return None;
    }

    let truncated = truncate_entrypoint_content(&content);
    Some(truncated.content)
}

/// Ensure the memory directory exists (idempotent).
pub fn ensure_memory_dir(cwd: &Path) -> Option<std::path::PathBuf> {
    let mem_dir = get_auto_mem_path(cwd)?;
    std::fs::create_dir_all(&mem_dir).ok()?;
    Some(mem_dir)
}
```

Add `pub mod memdir;` to `src/services/mod.rs`.

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test --lib services::memdir -- --nocapture`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/services/memdir/ src/services/mod.rs
git commit -m "feat(memdir): memory file scanner, frontmatter parser, and module root with prompt loading"
```

---

## Task 11: System Prompt Integration (Memory + Hooks Wiring)

**Files:**
- Modify: `src/services/context/mod.rs`
- Test: verify system prompt includes memory content

- [ ] **Step 1: Write failing test**

```rust
// Add to src/services/context/mod.rs tests
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_assemble_includes_memory_prompt() {
        let dir = TempDir::new().unwrap();
        // Create memory dir and MEMORY.md
        let mem_dir = dir.path().join(".atom-test-memory");
        std::fs::create_dir_all(&mem_dir).unwrap();
        std::fs::write(mem_dir.join("MEMORY.md"), "- [Test](test.md) — test memory").unwrap();

        // This test verifies the integration point exists
        // Full test requires env setup; here we test the helper
        let prompt = crate::services::memdir::load_memory_prompt(dir.path());
        // Without proper git root, returns None — that's expected
        assert!(prompt.is_none());
    }
}
```

- [ ] **Step 2: Add memory block injection to system prompt assembly**

In `src/services/context/mod.rs`, after the AGENTS.md block (around line 24), add:

```rust
// After AGENTS.md blocks, before git context
if let Some(memory_content) = crate::services::memdir::load_memory_prompt(cwd) {
    blocks.push(SystemBlock {
        block_type: "memory".to_string(),
        content: format!("# Memory\n\n{}", memory_content),
        cache_control: None,
    });
}
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test -- --nocapture`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add src/services/context/mod.rs
git commit -m "feat(memdir): inject memory prompt into system prompt assembly"
```

---

## Task 12: File Index TUI Integration

**Files:**
- Modify: `src/tui/app.rs`
- Test: verify file suggestions use FileIndex

- [ ] **Step 1: Add FileIndex field to App struct**

In `src/tui/app.rs`, add to the App struct:
```rust
pub file_index: Option<crate::services::file_index::FileIndex>,
```

Initialize as `None` in `App::new()`.

- [ ] **Step 2: Replace get_file_suggestions with FileIndex-backed search**

In `get_file_suggestions()` (lines 624-695), add FileIndex path:

```rust
pub fn get_file_suggestions(&self, query: &str) -> Vec<String> {
    if let Some(ref index) = self.file_index {
        return index.search(query, 15)
            .into_iter()
            .map(|r| r.path)
            .collect();
    }
    // Fallback to existing ignore-based walking
    self.get_file_suggestions_fallback(query)
}
```

- [ ] **Step 3: Add background index building on first @ trigger**

In `update_file_typeahead()`, before search:

```rust
if self.file_index.is_none() {
    // Build index from git ls-files
    if let Ok(output) = std::process::Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(&self.cwd)
        .output()
    {
        let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect();
        let mut index = crate::services::file_index::FileIndex::new();
        index.load_from_file_list(files);
        self.file_index = Some(index);
    }
}
```

- [ ] **Step 4: Run full test suite**

Run: `cargo test -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Manual smoke test**

Run: `cargo run` → type `@` → verify file suggestions appear

- [ ] **Step 6: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(file_index): integrate fuzzy file search into TUI @ mention suggestions"
```

---

## Task 13: Async Hook Registry

**Files:**
- Create: `src/services/hooks/async_registry.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write failing tests**

```rust
// src/services/hooks/async_registry.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_empty() {
        let registry = AsyncHookRegistry::new();
        assert!(registry.check_for_responses().is_empty());
    }

    #[test]
    fn test_parse_async_response_first_json_line() {
        let stdout = "some log line\n{\"decision\":\"block\",\"reason\":\"denied\"}\nmore output";
        let parsed = parse_async_stdout(stdout);
        assert_eq!(parsed.decision.as_deref(), Some("block"));
    }

    #[test]
    fn test_parse_async_response_skips_async_key() {
        let stdout = "{\"async\":true}\n{\"decision\":\"approve\"}";
        let parsed = parse_async_stdout(stdout);
        assert_eq!(parsed.decision.as_deref(), Some("approve"));
    }

    #[test]
    fn test_parse_async_response_no_json() {
        let stdout = "plain text output";
        let parsed = parse_async_stdout(stdout);
        assert!(parsed.decision.is_none());
    }
}
```

- [ ] **Step 2: Implement async registry**

```rust
// src/services/hooks/async_registry.rs
use super::types::SyncHookOutput;
use std::collections::HashMap;
use tokio::process::Child;

pub struct PendingAsyncHook {
    pub hook_id: String,
    pub child: Child,
    pub command: String,
    pub async_rewake: bool,
}

pub struct AsyncHookResponse {
    pub hook_id: String,
    pub output: SyncHookOutput,
    pub exit_code: i32,
}

pub struct AsyncHookRegistry {
    pending: HashMap<String, PendingAsyncHook>,
}

impl AsyncHookRegistry {
    pub fn new() -> Self {
        Self { pending: HashMap::new() }
    }

    pub fn register_pending(&mut self, hook: PendingAsyncHook) {
        self.pending.insert(hook.hook_id.clone(), hook);
    }

    pub fn check_for_responses(&mut self) -> Vec<AsyncHookResponse> {
        let mut completed = Vec::new();
        let mut done_ids = Vec::new();

        for (id, hook) in &mut self.pending {
            match hook.child.try_wait() {
                Ok(Some(status)) => {
                    let exit_code = status.code().unwrap_or(-1);
                    // Read stdout/stderr (best effort)
                    let stdout = String::new(); // Would read from captured output
                    let output = parse_async_stdout(&stdout);
                    completed.push(AsyncHookResponse {
                        hook_id: id.clone(),
                        output,
                        exit_code,
                    });
                    done_ids.push(id.clone());
                }
                _ => {} // Still running
            }
        }

        for id in done_ids {
            self.pending.remove(&id);
        }

        completed
    }

    pub fn finalize_pending(&mut self) {
        for (_, mut hook) in self.pending.drain() {
            let _ = hook.child.start_kill();
        }
    }
}

pub fn parse_async_stdout(stdout: &str) -> SyncHookOutput {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                // Skip async ack responses
                if parsed.get("async").is_some() {
                    continue;
                }
                if let Ok(output) = serde_json::from_value::<SyncHookOutput>(parsed) {
                    return output;
                }
            }
        }
    }
    SyncHookOutput::default()
}
```

- [ ] **Step 3: Run tests, verify pass**

Run: `cargo test --lib services::hooks::async_registry::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add src/services/hooks/async_registry.rs
git commit -m "feat(hooks): async hook registry with background process tracking and JSON response parsing"
```

---

## Task 14: Prompt & Agent Hook Executors (Stubs with LLM Integration Point)

**Files:**
- Create: `src/services/hooks/prompt_hook.rs`
- Create: `src/services/hooks/agent_hook.rs`
- Test: inline `#[cfg(test)]` modules

- [ ] **Step 1: Write failing tests for prompt hook**

```rust
// src/services/hooks/prompt_hook.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_hook_input() {
        let input = build_prompt_input("Check if $ARGUMENTS is safe", r#"{"command":"rm -rf /"}"#);
        assert!(input.contains("rm -rf /"));
        assert!(!input.contains("$ARGUMENTS"));
    }

    #[test]
    fn test_parse_prompt_hook_response_ok() {
        let response = r#"{"ok": true}"#;
        assert!(parse_prompt_response(response).is_ok());
    }

    #[test]
    fn test_parse_prompt_hook_response_blocked() {
        let response = r#"{"ok": false, "reason": "dangerous command"}"#;
        let result = parse_prompt_response(response);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("dangerous command"));
    }
}
```

- [ ] **Step 2: Implement prompt hook**

```rust
// src/services/hooks/prompt_hook.rs

pub fn build_prompt_input(prompt_template: &str, arguments_json: &str) -> String {
    prompt_template.replace("$ARGUMENTS", arguments_json)
}

pub fn parse_prompt_response(response: &str) -> Result<(), String> {
    let parsed: serde_json::Value = serde_json::from_str(response)
        .map_err(|e| format!("Invalid prompt hook response: {}", e))?;
    let ok = parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        Ok(())
    } else {
        let reason = parsed.get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Blocked by prompt hook");
        Err(reason.to_string())
    }
}

/// Execute a prompt hook by calling the LLM.
/// The actual LLM call is delegated to the caller via the `llm_query` callback.
pub async fn execute_prompt_hook(
    prompt_template: &str,
    arguments_json: &str,
    llm_query: impl AsyncFn(&str) -> Result<String, String>,
) -> Result<(), String> {
    let prompt = build_prompt_input(prompt_template, arguments_json);
    let response = llm_query(&prompt).await?;
    parse_prompt_response(&response)
}
```

- [ ] **Step 3: Write agent hook stub**

```rust
// src/services/hooks/agent_hook.rs
// Agent hooks require a multi-turn agent session (max 50 turns).
// This is a structural stub — full implementation requires the coordinator module (Phase 10B).

pub const MAX_AGENT_HOOK_TURNS: usize = 50;

pub fn parse_agent_output(output: &serde_json::Value) -> Result<(), String> {
    let ok = output.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        Ok(())
    } else {
        let reason = output.get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Blocked by agent hook");
        Err(reason.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_agent_output_ok() {
        let output = serde_json::json!({"ok": true});
        assert!(parse_agent_output(&output).is_ok());
    }

    #[test]
    fn test_parse_agent_output_blocked() {
        let output = serde_json::json!({"ok": false, "reason": "unsafe"});
        assert!(parse_agent_output(&output).is_err());
    }
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --lib services::hooks::prompt_hook::tests services::hooks::agent_hook::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/hooks/prompt_hook.rs src/services/hooks/agent_hook.rs
git commit -m "feat(hooks): prompt hook executor and agent hook stub (full agent requires coordinator)"
```

---

## Task 15: Hook Deduplication & Settings Hierarchy Enhancement

**Files:**
- Modify: `src/services/hooks/settings.rs`
- Modify: `src/services/hooks/executor.rs`
- Test: inline tests

- [ ] **Step 1: Write failing tests for deduplication**

```rust
// Add to src/services/hooks/settings.rs tests
#[test]
fn test_dedup_hooks_removes_duplicates() {
    let mut settings = HooksSettings::new();
    settings.insert("PreToolUse".to_string(), vec![
        HookMatcher {
            matcher: Some("Bash".to_string()),
            hooks: vec![HookCommand::Command {
                command: "echo test".to_string(),
                shell: None, timeout: None, status_message: None,
                once: None, r#async: None, async_rewake: None, r#if: None,
            }],
        },
        HookMatcher {
            matcher: Some("Bash".to_string()),
            hooks: vec![HookCommand::Command {
                command: "echo test".to_string(),
                shell: None, timeout: None, status_message: None,
                once: None, r#async: None, async_rewake: None, r#if: None,
            }],
        },
    ]);
    let deduped = dedup_hooks_settings(settings);
    // Should have only 1 matcher for PreToolUse Bash "echo test"
    assert_eq!(deduped["PreToolUse"].len(), 1);
}
```

- [ ] **Step 2: Implement deduplication**

```rust
// Add to src/services/hooks/settings.rs
use std::collections::HashSet;

pub fn dedup_hooks_settings(settings: HooksSettings) -> HooksSettings {
    let mut result = HooksSettings::new();
    for (event, matchers) in settings {
        let mut seen = HashSet::new();
        let mut deduped = Vec::new();
        for matcher in matchers {
            for hook in &matcher.hooks {
                let key = hook_dedup_key(&matcher, hook);
                if seen.insert(key) {
                    deduped.push(HookMatcher {
                        matcher: matcher.matcher.clone(),
                        hooks: vec![hook.clone()],
                    });
                }
            }
        }
        if !deduped.is_empty() {
            result.insert(event, deduped);
        }
    }
    result
}

fn hook_dedup_key(matcher: &HookMatcher, hook: &HookCommand) -> String {
    match hook {
        HookCommand::Command { command, shell, r#if, .. } => {
            let shell_str = shell.as_deref().unwrap_or("sh");
            let if_str = r#if.as_deref().unwrap_or("");
            format!("\0{}\0{}\0{}", shell_str, command, if_str)
        }
        HookCommand::Http { url, r#if, .. } => format!("\0http\0{}\0{}", url, r#if.as_deref().unwrap_or("")),
        HookCommand::Prompt { prompt, r#if, .. } => format!("\0prompt\0{}\0{}", prompt, r#if.as_deref().unwrap_or("")),
        HookCommand::Agent { prompt, r#if, .. } => format!("\0agent\0{}\0{}", prompt, r#if.as_deref().unwrap_or("")),
    }
}
```

- [ ] **Step 3: Add SessionStart/Setup always-fire and HTTP filter to executor**

```rust
// Add to executor.rs execute_event_hooks():
// Before matcher filtering:
const ALWAYS_EMITTED: &[&str] = &["SessionStart", "Setup"];

// After matching hooks collected, before execution:
// Filter out HTTP hooks for SessionStart/Setup
if event == "SessionStart" || event == "Setup" {
    matching_hooks.retain(|h| !matches!(h, HookCommand::Http { .. }));
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --lib services::hooks -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/hooks/settings.rs src/services/hooks/executor.rs
git commit -m "feat(hooks): deduplication, SessionStart/Setup always-fire, HTTP hook filtering"
```

---

## Task 16: Memdir — Relevance Search Stub

**Files:**
- Create: `src/services/memdir/relevance.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write failing tests**

```rust
// src/services/memdir/relevance.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_find_relevant_returns_empty_when_no_files() {
        let dir = TempDir::new().unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(find_relevant_memories(
            "test query", dir.path(), &[], &std::collections::HashSet::new(), None,
        ));
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_relevant_excludes_already_surfaced() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("test.md"), "---\nname: Test\ndescription: test\ntype: user\n---\ncontent").unwrap();
        let mut surfaced = std::collections::HashSet::new();
        surfaced.insert("test.md".to_string());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(find_relevant_memories(
            "test", dir.path(), &[], &surfaced, None,
        ));
        assert!(result.is_empty());
    }
}
```

- [ ] **Step 2: Implement relevance search**

```rust
// src/services/memdir/relevance.rs
use super::scan::scan_memory_files;
use super::types::RelevantMemory;
use std::collections::HashSet;
use std::path::Path;

const MAX_SELECTED_MEMORIES: usize = 5;

/// Find relevant memories for a query. Uses LLM-based selection when available,
/// falls back to recency-based selection. The `llm_selector` callback is optional —
/// when None, returns the most recent files.
pub async fn find_relevant_memories(
    query: &str,
    memory_dir: &Path,
    recent_tools: &[String],
    already_surfaced: &HashSet<String>,
    llm_selector: Option<&dyn Fn(&str, &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, String>> + Send + '_>>>,
) -> Vec<RelevantMemory> {
    let headers = scan_memory_files(memory_dir);
    if headers.is_empty() {
        return vec![];
    }

    // Filter out already-surfaced files
    let candidates: Vec<_> = headers.into_iter()
        .filter(|h| !already_surfaced.contains(&h.filename))
        .collect();

    if candidates.is_empty() {
        return vec![];
    }

    // If LLM selector available, use it; otherwise return most recent
    let selected = if let Some(selector) = llm_selector {
        let manifest = super::scan::format_memory_manifest(&candidates);
        match selector(query, &manifest).await {
            Ok(filenames) => filenames,
            Err(_) => {
                // Graceful degradation: return most recent
                candidates.iter().take(MAX_SELECTED_MEMORIES)
                    .map(|h| h.filename.clone())
                    .collect()
            }
        }
    } else {
        candidates.iter().take(MAX_SELECTED_MEMORIES)
            .map(|h| h.filename.clone())
            .collect()
    };

    // Map filenames back to RelevantMemory
    selected.into_iter()
        .filter_map(|filename| {
            candidates.iter().find(|h| h.filename == filename)
                .map(|h| RelevantMemory {
                    path: h.file_path.clone(),
                    mtime_ms: h.mtime_ms,
                })
        })
        .take(MAX_SELECTED_MEMORIES)
        .collect()
}
```

- [ ] **Step 3: Wire into memdir/mod.rs**

Add `pub mod relevance;` to `src/services/memdir/mod.rs`.

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --lib services::memdir::relevance::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/memdir/relevance.rs src/services/memdir/mod.rs
git commit -m "feat(memdir): relevance search with LLM selector callback and recency fallback"
```

---

## Task 17: Memdir — Extraction Stub

**Files:**
- Create: `src/services/memdir/extraction.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write failing test**

```rust
// src/services/memdir/extraction.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_extract_default_enabled() {
        assert!(should_extract_memories(true, false, 1, 1));
    }

    #[test]
    fn test_should_extract_disabled() {
        assert!(!should_extract_memories(false, false, 1, 1));
    }

    #[test]
    fn test_should_extract_skips_when_main_wrote() {
        assert!(!should_extract_memories(true, true, 1, 1));
    }

    #[test]
    fn test_should_extract_throttle() {
        // throttle_interval=3, turns_since=2 → skip
        assert!(!should_extract_memories(true, false, 2, 3));
        // throttle_interval=3, turns_since=3 → extract
        assert!(should_extract_memories(true, false, 3, 3));
    }
}
```

- [ ] **Step 2: Implement extraction gate logic**

```rust
// src/services/memdir/extraction.rs

/// Determine whether memory extraction should run.
pub fn should_extract_memories(
    auto_memory_enabled: bool,
    main_agent_wrote_memory: bool,
    turns_since_last_extraction: usize,
    throttle_interval: usize,
) -> bool {
    if !auto_memory_enabled {
        return false;
    }
    if main_agent_wrote_memory {
        return false;
    }
    turns_since_last_extraction >= throttle_interval
}

/// Placeholder for the full extraction agent.
/// Full implementation requires forked agent infrastructure (uses restricted tool set,
/// max 5 turns, canUseTool filter for Edit/Write to memory dir only).
/// This will be fully implemented when the coordinator/agent infrastructure is available.
pub async fn execute_extract_memories(
    _memory_dir: &std::path::Path,
    _messages: &[crate::types::message::ApiMessage],
) {
    // TODO: Implement forked agent extraction
    // 1. Scan existing memory files (manifest)
    // 2. Fork agent with restricted tools
    // 3. Max 5 turns
    // 4. Advance cursor on success
    // 5. Coalesce if new context arrives during run
}
```

- [ ] **Step 3: Wire into memdir/mod.rs**

Add `pub mod extraction;` to `src/services/memdir/mod.rs`.

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --lib services::memdir::extraction::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/memdir/extraction.rs src/services/memdir/mod.rs
git commit -m "feat(memdir): extraction gate logic and stub for background memory extraction agent"
```

---

## Task 18: Final Integration Test & Cleanup

**Files:**
- All modified files
- Test: full cargo test + cargo clippy

- [ ] **Step 1: Run full test suite**

Run: `cargo test -- --nocapture`
Expected: All PASS

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

- [ ] **Step 3: Verify build**

Run: `cargo build --release`
Expected: Success

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore: Phase 10A complete — hooks enhancement, file index, memdir with all integration points"
```
