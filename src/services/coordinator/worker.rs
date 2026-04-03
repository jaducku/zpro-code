//! Worker agent notification formatting.

/// Format a task notification as XML for delivery to the coordinator.
pub fn format_task_notification(
    agent_id: &str,
    status: &str,
    summary: &str,
    result: Option<&str>,
    total_tokens: Option<u64>,
    tool_uses: Option<u64>,
    duration_ms: Option<u64>,
) -> String {
    let mut xml = String::from("<task-notification>\n");
    xml.push_str(&format!("<task-id>{agent_id}</task-id>\n"));
    xml.push_str(&format!("<status>{status}</status>\n"));
    xml.push_str(&format!("<summary>{summary}</summary>\n"));

    if let Some(r) = result {
        xml.push_str(&format!("<result>{r}</result>\n"));
    }

    if total_tokens.is_some() || tool_uses.is_some() || duration_ms.is_some() {
        xml.push_str("<usage>\n");
        if let Some(t) = total_tokens {
            xml.push_str(&format!("  <total_tokens>{t}</total_tokens>\n"));
        }
        if let Some(u) = tool_uses {
            xml.push_str(&format!("  <tool_uses>{u}</tool_uses>\n"));
        }
        if let Some(d) = duration_ms {
            xml.push_str(&format!("  <duration_ms>{d}</duration_ms>\n"));
        }
        xml.push_str("</usage>\n");
    }

    xml.push_str("</task-notification>");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_task_notification() {
        let xml = format_task_notification(
            "agent-a1b",
            "completed",
            "Agent \"Investigate auth bug\" completed",
            Some("Found null pointer in src/auth/validate.ts:42"),
            Some(1500),
            Some(12),
            Some(5432),
        );

        assert!(xml.contains("<task-id>agent-a1b</task-id>"));
        assert!(xml.contains("<status>completed</status>"));
        assert!(xml.contains("<summary>"));
        assert!(xml.contains("<result>Found null pointer"));
        assert!(xml.contains("<total_tokens>1500</total_tokens>"));
        assert!(xml.contains("<tool_uses>12</tool_uses>"));
        assert!(xml.contains("<duration_ms>5432</duration_ms>"));
        assert!(xml.starts_with("<task-notification>"));
        assert!(xml.ends_with("</task-notification>"));
    }

    #[test]
    fn test_format_task_notification_minimal() {
        let xml = format_task_notification(
            "agent-xyz",
            "failed",
            "Agent failed",
            None,
            None,
            None,
            None,
        );

        assert!(xml.contains("<task-id>agent-xyz</task-id>"));
        assert!(xml.contains("<status>failed</status>"));
        assert!(!xml.contains("<result>"));
        assert!(!xml.contains("<usage>"));
    }
}
