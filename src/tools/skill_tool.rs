use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, RwLock};

use crate::commands::context::{CommandContext, CommandOptions};
use crate::commands::traits::Command;
use crate::commands::types::ContextMode;
use crate::query::engine::query;
use crate::query::types::QueryContext;
use crate::state::AppState;
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};

pub struct SkillTool;

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        "Execute a skill within the main conversation.\n\n\
         When users ask you to perform tasks, check if any of the available skills match. \
         Skills provide specialized capabilities and domain knowledge.\n\n\
         When users reference a \"slash command\" or \"/<something>\" (e.g., \"/commit\", \"/review-pr\"), \
         they are referring to a skill. Use this tool to invoke it.\n\n\
         How to invoke:\n\
         - Use this tool with the skill name and optional arguments\n\
         - Examples:\n\
           - skill: \"pdf\" - invoke the pdf skill\n\
           - skill: \"commit\", args: \"-m 'Fix bug'\" - invoke with arguments\n\n\
         Important:\n\
         - Available skills are listed in system-reminder messages in the conversation\n\
         - When a skill matches the user's request, this is a BLOCKING REQUIREMENT: \
         invoke the relevant Skill tool BEFORE generating any other response about the task\n\
         - NEVER mention a skill without actually calling this tool\n\
         - Do not invoke a skill that is already running\n\
         - Do not use this tool for built-in CLI commands (like /help, /clear, etc.)\n\
         - If you see a <command-name> tag in the current conversation turn, the skill has \
         ALREADY been loaded - follow the instructions directly instead of calling this tool again"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "The skill name. E.g., \"commit\", \"review-pr\", or \"pdf\""
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the skill"
                }
            },
            "required": ["skill"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let skill_name = ctx
            .input
            .get("skill")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Skill".into(),
                message: "Missing required 'skill' parameter".into(),
            })?;
        let args = ctx
            .input
            .get("args")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Look up skill in registry
        let command = ctx.commands.find(skill_name);
        let prompt_cmd = match command {
            Some(Command::Prompt(cmd)) => cmd,
            _ => {
                return Ok(ToolResult {
                    content: format!(
                        "Skill '{}' not found. Check available skills in the system-reminder.",
                        skill_name
                    ),
                    is_error: true,
                    content_blocks: None,
                });
            }
        };

        let context_mode = prompt_cmd.context_mode();

        // Build a CommandContext for get_prompt()
        let (msg_tx, _) = mpsc::unbounded_channel();
        let (ui_tx, _) = mpsc::unbounded_channel();
        let cmd_ctx = CommandContext {
            app_state: Arc::new(RwLock::new(AppState::default())),
            abort_signal: ctx.abort_signal.child_token(),
            cwd: ctx.cwd.clone(),
            message_tx: msg_tx,
            ui_event_tx: ui_tx,
            options: CommandOptions {
                theme: "default".to_string(),
            },
        };

        let blocks = match prompt_cmd.get_prompt(args, &cmd_ctx).await {
            Ok(b) => b,
            Err(e) => {
                return Ok(ToolResult {
                    content: format!("Failed to load skill '{}': {}", skill_name, e),
                    is_error: true,
                    content_blocks: None,
                });
            }
        };

        let prompt_text: String = blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if prompt_text.is_empty() {
            return Ok(ToolResult {
                content: format!("Skill '{}' produced no content.", skill_name),
                is_error: true,
                content_blocks: None,
            });
        }

        match context_mode {
            ContextMode::Inline => Ok(ToolResult {
                content: format!(
                    "<command-name>{}</command-name>\n{}",
                    skill_name, prompt_text
                ),
                is_error: false,
                content_blocks: None,
            }),
            ContextMode::Fork => execute_forked_skill(skill_name, &prompt_text, &ctx).await,
        }
    }

    fn is_read_only(&self) -> bool {
        false
    }
}

async fn execute_forked_skill(
    skill_name: &str,
    prompt_text: &str,
    parent_ctx: &ToolUseContext,
) -> crate::types::Result<ToolResult> {
    let child_abort = parent_ctx.abort_signal.child_token();
    let event_tx = parent_ctx.event_tx.clone();

    let query_ctx = QueryContext {
        messages: vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: prompt_text.to_string(),
            }],
        }],
        // Box<dyn Tool> isn't Clone — create fresh tool instances.
        tools: {
            let mut t = crate::tools::get_builtin_tools();
            if let Some(ref mcp) = parent_ctx.mcp_manager {
                t.extend(mcp.get_all_tools().await);
            }
            t
        },
        system_prompt: Some(parent_ctx.system_prompt.clone()),
        model: parent_ctx.model.clone(),
        max_tokens: 16_384,
        permission_ctx: parent_ctx.permission_ctx.clone(),
        permission_tx: parent_ctx.permission_tx.clone(),
        question_tx: parent_ctx.question_tx.clone(),
        hooks: std::collections::HashMap::new(),
        cwd: parent_ctx.cwd.clone(),
        max_turns: Some(25),
        max_budget_usd: None,
        commands: parent_ctx.commands.clone(),
        provider: parent_ctx.provider.clone(),
        app_state: parent_ctx.app_state.clone(),
        mcp_manager: parent_ctx.mcp_manager.clone(),
        agent_depth: None,
    };

    let result = query(query_ctx, event_tx, child_abort).await;

    match result {
        Ok(qr) => {
            let text = qr
                .messages
                .iter()
                .rev()
                .find(|m| m.role == Role::Assistant)
                .and_then(|m| {
                    m.content.iter().find_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| {
                    format!("Skill '{}' completed with no text output.", skill_name)
                });
            Ok(ToolResult {
                content: text,
                is_error: false,
                content_blocks: None,
            })
        }
        Err(e) => Ok(ToolResult {
            content: format!("Skill '{}' failed: {}", skill_name, e),
            is_error: true,
            content_blocks: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_tool_name() {
        assert_eq!(SkillTool.name(), "Skill");
    }

    #[test]
    fn test_skill_tool_schema() {
        let schema = SkillTool.input_schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("skill")));
    }

    #[tokio::test]
    async fn test_skill_not_found() {
        let ctx = ToolUseContext::test(serde_json::json!({ "skill": "nonexistent" }));
        let result = SkillTool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_skill_missing_param() {
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = SkillTool.execute(ctx).await;
        assert!(result.is_err());
    }
}
