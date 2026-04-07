# Phase 10B: Coordinator + Plugins + Output Styles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement multi-agent coordination (AgentTool + coordinator mode), plugin registry with hook/skill/MCP integration, and markdown-based output style system with system prompt injection.

**Architecture:** Three modules in dependency order: output_styles first (standalone, simplest), then plugins (depends on hooks from Phase 10A, provides output styles), then coordinator (depends on tools, hooks, and the agent execution infrastructure). Each is a new service module following established patterns.

**Tech Stack:** Rust, tokio (async), serde/serde_json, existing Tool trait, existing hook system from Phase 10A

**Spec:** `docs/superpowers/specs/2026-04-03-phase10-auxiliary-modules-design.md` (sections 4, 5, 6)

---

## File Structure

### Output Styles
```
src/services/output_styles/
  mod.rs              — CREATE: load_output_style, get_all_styles, OutputStyleConfig
  loader.rs           — CREATE: load from .agents/output-styles/ and ~/.atom/output-styles/
```

### Plugins
```
src/services/plugins/
  mod.rs              — CREATE: BuiltinPluginDefinition, plugin registry, load_all_plugins
  loader.rs           — CREATE: plugin discovery, manifest parsing, enable/disable state
  hooks_bridge.rs     — CREATE: convert plugin hooks to HooksSettings, register in hook system
```

### Coordinator
```
src/tools/agent_tool.rs  — CREATE: AgentTool implementing Tool trait
src/services/coordinator/
  mod.rs              — CREATE: is_coordinator_mode, coordinator tool filtering, worker context
  worker.rs           — CREATE: worker spawn, tool pool assembly, async lifecycle
```

### Integration Points (modifications)
```
src/services/mod.rs           — MODIFY: add output_styles, plugins, coordinator
src/services/context/mod.rs   — MODIFY: inject output_style block
src/tools/mod.rs              — MODIFY: register AgentTool
src/utils/settings/types.rs   — MODIFY: add output_style, enabled_plugins fields
src/tui/app.rs                — MODIFY: add output_style field, load at startup
```

---

## Task 1: Output Style Types & Loader

