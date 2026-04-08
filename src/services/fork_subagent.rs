use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};

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
9. Your response MUST begin with "Scope:". Reasoning is acceptable; be direct.
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

/// Build the fork child's initial messages from the parent's assistant message.
pub fn build_forked_messages(directive: &str, _assistant_message: &ApiMessage) -> Vec<ApiMessage> {
    // Fresh start: don't clone parent's assistant message to avoid confusing the model.
    // Just provide the directive as a clean user message.
    // This prevents tool call XML parsing issues and ensures fork child starts fresh.
    vec![ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: build_child_message(directive),
        }],
    }]
}

pub fn build_worktree_notice(parent_cwd: &Path, worktree_cwd: &Path) -> String {
    format!(
        "You've inherited the conversation context above from a parent agent working in {}. \
         You are operating in an isolated git worktree at {} — same repository, same relative \
         file structure, separate working copy. Paths in the inherited context refer to the \
         parent's working directory; translate them to your worktree root. Re-read files before \
         editing if the parent may have modified them since they appear in the context. Your \
         changes stay in this worktree and will not affect the parent's files.",
        parent_cwd.display(),
        worktree_cwd.display(),
    )
}

pub async fn is_fork_enabled(app_state: &Arc<RwLock<crate::state::AppState>>) -> bool {
    let state = app_state.read().await;
    state.settings.fork_subagent.unwrap_or(false)
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

    use crate::types::content::ToolResultContent;

    #[test]
    fn test_build_forked_messages_with_tool_uses() {
        let assistant_msg = ApiMessage {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "I'll help.".into(),
                },
                ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "Read".into(),
                    input: serde_json::json!({}),
                },
                ContentBlock::ToolUse {
                    id: "tu_2".into(),
                    name: "Write".into(),
                    input: serde_json::json!({}),
                },
            ],
        };

        let result = build_forked_messages("Fix the bug", &assistant_msg);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, Role::Assistant);
        assert_eq!(result[0].content.len(), 3);
        assert_eq!(result[1].role, Role::User);
        assert_eq!(result[1].content.len(), 3); // 2 results + 1 text

        match &result[1].content[0] {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                assert_eq!(tool_use_id, "tu_1");
                assert_eq!(*content, ToolResultContent::Text(FORK_PLACEHOLDER.into()));
            }
            _ => panic!("Expected ToolResult"),
        }

        match &result[1].content[2] {
            ContentBlock::Text { text } => {
                assert!(text.contains("<fork-boilerplate>"));
                assert!(text.contains("Your directive: Fix the bug"));
            }
            _ => panic!("Expected Text"),
        }
    }

    #[test]
    fn test_build_forked_messages_no_tool_uses() {
        let assistant_msg = ApiMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::Text {
                text: "Just text".into(),
            }],
        };

        let result = build_forked_messages("Do something", &assistant_msg);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, Role::User);
        match &result[0].content[0] {
            ContentBlock::Text { text } => {
                assert!(text.contains("Your directive: Do something"));
            }
            _ => panic!("Expected Text"),
        }
    }

    #[test]
    fn test_build_worktree_notice() {
        let notice = build_worktree_notice(
            std::path::Path::new("/parent/dir"),
            std::path::Path::new("/worktree/dir"),
        );
        assert!(notice.contains("/parent/dir"));
        assert!(notice.contains("/worktree/dir"));
        assert!(notice.contains("isolated git worktree"));
    }

    #[tokio::test]
    async fn test_is_fork_enabled_true() {
        let mut settings = crate::utils::settings::SettingsJson::default();
        settings.fork_subagent = Some(true);
        let app_state = crate::state::AppState {
            settings,
            ..Default::default()
        };
        let state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));
        assert!(is_fork_enabled(&state).await);
    }

    #[tokio::test]
    async fn test_is_fork_enabled_default_false() {
        let app_state = crate::state::AppState::default();
        let state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));
        assert!(!is_fork_enabled(&state).await);
    }

    #[test]
    fn test_full_fork_message_flow() {
        let parent_messages = vec![
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "Help me fix a bug".into(),
                }],
            },
            ApiMessage {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Text {
                        text: "I'll investigate.".into(),
                    },
                    ContentBlock::ToolUse {
                        id: "tu_read".into(),
                        name: "Read".into(),
                        input: serde_json::json!({"path": "/src/main.rs"}),
                    },
                ],
            },
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_read".into(),
                    content: ToolResultContent::Text("fn main() {}".into()),
                    is_error: false,
                }],
            },
            ApiMessage {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Text {
                        text: "Let me spawn an agent.".into(),
                    },
                    ContentBlock::ToolUse {
                        id: "tu_agent".into(),
                        name: "Agent".into(),
                        input: serde_json::json!({"prompt": "fix bug"}),
                    },
                ],
            },
        ];

        // Last assistant has the Agent tool_use
        let last_assistant = parent_messages
            .iter()
            .rev()
            .find(|m| m.role == Role::Assistant)
            .unwrap();

        // Build fork messages
        let fork_msgs = build_forked_messages("Fix the bug in main.rs", last_assistant);
        assert_eq!(fork_msgs.len(), 2);

        // Verify fork child can't re-fork (recursion guard)
        assert!(is_in_fork_child(&fork_msgs));

        // Verify the directive is in the output
        let user_msg = &fork_msgs[1];
        let has_directive = user_msg.content.iter().any(|block| {
            matches!(
                block,
                ContentBlock::Text { text } if text.contains("Fix the bug in main.rs")
            )
        });
        assert!(has_directive);
    }
}
