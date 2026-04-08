use crate::types::permissions::*;

use super::auto_classify::auto_classify;
use super::PermissionDecision;

/// Check permission for a tool call.
///
/// Flow:
/// 1. Bypass/DontAsk mode -> Allow
/// 2. Plan mode -> Deny
/// 3. Deny rules match -> Deny
/// 4. Allow rules match -> Allow
/// 5. AcceptEdits + edit tool -> Allow
/// 6. Auto mode -> auto_classify
/// 7. Default -> Ask
pub fn check_permission(
    tool_name: &str,
    tool_input: &serde_json::Value,
    permission_meta: &PermissionMeta,
    ctx: &ToolPermissionContext,
) -> PermissionDecision {
    // 1. Bypass/DontAsk -> Allow
    if ctx.mode == PermissionMode::BypassPermissions || ctx.mode == PermissionMode::DontAsk {
        return PermissionDecision::Allow {
            reason: format!("mode:{:?}", ctx.mode),
        };
    }

    // 2. Plan mode -> Deny
    if ctx.mode == PermissionMode::Plan {
        return PermissionDecision::Deny {
            reason: "plan mode denies all tool use".into(),
        };
    }

    // 3. Deny rules
    if matches_any_rule(tool_name, permission_meta, &ctx.always_deny_rules) {
        return PermissionDecision::Deny {
            reason: "matched deny rule".into(),
        };
    }

    // 4. Allow rules
    if matches_any_rule(tool_name, permission_meta, &ctx.always_allow_rules) {
        return PermissionDecision::Allow {
            reason: "matched allow rule".into(),
        };
    }

    // 5. AcceptEdits + edit tool -> Allow
    if ctx.mode == PermissionMode::AcceptEdits && is_edit_tool(tool_name) {
        return PermissionDecision::Allow {
            reason: "acceptEdits mode allows edit tools".into(),
        };
    }

    // 6. Auto mode -> auto_classify
    if ctx.mode == PermissionMode::Auto {
        let behavior = auto_classify(tool_name, tool_input);
        return match behavior {
            PermissionBehavior::Allow => PermissionDecision::Allow {
                reason: "auto-classified as safe".into(),
            },
            PermissionBehavior::Ask | PermissionBehavior::Deny => PermissionDecision::Ask {
                tool_name: tool_name.to_string(),
                description: format_tool_description(tool_name, permission_meta),
            },
        };
    }

    // 7. Default -> Ask
    PermissionDecision::Ask {
        tool_name: tool_name.to_string(),
        description: format_tool_description(tool_name, permission_meta),
    }
}

/// Check if a tool is an edit/write tool.
fn is_edit_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Edit" | "Write" | "NotebookEdit")
}

/// Check if any rule in the list matches the given tool + metadata.
fn matches_any_rule(tool_name: &str, meta: &PermissionMeta, rules: &[PermissionRule]) -> bool {
    rules.iter().any(|rule| {
        if rule.tool_name != tool_name && rule.tool_name != "*" {
            return false;
        }
        match &rule.rule_content {
            None => true,
            Some(pattern) => match_rule_content(pattern, meta),
        }
    })
}

/// Match a rule_content pattern against the relevant metadata field.
///
/// Pattern format: simple glob with `*` suffix (e.g. `git:*` matches commands
/// starting with "git:"). A bare `*` matches everything.
fn match_rule_content(pattern: &str, meta: &PermissionMeta) -> bool {
    let text = meta
        .command
        .as_deref()
        .or(meta.file_path.as_deref())
        .or(meta.url.as_deref())
        .unwrap_or("");

    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return text.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return text.ends_with(suffix);
    }
    pattern == text
}

