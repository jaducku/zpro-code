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
