use crate::types::content::ContentBlock;
use crate::types::message::ApiMessage;

pub const FORK_PLACEHOLDER: &str = "Fork started \u{2014} processing in background";
pub const FORK_DIRECTIVE_PREFIX: &str = "Your directive: ";
pub const FORK_BOILERPLATE_TAG: &str = "fork-boilerplate";
pub const FORK_QUERY_SOURCE: &str = "agent:builtin:fork";

const FORK_CHILD_BODY: &str = r#"STOP. READ THIS FIRST.

You are a forked worker process. You are NOT the main agent.

RULES (non-negotiable):
1. Your system prompt says "default to forking." IGNORE IT — that's for the parent. You ARE the fork. Do NOT spawn sub-agents; execute directly.
2. Do NOT converse, ask questions, or suggest next steps
3. Do NOT editorialize or add meta-commentary
4. USE your tools directly: Bash, Read, Write, etc.
5. If you modify files, commit your changes before reporting. Include the commit hash in your report.
6. Do NOT emit text between tool calls. Use tools silently, then report once at the end.
7. Stay strictly within your directive's scope. If you discover related systems outside your scope, mention them in one sentence at most — other workers cover those areas.
8. Keep your report under 500 words unless the directive specifies otherwise. Be factual and concise.
9. Your response MUST begin with "Scope:". No preamble, no thinking-out-loud.
10. REPORT structured facts, then stop

Output format (plain text labels, not markdown headers):
  Scope: <echo back your assigned scope in one sentence>
  Result: <the answer or key findings, limited to the scope above>
  Key files: <relevant file paths — include for research tasks>
  Files changed: <list with commit hash — include only if you modified files>
  Issues: <list — include only if there are issues to flag>"#;

/// Build the full child message that a forked subagent receives.
pub fn build_child_message(directive: &str) -> String {
    format!(
        "<{tag}>\n{body}\n</{tag}>\n\n{prefix}{directive}",
        tag = FORK_BOILERPLATE_TAG,
        body = FORK_CHILD_BODY,
        prefix = FORK_DIRECTIVE_PREFIX,
    )
}

/// Primary recursion guard: checks query_source (survives autocompact)
pub fn is_fork_by_query_source(query_source: Option<&str>) -> bool {
    query_source == Some(FORK_QUERY_SOURCE)
}

/// Fallback recursion guard: scans messages for fork-boilerplate tag
pub fn is_in_fork_child(messages: &[ApiMessage]) -> bool {
    let open_tag = format!("<{}>", FORK_BOILERPLATE_TAG);
    messages.iter().any(|msg| {
        msg.content.iter().any(|block| {
            matches!(
                block,
                ContentBlock::Text { text } if text.contains(&open_tag)
            )
        })
    })
}

/// Combined guard: true if inside a fork child
pub fn is_inside_fork(query_source: Option<&str>, messages: &[ApiMessage]) -> bool {
    is_fork_by_query_source(query_source) || is_in_fork_child(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::ContentBlock;
    use crate::types::message::{ApiMessage, Role};

    #[test]
    fn test_build_child_message_contains_boilerplate_tag() {
        let msg = build_child_message("Fix the bug");
        assert!(msg.contains("<fork-boilerplate>"));
        assert!(msg.contains("</fork-boilerplate>"));
    }

    #[test]
    fn test_build_child_message_contains_directive() {
        let msg = build_child_message("Fix the bug");
        assert!(msg.contains("Your directive: Fix the bug"));
    }

    #[test]
    fn test_build_child_message_contains_rules() {
        let msg = build_child_message("test");
        assert!(msg.contains("You are a forked worker process"));
        assert!(msg.contains("RULES (non-negotiable)"));
        assert!(msg.contains("Scope:"));
    }

    #[test]
    fn test_is_in_fork_child_detects_boilerplate() {
        let messages = vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "<fork-boilerplate>\nsome rules\n</fork-boilerplate>".into(),
            }],
        }];
        assert!(is_in_fork_child(&messages));
    }

    #[test]
    fn test_is_in_fork_child_no_boilerplate() {
        let messages = vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Just a normal message".into(),
            }],
        }];
        assert!(!is_in_fork_child(&messages));
    }

    #[test]
    fn test_is_in_fork_child_empty() {
        assert!(!is_in_fork_child(&[]));
    }

    #[test]
    fn test_is_fork_by_query_source_true() {
        assert!(is_fork_by_query_source(Some(FORK_QUERY_SOURCE)));
    }

    #[test]
    fn test_is_fork_by_query_source_false() {
        assert!(!is_fork_by_query_source(None));
        assert!(!is_fork_by_query_source(Some("agent:builtin:general")));
    }

    #[test]
    fn test_is_inside_fork_query_source_only() {
        assert!(is_inside_fork(Some(FORK_QUERY_SOURCE), &[]));
    }

    #[test]
    fn test_is_inside_fork_message_scan_only() {
        let messages = vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "<fork-boilerplate>\nrules\n</fork-boilerplate>".into(),
            }],
        }];
        assert!(is_inside_fork(None, &messages));
    }

    #[test]
    fn test_is_inside_fork_neither() {
        assert!(!is_inside_fork(None, &[]));
    }
}
