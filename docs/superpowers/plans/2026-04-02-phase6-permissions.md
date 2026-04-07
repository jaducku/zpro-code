# Phase 6: Permission System

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the permission checking system that validates tool calls before execution.

**Architecture:** Permission rules are checked in order: bypass → deny rules → allow rules → YOLO classifier (auto mode) → ask user. This mirrors the original `src/utils/permissions/`.

**Tech Stack:** globset (for pattern matching), serde

**Spec:** `docs/superpowers/specs/2026-04-02-ts-to-rust-migration-design.md` (Section 8)

---

### Task 1: Permission checker

**Files:**
- Create: `src/utils/permissions/checker.rs`
- Create: `src/utils/permissions/mod.rs`
- Modify: `src/utils/mod.rs`

- [ ] **Step 1: Implement permission checking logic**

```rust
// src/utils/permissions/checker.rs
use crate::types::permissions::*;

/// Check permission for a tool call
pub fn check_permission(
    tool_name: &str,
    meta: &PermissionMeta,
    context: &ToolPermissionContext,
) -> PermissionResult {
    // 1. Bypass mode — allow everything
    if context.mode == PermissionMode::BypassPermissions {
        return PermissionResult::Allowed;
    }

    // 2. Check deny rules
    for rule in &context.always_deny_rules {
        if matches_rule(tool_name, meta, rule) {
            return PermissionResult::Denied;
        }
    }

    // 3. Check allow rules
    for rule in &context.always_allow_rules {
        if matches_rule(tool_name, meta, rule) {
            return PermissionResult::Allowed;
        }
    }

    // 4. Auto mode — use YOLO classifier for safe operations
    if context.mode == PermissionMode::Auto || context.mode == PermissionMode::AcceptEdits {
        if is_safe_operation(tool_name, meta) {
            return PermissionResult::Allowed;
        }
    }

    // 5. Plan mode — deny non-read operations
    if context.mode == PermissionMode::Plan {
        if is_read_only_tool(tool_name) {
            return PermissionResult::Allowed;
        }
        return PermissionResult::Denied;
    }

    // 6. Default — ask user
    PermissionResult::Ask(PermissionRequest {
        tool_name: tool_name.to_string(),
        description: format_permission_description(tool_name, meta),
        metadata: meta.clone(),
    })
}

/// Check if a rule matches the tool + metadata
fn matches_rule(tool_name: &str, meta: &PermissionMeta, rule: &PermissionRule) -> bool {
    if rule.tool_name != tool_name && rule.tool_name != "*" {
        return false;
    }

    match &rule.rule_content {
        None => true, // No pattern = matches all invocations of this tool
        Some(pattern) => {
            // Match against the relevant metadata field
            if let Some(cmd) = &meta.command {
                glob_match(pattern, cmd)
            } else if let Some(path) = &meta.file_path {
                glob_match(pattern, path)
            } else {
                false
            }
        }
    }
}

/// Simple glob matching (supports * and **)
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    // Simple prefix/suffix matching for common patterns
    if let Some(suffix) = pattern.strip_prefix('*') {
        return text.ends_with(suffix);
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return text.starts_with(prefix);
    }

    // Exact match fallback
    pattern == text
}

/// YOLO classifier — determine if an operation is safe to auto-approve
fn is_safe_operation(tool_name: &str, meta: &PermissionMeta) -> bool {
    match tool_name {
        // Read-only tools are always safe
        "Read" | "Glob" | "Grep" | "WebSearch" | "WebFetch" => true,

        // Bash: only safe for simple read commands
        "Bash" => {
            if let Some(cmd) = &meta.command {
                is_safe_bash_command(cmd)
            } else {
                false
            }
        }

        // File operations: safe within project directory
        "Write" | "Edit" => {
            if let Some(path) = &meta.file_path {
                !is_sensitive_path(path)
            } else {
                false
            }
        }

        _ => false,
    }
}

/// Check if a bash command is safe (no destructive operations)
fn is_safe_bash_command(cmd: &str) -> bool {
    let dangerous_patterns = [
        "rm ", "rm\t", "rmdir",
        "sudo", "chmod", "chown",
        "mv ", "mv\t",
        "> /", ">> /",
        "kill", "pkill",
        "dd ", "mkfs",
        "curl", "wget",
        "eval", "exec",
        "|", ";", "&&", "||",
        "`", "$(", "${",
    ];

    let cmd_lower = cmd.to_lowercase();
    !dangerous_patterns.iter().any(|p| cmd_lower.contains(p))
}

/// Check if a file path is sensitive
fn is_sensitive_path(path: &str) -> bool {
    let sensitive = [
        ".env", "credentials", ".ssh",
        "/etc/", "/usr/", "/bin/",
        "id_rsa", "id_ed25519",
        ".git/config",
    ];
    sensitive.iter().any(|s| path.contains(s))
}

/// Check if a tool is read-only by name
fn is_read_only_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Read" | "Glob" | "Grep" | "WebSearch" | "WebFetch"
        | "ListMcpResources" | "ReadMcpResource" | "TaskGet" | "TaskList")
}

