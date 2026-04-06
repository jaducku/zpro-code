/// Extract a human-readable one-line summary from tool input JSON.
pub fn tool_input_summary(name: &str, input: &serde_json::Value) -> String {
    match name {
        "Bash" => input["command"].as_str()
            .map(|c| truncate_line(c, 80))
            .unwrap_or_default(),
        "Read" | "Write" => input["file_path"].as_str()
            .unwrap_or("").to_string(),
        "Edit" => input["file_path"].as_str()
            .unwrap_or("").to_string(),
        "Grep" => input["pattern"].as_str()
            .unwrap_or("").to_string(),
        "Glob" => input["pattern"].as_str()
            .unwrap_or("").to_string(),
        "Agent" => input["description"].as_str()
            .unwrap_or("").to_string(),
        "TaskCreate" | "TaskUpdate" => input["subject"].as_str()
            .unwrap_or("").to_string(),
        "WebFetch" => input["url"].as_str()
            .unwrap_or("").to_string(),
        "WebSearch" => input["query"].as_str()
            .unwrap_or("").to_string(),
        _ => {
            input.as_object()
                .and_then(|o| o.values().find_map(|v| v.as_str()))
                .map(|s| truncate_line(s, 60))
                .unwrap_or_default()
        }
    }
}

fn truncate_line(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.chars().count() > max {
        let truncated: String = first_line.chars().take(max).collect();
        format!("{}…", truncated)
    } else {
        first_line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_bash_summary() {
        let input = json!({"command": "ls -la"});
        assert_eq!(tool_input_summary("Bash", &input), "ls -la");
    }

    #[test]
    fn test_read_summary() {
        let input = json!({"file_path": "/src/main.rs"});
        assert_eq!(tool_input_summary("Read", &input), "/src/main.rs");
    }

    #[test]
    fn test_grep_summary() {
        let input = json!({"pattern": "fn main"});
        assert_eq!(tool_input_summary("Grep", &input), "fn main");
    }

    #[test]
    fn test_edit_summary() {
        let input = json!({"file_path": "/src/lib.rs", "old_string": "foo", "new_string": "bar"});
        assert_eq!(tool_input_summary("Edit", &input), "/src/lib.rs");
    }

    #[test]
    fn test_agent_summary() {
        let input = json!({"description": "Exploring codebase"});
        assert_eq!(tool_input_summary("Agent", &input), "Exploring codebase");
    }

    #[test]
    fn test_unknown_tool_fallback() {
        let input = json!({"some_field": "some_value"});
        let result = tool_input_summary("UnknownTool", &input);
        assert_eq!(result, "some_value");
    }

    #[test]
    fn test_truncate_line_short() {
        assert_eq!(truncate_line("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_line_long() {
        let long = "a".repeat(100);
        let result = truncate_line(&long, 10);
        assert_eq!(result.chars().count(), 11); // 10 + '…'
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_line_cjk() {
        let cjk = "한국어테스트문자열이길다";
        let result = truncate_line(cjk, 5);
        assert_eq!(result, "한국어테스…");
    }

    #[test]
    fn test_truncate_line_multiline() {
        assert_eq!(truncate_line("line1\nline2\nline3", 80), "line1");
    }

    #[test]
    fn test_empty_input() {
        let input = json!({});
        assert_eq!(tool_input_summary("Bash", &input), "");
    }
}