/// Format a human-readable description for the permission prompt.
fn format_tool_description(tool_name: &str, meta: &PermissionMeta) -> String {
    match tool_name {
        "Bash" => format!(
            "Run command: {}",
            meta.command.as_deref().unwrap_or("(unknown)")
        ),
        "Write" | "Edit" | "Read" => format!(
            "{} file: {}",
            tool_name,
            meta.file_path.as_deref().unwrap_or("(unknown)")
        ),
        _ => format!("Use tool: {}", tool_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_input() -> serde_json::Value {
        serde_json::json!({})
    }

    fn bash_input(cmd: &str) -> serde_json::Value {
        serde_json::json!({ "command": cmd })
    }

    fn meta_cmd(cmd: &str) -> PermissionMeta {
        PermissionMeta {
            command: Some(cmd.into()),
            file_path: None,
            url: None,
        }
    }

    fn meta_file(path: &str) -> PermissionMeta {
        PermissionMeta {
            command: None,
            file_path: Some(path.into()),
            url: None,
        }
    }

    fn meta_empty() -> PermissionMeta {
        PermissionMeta {
            command: None,
            file_path: None,
            url: None,
        }
    }

    fn ctx_with_mode(mode: PermissionMode) -> ToolPermissionContext {
        ToolPermissionContext {
            mode,
            ..Default::default()
        }
    }

    #[test]
    fn test_bypass_allows_everything() {
        let result = check_permission(
            "Bash",
            &bash_input("rm -rf /"),
            &meta_cmd("rm -rf /"),
            &ctx_with_mode(PermissionMode::BypassPermissions),
        );
        assert!(matches!(result, PermissionDecision::Allow { .. }));
    }

    #[test]
    fn test_plan_denies_everything() {
        let result = check_permission(
            "Bash",
            &bash_input("ls"),
            &meta_cmd("ls"),
            &ctx_with_mode(PermissionMode::Plan),
        );
        assert!(matches!(result, PermissionDecision::Deny { .. }));

        // Even read tools are denied in plan mode
        let result = check_permission(
            "Read",
            &empty_input(),
            &meta_file("/tmp/test.rs"),
            &ctx_with_mode(PermissionMode::Plan),
        );
        assert!(matches!(result, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn test_deny_rule_matches() {
        let ctx = ToolPermissionContext {
            mode: PermissionMode::Default,
            always_deny_rules: vec![PermissionRule {
                tool_name: "Bash".into(),
                rule_content: Some("rm*".into()),
            }],
            ..Default::default()
        };
        let result = check_permission("Bash", &bash_input("rm -rf /"), &meta_cmd("rm -rf /"), &ctx);
        assert!(matches!(result, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn test_allow_rule_matches() {
        let ctx = ToolPermissionContext {
            mode: PermissionMode::Default,
            always_allow_rules: vec![PermissionRule {
                tool_name: "Bash".into(),
                rule_content: Some("git *".into()),
            }],
            ..Default::default()
        };
        let result = check_permission(
            "Bash",
            &bash_input("git status"),
            &meta_cmd("git status"),
            &ctx,
        );
        assert!(matches!(result, PermissionDecision::Allow { .. }));
    }

    #[test]
    fn test_accept_edits_allows_edit() {
        let result = check_permission(
            "Edit",
            &empty_input(),
            &meta_file("/tmp/foo.rs"),
            &ctx_with_mode(PermissionMode::AcceptEdits),
        );
        assert!(matches!(result, PermissionDecision::Allow { .. }));

        // Write is also an edit tool
        let result = check_permission(
            "Write",
            &empty_input(),
            &meta_file("/tmp/bar.rs"),
            &ctx_with_mode(PermissionMode::AcceptEdits),
        );
        assert!(matches!(result, PermissionDecision::Allow { .. }));
    }

    #[test]
    fn test_accept_edits_asks_bash() {
        let result = check_permission(
            "Bash",
            &bash_input("npm install"),
            &meta_cmd("npm install"),
            &ctx_with_mode(PermissionMode::AcceptEdits),
        );
        assert!(matches!(result, PermissionDecision::Ask { .. }));
    }

    #[test]
    fn test_auto_allows_read() {
        let result = check_permission(
            "Read",
            &empty_input(),
            &meta_file("/tmp/test.rs"),
            &ctx_with_mode(PermissionMode::Auto),
        );
        assert!(matches!(result, PermissionDecision::Allow { .. }));
    }

    #[test]
    fn test_auto_allows_readonly_bash() {
        let result = check_permission(
            "Bash",
            &bash_input("git status"),
            &meta_cmd("git status"),
            &ctx_with_mode(PermissionMode::Auto),
        );
        assert!(matches!(result, PermissionDecision::Allow { .. }));
    }

    #[test]
    fn test_auto_asks_mutating_bash() {
        let result = check_permission(
            "Bash",
            &bash_input("npm install"),
            &meta_cmd("npm install"),
            &ctx_with_mode(PermissionMode::Auto),
        );
        assert!(matches!(result, PermissionDecision::Ask { .. }));
    }

    #[test]
    fn test_default_always_asks() {
        // Bash asks
        let result = check_permission(
            "Bash",
            &bash_input("ls"),
            &meta_cmd("ls"),
            &ctx_with_mode(PermissionMode::Default),
        );
        assert!(matches!(result, PermissionDecision::Ask { .. }));

        // Read asks
        let result = check_permission(
            "Read",
            &empty_input(),
            &meta_file("/tmp/test.rs"),
            &ctx_with_mode(PermissionMode::Default),
        );
        assert!(matches!(result, PermissionDecision::Ask { .. }));

        // Write asks
        let result = check_permission(
            "Write",
            &empty_input(),
            &meta_file("/tmp/test.rs"),
            &ctx_with_mode(PermissionMode::Default),
        );
        assert!(matches!(result, PermissionDecision::Ask { .. }));
    }

    #[test]
    fn test_deny_rules_before_allow_rules() {
        let ctx = ToolPermissionContext {
            mode: PermissionMode::Default,
            always_deny_rules: vec![PermissionRule {
                tool_name: "Bash".into(),
                rule_content: None,
            }],
            always_allow_rules: vec![PermissionRule {
                tool_name: "Bash".into(),
                rule_content: None,
            }],
            ..Default::default()
        };
        let result = check_permission("Bash", &bash_input("ls"), &meta_cmd("ls"), &ctx);
        assert!(matches!(result, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn test_dontask_allows_everything() {
        let result = check_permission(
            "Bash",
            &bash_input("rm -rf /"),
            &meta_cmd("rm -rf /"),
            &ctx_with_mode(PermissionMode::DontAsk),
        );
        assert!(matches!(result, PermissionDecision::Allow { .. }));
    }

    #[test]
    fn test_match_rule_content_glob() {
        let meta = meta_cmd("git status");
        assert!(match_rule_content("git *", &meta));
        assert!(match_rule_content("git status", &meta));
        assert!(!match_rule_content("npm*", &meta));
        assert!(match_rule_content("*", &meta));
    }

    #[test]
    fn test_format_tool_description() {
        assert_eq!(
            format_tool_description("Bash", &meta_cmd("ls -la")),
            "Run command: ls -la"
        );
        assert_eq!(
            format_tool_description("Edit", &meta_file("/tmp/foo.rs")),
            "Edit file: /tmp/foo.rs"
        );
        assert_eq!(
            format_tool_description("SomeTool", &meta_empty()),
            "Use tool: SomeTool"
        );
    }
}