fn format_permission_description(tool_name: &str, meta: &PermissionMeta) -> String {
    match tool_name {
        "Bash" => format!("Run command: {}", meta.command.as_deref().unwrap_or("?")),
        "Write" | "Edit" | "Read" => format!("{} file: {}", tool_name, meta.file_path.as_deref().unwrap_or("?")),
        _ => format!("Use tool: {}", tool_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_ctx() -> ToolPermissionContext {
        ToolPermissionContext::default()
    }

    fn bypass_ctx() -> ToolPermissionContext {
        ToolPermissionContext {
            mode: PermissionMode::BypassPermissions,
            ..Default::default()
        }
    }

    fn auto_ctx() -> ToolPermissionContext {
        ToolPermissionContext {
            mode: PermissionMode::Auto,
            ..Default::default()
        }
    }

    #[test]
    fn test_bypass_allows_everything() {
        let meta = PermissionMeta { command: Some("rm -rf /".into()), file_path: None, url: None };
        assert!(matches!(check_permission("Bash", &meta, &bypass_ctx()), PermissionResult::Allowed));
    }

    #[test]
    fn test_deny_rule() {
        let ctx = ToolPermissionContext {
            always_deny_rules: vec![PermissionRule { tool_name: "Bash".into(), rule_content: None }],
            ..Default::default()
        };
        let meta = PermissionMeta { command: Some("ls".into()), file_path: None, url: None };
        assert!(matches!(check_permission("Bash", &meta, &ctx), PermissionResult::Denied));
    }

    #[test]
    fn test_allow_rule() {
        let ctx = ToolPermissionContext {
            always_allow_rules: vec![PermissionRule { tool_name: "Bash".into(), rule_content: Some("ls*".into()) }],
            ..Default::default()
        };
        let meta = PermissionMeta { command: Some("ls -la".into()), file_path: None, url: None };
        assert!(matches!(check_permission("Bash", &meta, &ctx), PermissionResult::Allowed));
    }

    #[test]
    fn test_deny_before_allow() {
        let ctx = ToolPermissionContext {
            always_deny_rules: vec![PermissionRule { tool_name: "Bash".into(), rule_content: None }],
            always_allow_rules: vec![PermissionRule { tool_name: "Bash".into(), rule_content: None }],
            ..Default::default()
        };
        let meta = PermissionMeta { command: Some("ls".into()), file_path: None, url: None };
        assert!(matches!(check_permission("Bash", &meta, &ctx), PermissionResult::Denied));
    }

    #[test]
    fn test_auto_mode_safe_read() {
        let meta = PermissionMeta { command: None, file_path: Some("/tmp/test.txt".into()), url: None };
        assert!(matches!(check_permission("Read", &meta, &auto_ctx()), PermissionResult::Allowed));
    }

    #[test]
    fn test_auto_mode_safe_bash() {
        let meta = PermissionMeta { command: Some("ls -la".into()), file_path: None, url: None };
        assert!(matches!(check_permission("Bash", &meta, &auto_ctx()), PermissionResult::Allowed));
    }

    #[test]
    fn test_auto_mode_dangerous_bash() {
        let meta = PermissionMeta { command: Some("rm -rf /".into()), file_path: None, url: None };
        assert!(matches!(check_permission("Bash", &meta, &auto_ctx()), PermissionResult::Ask(_)));
    }

    #[test]
    fn test_default_mode_asks() {
        let meta = PermissionMeta { command: Some("ls".into()), file_path: None, url: None };
        assert!(matches!(check_permission("Bash", &meta, &default_ctx()), PermissionResult::Ask(_)));
    }

    #[test]
    fn test_sensitive_path() {
        assert!(is_sensitive_path("/home/user/.env"));
        assert!(is_sensitive_path("/home/user/.ssh/id_rsa"));
        assert!(!is_sensitive_path("/home/user/project/src/main.rs"));
    }

    #[test]
    fn test_safe_bash_command() {
        assert!(is_safe_bash_command("ls -la"));
        assert!(is_safe_bash_command("cat file.txt"));
        assert!(is_safe_bash_command("git status"));
        assert!(!is_safe_bash_command("rm -rf /"));
        assert!(!is_safe_bash_command("sudo apt install"));
        assert!(!is_safe_bash_command("echo hi | cat"));
        assert!(!is_safe_bash_command("ls && rm foo"));
    }

    #[test]
    fn test_plan_mode() {
        let ctx = ToolPermissionContext {
            mode: PermissionMode::Plan,
            ..Default::default()
        };
        let read_meta = PermissionMeta { file_path: Some("test.rs".into()), command: None, url: None };
        let write_meta = PermissionMeta { file_path: Some("test.rs".into()), command: None, url: None };
        assert!(matches!(check_permission("Read", &read_meta, &ctx), PermissionResult::Allowed));
        assert!(matches!(check_permission("Write", &write_meta, &ctx), PermissionResult::Denied));
    }

    #[test]
    fn test_wildcard_rule() {
        let ctx = ToolPermissionContext {
            always_allow_rules: vec![PermissionRule { tool_name: "*".into(), rule_content: None }],
            ..Default::default()
        };
        let meta = PermissionMeta { command: Some("anything".into()), file_path: None, url: None };
        assert!(matches!(check_permission("Bash", &meta, &ctx), PermissionResult::Allowed));
        assert!(matches!(check_permission("Write", &meta, &ctx), PermissionResult::Allowed));
    }
}
```

```rust
// src/utils/permissions/mod.rs
pub mod checker;

pub use checker::check_permission;
```

- [ ] **Step 2: Update utils/mod.rs**

Add `pub mod permissions;` to `src/utils/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code utils::permissions`
Expected: 12 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/utils/permissions/ src/utils/mod.rs
git commit -m "feat: implement permission system (bypass, deny, allow, auto/YOLO, plan mode)"
```

---

### Task 2: Verification

- [ ] **Step 1: Full test suite**

Run: `cargo test --workspace`
Run: `cargo clippy --workspace -- -D warnings`
Expected: All pass, no warnings