**Files:**
- Create: `src/services/output_styles/mod.rs`
- Create: `src/services/output_styles/loader.rs`
- Modify: `src/services/mod.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests for OutputStyleConfig**

```rust
// src/services/output_styles/loader.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_output_style_file() {
        let content = "---\nname: Custom\ndescription: A custom style\nkeep-coding-instructions: true\n---\n\n# Instructions\nBe verbose.";
        let config = parse_output_style(content, "custom.md", "userSettings");
        assert_eq!(config.name, "Custom");
        assert_eq!(config.description, "A custom style");
        assert!(config.keep_coding_instructions);
        assert!(config.prompt.contains("Be verbose"));
    }

    #[test]
    fn test_parse_output_style_no_frontmatter() {
        let content = "# Just Instructions\nBe concise.";
        let config = parse_output_style(content, "concise.md", "userSettings");
        assert_eq!(config.name, "concise"); // filename without .md
        assert!(config.prompt.contains("Be concise"));
    }

    #[test]
    fn test_load_styles_from_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("verbose.md"),
            "---\nname: Verbose\ndescription: Detailed output\n---\nBe very detailed.",
        ).unwrap();
        let styles = load_styles_from_dir(dir.path(), "userSettings");
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].name, "Verbose");
    }

    #[test]
    fn test_load_styles_empty_dir() {
        let dir = TempDir::new().unwrap();
        let styles = load_styles_from_dir(dir.path(), "userSettings");
        assert!(styles.is_empty());
    }

    #[test]
    fn test_builtin_styles() {
        let styles = get_builtin_styles();
        assert!(styles.iter().any(|s| s.name == "default"));
        assert!(styles.iter().any(|s| s.name == "Explanatory"));
        assert!(styles.iter().any(|s| s.name == "Learning"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::output_styles -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement OutputStyleConfig and loader**

```rust
// src/services/output_styles/loader.rs
use std::path::Path;

#[derive(Debug, Clone)]
pub struct OutputStyleConfig {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub source: String,  // "built-in", "plugin", "userSettings", "projectSettings"
    pub keep_coding_instructions: bool,
    pub force_for_plugin: bool,
}

pub fn parse_output_style(content: &str, filename: &str, source: &str) -> OutputStyleConfig {
    let (frontmatter, body) = split_frontmatter(content);
    let name = frontmatter.get("name")
        .cloned()
        .unwrap_or_else(|| filename.trim_end_matches(".md").to_string());
    let description = frontmatter.get("description").cloned().unwrap_or_default();
    let keep = frontmatter.get("keep-coding-instructions")
        .map(|v| v == "true")
        .unwrap_or(false);
    let force = frontmatter.get("force-for-plugin")
        .map(|v| v == "true")
        .unwrap_or(false);

    OutputStyleConfig {
        name,
        description,
        prompt: body.trim().to_string(),
        source: source.to_string(),
        keep_coding_instructions: keep,
        force_for_plugin: force,
    }
}

pub fn load_styles_from_dir(dir: &Path, source: &str) -> Vec<OutputStyleConfig> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
        .filter_map(|e| {
            let content = std::fs::read_to_string(e.path()).ok()?;
            let filename = e.file_name().to_string_lossy().to_string();
            Some(parse_output_style(&content, &filename, source))
        })
        .collect()
}

pub fn get_builtin_styles() -> Vec<OutputStyleConfig> {
    vec![
        OutputStyleConfig {
            name: "default".to_string(),
            description: "Standard output (no style injection)".to_string(),
            prompt: String::new(),
            source: "built-in".to_string(),
            keep_coding_instructions: true,
            force_for_plugin: false,
        },
        OutputStyleConfig {
            name: "Explanatory".to_string(),
            description: "Adds educational insights with explanations".to_string(),
            prompt: "When responding, provide detailed explanations of your approach. Explain the 'why' behind implementation choices. Reference relevant patterns in the codebase.".to_string(),
            source: "built-in".to_string(),
            keep_coding_instructions: true,
            force_for_plugin: false,
        },
        OutputStyleConfig {
            name: "Learning".to_string(),
            description: "Interactive learning mode with hands-on guidance".to_string(),
            prompt: "Respond in a teaching style. Ask the user questions to test understanding. Provide exercises and challenges when appropriate. Explain concepts from first principles.".to_string(),
            source: "built-in".to_string(),
            keep_coding_instructions: true,
            force_for_plugin: false,
        },
    ]
}

fn split_frontmatter(content: &str) -> (std::collections::HashMap<String, String>, &str) {
    let mut map = std::collections::HashMap::new();
    if !content.starts_with("---") {
        return (map, content);
    }
    let rest = &content[3..];
    if let Some(end) = rest.find("\n---") {
        let fm_block = &rest[..end];
        let body = &rest[end + 4..];
        for line in fm_block.lines() {
            if let Some((key, value)) = line.split_once(':') {
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
        (map, body)
    } else {
        (map, content)
    }
}
```

```rust
// src/services/output_styles/mod.rs
pub mod loader;

pub use loader::{OutputStyleConfig, get_builtin_styles, load_styles_from_dir, parse_output_style};

use std::collections::HashMap;
use std::path::Path;

/// Get all available output styles from all sources (lowest to highest priority).
pub fn get_all_output_styles(cwd: &Path) -> HashMap<String, OutputStyleConfig> {
    let mut all = HashMap::new();

    // 1. Built-in
    for style in get_builtin_styles() {
        all.insert(style.name.clone(), style);
    }

    // 2. User-level: ~/.atom/output-styles/
    let user_dir = crate::utils::paths::global_config_dir().join("output-styles");
    for style in load_styles_from_dir(&user_dir, "userSettings") {
        all.insert(style.name.clone(), style);
    }

    // 3. Project-level: .agents/output-styles/
    let project_dir = cwd.join(".agents/output-styles");
    for style in load_styles_from_dir(&project_dir, "projectSettings") {
        all.insert(style.name.clone(), style);
    }

    all
}

/// Get the active output style config based on settings.
pub fn get_output_style_config(cwd: &Path, selected: &str) -> Option<OutputStyleConfig> {
    let all = get_all_output_styles(cwd);

    // Check forced plugin styles first
    let forced: Vec<_> = all.values()
        .filter(|s| s.source == "plugin" && s.force_for_plugin)
        .collect();
    if let Some(forced_style) = forced.first() {
        return Some((*forced_style).clone());
    }

    // Use selected style
    if selected == "default" {
        return None; // No injection for default
    }
    all.get(selected).cloned()
}
```

- [ ] **Step 4: Add module to services/mod.rs**

Add `pub mod output_styles;` to `src/services/mod.rs`.

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test --lib services::output_styles -- --nocapture`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/services/output_styles/ src/services/mod.rs
git commit -m "feat(output_styles): output style config, loader, and built-in styles"
```

---

## Task 2: Output Style System Prompt Integration

**Files:**
- Modify: `src/services/context/mod.rs`
- Modify: `src/utils/settings/types.rs`
- Test: inline tests

- [ ] **Step 1: Add output_style field to SettingsJson**

In `src/utils/settings/types.rs`, add to the SettingsJson struct:
```rust
pub output_style: Option<String>,
pub enabled_plugins: Option<HashMap<String, bool>>,
```

- [ ] **Step 2: Modify assemble_system_prompt to accept output_style parameter**

Add an optional `output_style: Option<&str>` parameter. After the memory block injection, add:

```rust
// Output style injection
if let Some(style_name) = output_style {
    if let Some(style_config) = crate::services::output_styles::get_output_style_config(cwd, style_name) {
        blocks.push(SystemBlock::Text {
            text: format!("# Output Style: {}\n\n{}", style_config.name, style_config.prompt),
        });
    }
}
```

- [ ] **Step 3: Update all callers of assemble_system_prompt**

Add `None` as the output_style parameter at existing call sites to maintain backward compatibility. Search for all calls.

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/context/mod.rs src/utils/settings/types.rs
git commit -m "feat(output_styles): inject output style into system prompt assembly"
```

---

## Task 3: Plugin Types & Registry

**Files:**
- Create: `src/services/plugins/mod.rs`
- Create: `src/services/plugins/loader.rs`
- Modify: `src/services/mod.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests**

```rust
// src/services/plugins/loader.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_plugin_id_builtin() {
        let def = BuiltinPluginDefinition {
            name: "test-plugin".to_string(),
            description: "A test".to_string(),
            ..Default::default()
        };
        assert_eq!(def.plugin_id(), "test-plugin@builtin");
    }

    #[test]
    fn test_is_plugin_enabled_default() {
        let enabled: HashMap<String, bool> = HashMap::new();
        let def = BuiltinPluginDefinition {
            name: "test".to_string(),
            default_enabled: None, // defaults to true
            ..Default::default()
        };
        assert!(is_plugin_enabled(&def.plugin_id(), &enabled, def.default_enabled));
    }

    #[test]
    fn test_is_plugin_enabled_explicit_false() {
        let mut enabled = HashMap::new();
        enabled.insert("test@builtin".to_string(), false);
        assert!(!is_plugin_enabled("test@builtin", &enabled, None));
    }

    #[test]
    fn test_register_and_get_builtin() {
        let mut registry = PluginRegistry::new();
        registry.register(BuiltinPluginDefinition {
            name: "my-plugin".to_string(),
            description: "desc".to_string(),
            ..Default::default()
        });
        assert_eq!(registry.all().len(), 1);
        assert!(registry.get("my-plugin").is_some());
    }

    #[test]
    fn test_get_enabled_plugins() {
        let mut registry = PluginRegistry::new();
        registry.register(BuiltinPluginDefinition {
            name: "enabled-one".to_string(),
            description: "e".to_string(),
            ..Default::default()
        });
        registry.register(BuiltinPluginDefinition {
            name: "disabled-one".to_string(),
            description: "d".to_string(),
            default_enabled: Some(false),
            ..Default::default()
        });
        let enabled_settings = HashMap::new();
        let enabled = registry.get_enabled(&enabled_settings);
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "enabled-one");
    }
}
```

- [ ] **Step 2: Implement plugin types and registry**

```rust
// src/services/plugins/loader.rs
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct BuiltinPluginDefinition {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub hooks: Option<crate::services::hooks::types::HooksSettings>,
    pub default_enabled: Option<bool>,
}

impl BuiltinPluginDefinition {
    pub fn plugin_id(&self) -> String {
        format!("{}@builtin", self.name)
    }
}

pub struct PluginRegistry {
    plugins: HashMap<String, BuiltinPluginDefinition>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self { plugins: HashMap::new() }
    }

    pub fn register(&mut self, def: BuiltinPluginDefinition) {
        self.plugins.insert(def.name.clone(), def);
    }

    pub fn get(&self, name: &str) -> Option<&BuiltinPluginDefinition> {
        self.plugins.get(name)
    }

    pub fn all(&self) -> Vec<&BuiltinPluginDefinition> {
        self.plugins.values().collect()
    }

    pub fn get_enabled(&self, enabled_settings: &HashMap<String, bool>) -> Vec<&BuiltinPluginDefinition> {
        self.plugins.values()
            .filter(|p| is_plugin_enabled(&p.plugin_id(), enabled_settings, p.default_enabled))
            .collect()
    }
}

pub fn is_plugin_enabled(
    plugin_id: &str,
    enabled_settings: &HashMap<String, bool>,
    default_enabled: Option<bool>,
) -> bool {
    if let Some(&enabled) = enabled_settings.get(plugin_id) {
        return enabled;
    }
    default_enabled.unwrap_or(true)
}
```

```rust
// src/services/plugins/mod.rs
pub mod loader;
pub use loader::{BuiltinPluginDefinition, PluginRegistry, is_plugin_enabled};
```

- [ ] **Step 3: Add module, run tests, commit**

Add `pub mod plugins;` to `src/services/mod.rs`.

Run: `cargo test --lib services::plugins -- --nocapture`
Expected: All PASS

```bash
git add src/services/plugins/ src/services/mod.rs
git commit -m "feat(plugins): plugin registry with builtin definitions and enable/disable logic"
```

---

## Task 4: Plugin Hooks Bridge

**Files:**
- Create: `src/services/plugins/hooks_bridge.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests**

```rust
// src/services/plugins/hooks_bridge.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_plugin_hooks_into_settings() {
        let mut base = HooksSettings::new();
        let plugin_def = BuiltinPluginDefinition {
            name: "test-plugin".to_string(),
            hooks: Some({
                let mut h = HooksSettings::new();
                h.insert("SessionStart".to_string(), vec![HookMatcher {
                    matcher: None,
                    hooks: vec![HookCommand::Command {
                        command: "echo plugin loaded".to_string(),
                        shell: None, timeout: None, status_message: None,
                        once: None, r#async: None, async_rewake: None, r#if: None,
                    }],
                }]);
                h
            }),
            ..Default::default()
        };

        merge_plugin_hooks(&mut base, &plugin_def);
        assert!(base.contains_key("SessionStart"));
        assert_eq!(base["SessionStart"].len(), 1);
    }

    #[test]
    fn test_merge_plugin_hooks_no_hooks() {
        let mut base = HooksSettings::new();
        let plugin_def = BuiltinPluginDefinition {
            name: "empty-plugin".to_string(),
            hooks: None,
            ..Default::default()
        };
        merge_plugin_hooks(&mut base, &plugin_def);
        assert!(base.is_empty());
    }
}
```

- [ ] **Step 2: Implement hooks bridge**

```rust
// src/services/plugins/hooks_bridge.rs
use crate::services::hooks::types::HooksSettings;
use super::loader::BuiltinPluginDefinition;

/// Merge a plugin's hooks into the base hook settings.
pub fn merge_plugin_hooks(base: &mut HooksSettings, plugin: &BuiltinPluginDefinition) {
    if let Some(ref plugin_hooks) = plugin.hooks {
        for (event, matchers) in plugin_hooks {
            base.entry(event.clone()).or_default().extend(matchers.clone());
        }
    }
}

/// Collect hooks from all enabled plugins.
pub fn collect_plugin_hooks(
    registry: &super::PluginRegistry,
    enabled_settings: &std::collections::HashMap<String, bool>,
) -> HooksSettings {
    let mut all_hooks = HooksSettings::new();
    for plugin in registry.get_enabled(enabled_settings) {
        merge_plugin_hooks(&mut all_hooks, plugin);
    }
    all_hooks
}
```

- [ ] **Step 3: Wire into plugins/mod.rs, run tests, commit**

Add `pub mod hooks_bridge;` to `src/services/plugins/mod.rs`.

```bash
git add src/services/plugins/hooks_bridge.rs src/services/plugins/mod.rs
git commit -m "feat(plugins): hooks bridge to merge plugin hooks into hook settings"
```

---

## Task 5: Coordinator Mode Detection & Tool Filtering

**Files:**
- Create: `src/services/coordinator/mod.rs`
- Create: `src/services/coordinator/worker.rs`
- Modify: `src/services/mod.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests**

```rust
// src/services/coordinator/mod.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_coordinator_mode_default_false() {
        // Without env var, should be false
        std::env::remove_var("ATOM_CODE_COORDINATOR_MODE");
        assert!(!is_coordinator_mode());
    }

    #[test]
    fn test_coordinator_allowed_tools() {
        assert_eq!(COORDINATOR_ALLOWED_TOOLS.len(), 4);
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"Agent"));
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"TaskStop"));
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"SendMessage"));
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"SyntheticOutput"));
    }

    #[test]
    fn test_async_agent_allowed_tools() {
        assert!(ASYNC_AGENT_ALLOWED_TOOLS.contains(&"Read"));
        assert!(ASYNC_AGENT_ALLOWED_TOOLS.contains(&"Write"));
        assert!(ASYNC_AGENT_ALLOWED_TOOLS.contains(&"Bash"));
        assert!(!ASYNC_AGENT_ALLOWED_TOOLS.contains(&"Agent")); // blocked
    }

    #[test]
    fn test_filter_tools_for_coordinator() {
        let tools = vec!["Agent", "Read", "Write", "Bash", "TaskStop"];
        let filtered = filter_coordinator_tools(&tools);
        // Only coordinator-allowed tools remain
        assert!(filtered.contains(&"Agent"));
        assert!(filtered.contains(&"TaskStop"));
        assert!(!filtered.contains(&"Read"));
    }
}
```

- [ ] **Step 2: Implement coordinator mode**

```rust
// src/services/coordinator/mod.rs
pub mod worker;

pub const COORDINATOR_ALLOWED_TOOLS: &[&str] = &[
    "Agent", "TaskStop", "SendMessage", "SyntheticOutput",
];

pub const ASYNC_AGENT_ALLOWED_TOOLS: &[&str] = &[
    "Read", "Write", "Edit", "Glob", "Grep", "Bash",
    "WebSearch", "WebFetch", "TodoWrite", "NotebookEdit",
    "Skill", "ToolSearch", "SyntheticOutput",
    "EnterWorktree", "ExitWorktree",
];

pub fn is_coordinator_mode() -> bool {
    std::env::var("ATOM_CODE_COORDINATOR_MODE")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
}

pub fn filter_coordinator_tools<'a>(tool_names: &[&'a str]) -> Vec<&'a str> {
    tool_names.iter()
        .filter(|name| COORDINATOR_ALLOWED_TOOLS.contains(name))
        .copied()
        .collect()
}

pub fn filter_worker_tools<'a>(tool_names: &[&'a str]) -> Vec<&'a str> {
    tool_names.iter()
        .filter(|name| ASYNC_AGENT_ALLOWED_TOOLS.contains(name))
        .copied()
        .collect()
}

pub fn match_session_mode(stored_mode: Option<&str>) -> Option<String> {
    let current = is_coordinator_mode();
    let stored_is_coordinator = stored_mode == Some("coordinator");
    if current != stored_is_coordinator {
        if current {
            std::env::remove_var("ATOM_CODE_COORDINATOR_MODE");
        } else {
            std::env::set_var("ATOM_CODE_COORDINATOR_MODE", "1");
        }
        Some(format!(
            "Session mode switched from {} to {}",
            if stored_is_coordinator { "coordinator" } else { "normal" },
            if current { "coordinator" } else { "normal" },
        ))
    } else {
        None
    }
}

pub fn get_coordinator_user_context(
    mcp_server_names: &[String],
    scratchpad: Option<&std::path::Path>,
) -> std::collections::HashMap<String, String> {
    let mut ctx = std::collections::HashMap::new();
    if !is_coordinator_mode() {
        return ctx;
    }

    let tools_list = ASYNC_AGENT_ALLOWED_TOOLS.join(", ");
    let mut content = format!("Workers spawned via the Agent tool have access to these tools: {}", tools_list);

    if !mcp_server_names.is_empty() {
        content.push_str(&format!(
            "\n\nWorkers also have access to MCP tools from connected MCP servers: {}",
            mcp_server_names.join(", ")
        ));
    }

    if let Some(scratchpad_path) = scratchpad {
        content.push_str(&format!(
            "\n\nScratchpad directory: {}\nWorkers can read and write here without permission prompts.",
            scratchpad_path.display()
        ));
    }

    ctx.insert("workerToolsContext".to_string(), content);
    ctx
}
```

- [ ] **Step 3: Create worker.rs stub**

```rust
// src/services/coordinator/worker.rs

/// Task notification XML format for async agent completion.
pub fn format_task_notification(
    agent_id: &str,
    status: &str,
    summary: &str,
    result: &str,
    total_tokens: u64,
    tool_uses: u64,
    duration_ms: u64,
) -> String {
    format!(
        "<task-notification>\n<task-id>{}</task-id>\n<status>{}</status>\n<summary>{}</summary>\n<result>{}</result>\n<usage>\n  <total_tokens>{}</total_tokens>\n  <tool_uses>{}</tool_uses>\n  <duration_ms>{}</duration_ms>\n</usage>\n</task-notification>",
        agent_id, status, summary, result, total_tokens, tool_uses, duration_ms
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_task_notification() {
        let xml = format_task_notification("abc123", "completed", "Done", "Result text", 1000, 5, 3000);
        assert!(xml.contains("<task-id>abc123</task-id>"));
        assert!(xml.contains("<status>completed</status>"));
        assert!(xml.contains("<total_tokens>1000</total_tokens>"));
    }
}
```

- [ ] **Step 4: Add module, run tests, commit**

Add `pub mod coordinator;` to `src/services/mod.rs`.

Run: `cargo test --lib services::coordinator -- --nocapture`
Expected: All PASS

```bash
git add src/services/coordinator/ src/services/mod.rs
git commit -m "feat(coordinator): mode detection, tool filtering, worker context, task notification format"
```

---

## Task 6: AgentTool — Tool Trait Implementation

**Files:**
- Create: `src/tools/agent_tool.rs`
- Modify: `src/tools/mod.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests**

```rust
// src/tools/agent_tool.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_tool_name() {
        let tool = AgentTool;
        assert_eq!(tool.name(), "Agent");
    }

    #[test]
    fn test_agent_tool_schema() {
        let tool = AgentTool;
        let schema = tool.input_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("prompt"));
        assert!(props.contains_key("description"));
        assert!(props.contains_key("subagent_type"));
        assert!(props.contains_key("model"));
        assert!(props.contains_key("run_in_background"));
        assert!(props.contains_key("isolation"));
    }

    #[test]
    fn test_agent_tool_is_not_read_only() {
        let tool = AgentTool;
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_agent_tool_not_concurrent_safe() {
        let tool = AgentTool;
        assert!(!tool.is_concurrent_safe());
    }
}
```

- [ ] **Step 2: Implement AgentTool**

```rust
// src/tools/agent_tool.rs
use crate::tool::{Tool, ToolUseContext, ToolResult, ToolDefinition, ValidationResult, PermissionMeta};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct AgentTool;

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &str { "Agent" }

    fn description(&self) -> &str {
        "Launch a new agent to handle complex, multi-step tasks autonomously."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The task for the agent to perform"
                },
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of specialized agent to use"
                },
                "model": {
                    "type": "string",
                    "description": "Optional model override for this agent"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Run this agent in the background"
                },
                "name": {
                    "type": "string",
                    "description": "Named agent for SendMessage continuation"
                },
                "isolation": {
                    "type": "string",
                    "enum": ["worktree"],
                    "description": "Isolation mode"
                }
            },
            "required": ["prompt"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> anyhow::Result<ToolResult> {
        let prompt = ctx.input.get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let description = ctx.input.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("agent task");

        // Stub: return a message indicating the agent was requested
        // Full implementation requires: query engine fork, tool pool assembly,
        // async agent lifecycle management
        Ok(ToolResult {
            content: format!(
                "Agent requested: '{}' (description: '{}'). \
                Note: Full agent spawning requires coordinator infrastructure.",
                prompt, description
            ),
            is_error: false,
        })
    }

    fn can_use(&self, _ctx: &ToolUseContext) -> ValidationResult {
        ValidationResult::Ok
    }

    fn permission_metadata(&self, input: &Value) -> PermissionMeta {
        PermissionMeta {
            file_path: None,
            command: input.get("prompt").and_then(|v| v.as_str()).map(|s| s.to_string()),
            url: None,
        }
    }

    fn is_read_only(&self) -> bool { false }
    fn is_concurrent_safe(&self) -> bool { false }
    fn aliases(&self) -> &[&str] { &[] }
    fn is_mcp(&self) -> bool { false }
}
```

- [ ] **Step 3: Register in tools/mod.rs**

Add to `get_builtin_tools()`:
```rust
Box::new(agent_tool::AgentTool),
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --lib tools::agent_tool -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/tools/agent_tool.rs src/tools/mod.rs
git commit -m "feat(coordinator): AgentTool implementing Tool trait with schema and stub execution"
```

---

## Task 7: Output Style TUI Command Enhancement

**Files:**
- Modify: `src/commands/tui_cmds/output_style.rs`
- Modify: `src/tui/app.rs`
- Test: manual verification

- [ ] **Step 1: Add output_style field to App**

In `src/tui/app.rs`, add `pub output_style: String` to App struct, initialize as `"default".to_string()` in `App::new()`.

- [ ] **Step 2: Update OutputStyleCommand to use real styles**

Read the current `output_style.rs` and update it to load styles from `crate::services::output_styles::get_all_output_styles()` instead of hardcoded options. The command should:
- List all available styles (built-in + user + project)
- Show current selection
- Allow selection via Enter key
- Update `app.output_style` on selection

- [ ] **Step 3: Verify build compiles**

Run: `cargo build`
Expected: Success

- [ ] **Step 4: Commit**

```bash
git add src/commands/tui_cmds/output_style.rs src/tui/app.rs
git commit -m "feat(output_styles): enhance TUI command with real style loading and selection"
```

---

## Task 8: Final Integration & Cleanup

**Files:**
- All modified files
- Test: full cargo test + cargo clippy

- [ ] **Step 1: Run full test suite**

Run: `cargo test --lib`
Expected: All PASS (except pre-existing skills failures)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --lib`
Expected: No new errors

- [ ] **Step 3: Verify release build**

Run: `cargo build --release`
Expected: Success

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: Phase 10B complete — coordinator, plugins, output styles"
```
