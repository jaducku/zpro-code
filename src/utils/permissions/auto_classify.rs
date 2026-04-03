use crate::types::permissions::PermissionBehavior;

/// Auto-classify a tool invocation as Allow/Ask based on tool name and input.
pub fn auto_classify(tool_name: &str, input: &serde_json::Value) -> PermissionBehavior {
    match tool_name {
        "Read" | "Glob" | "Grep" => PermissionBehavior::Allow,
        "Bash" => {
            let command = input
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if is_readonly_bash(command) {
                PermissionBehavior::Allow
            } else {
                PermissionBehavior::Ask
            }
        }
        _ => PermissionBehavior::Ask,
    }
}

/// Check if a bash command is read-only (no side effects).
fn is_readonly_bash(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    let first_word = trimmed.split_whitespace().next().unwrap_or("");

    // Simple commands that are always read-only
    let safe_single = [
        "ls", "cat", "head", "tail", "find", "grep", "rg", "which", "pwd", "echo", "wc", "file",
        "stat", "du", "df", "env", "printenv", "uname", "hostname", "whoami", "date", "tree",
    ];
    if safe_single.contains(&first_word) {
        return true;
    }

    // Two-word prefixes: "git status", "cargo check", etc.
    let safe_prefixes: &[(&str, &[&str])] = &[
        (
            "git",
            &["status", "log", "diff", "branch", "show", "rev-parse", "remote", "tag", "stash list"],
        ),
        ("cargo", &["check", "test", "clippy", "doc", "metadata", "tree"]),
    ];

    for (cmd_name, subcommands) in safe_prefixes {
        if first_word == *cmd_name {
            let rest = trimmed[cmd_name.len()..].trim();
            let sub = rest.split_whitespace().next().unwrap_or("");
            if subcommands.contains(&sub) {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_with_command(cmd: &str) -> serde_json::Value {
        serde_json::json!({ "command": cmd })
    }

    #[test]
    fn test_read_allowed() {
        assert_eq!(
            auto_classify("Read", &serde_json::json!({})),
            PermissionBehavior::Allow
        );
        assert_eq!(
            auto_classify("Glob", &serde_json::json!({})),
            PermissionBehavior::Allow
        );
        assert_eq!(
            auto_classify("Grep", &serde_json::json!({})),
            PermissionBehavior::Allow
        );
    }

    #[test]
    fn test_bash_readonly_allowed() {
        let cases = [
            "git status",
            "git log --oneline",
            "git diff HEAD",
            "git branch -a",
            "ls -la",
            "cat foo.txt",
            "head -n 10 file",
            "tail -f log.txt",
            "find . -name '*.rs'",
            "grep -r pattern .",
            "rg pattern",
            "which cargo",
            "pwd",
            "echo hello",
            "cargo check",
            "cargo test --lib",
            "cargo clippy",
        ];
        for cmd in cases {
            assert_eq!(
                auto_classify("Bash", &input_with_command(cmd)),
                PermissionBehavior::Allow,
                "Expected Allow for: {cmd}"
            );
        }
    }

    #[test]
    fn test_bash_mutating_asks() {
        let cases = [
            "rm -rf /tmp/stuff",
            "cargo build",
            "npm install",
            "sudo apt update",
            "mkdir newdir",
            "mv a b",
            "cp a b",
        ];
        for cmd in cases {
            assert_eq!(
                auto_classify("Bash", &input_with_command(cmd)),
                PermissionBehavior::Ask,
                "Expected Ask for: {cmd}"
            );
        }
    }

    #[test]
    fn test_write_asks() {
        assert_eq!(
            auto_classify("Write", &serde_json::json!({})),
            PermissionBehavior::Ask
        );
        assert_eq!(
            auto_classify("Edit", &serde_json::json!({})),
            PermissionBehavior::Ask
        );
    }
}
