use std::collections::HashSet;

use crate::query::types::QueryContext;
use crate::tool::{Tool, ToolUseContext};
use crate::tools::get_builtin_tools;

/// Tools allowed for background agents (read-only + search)
const BACKGROUND_ALLOWED_TOOLS: &[&str] = &[
    "Read",
    "Glob",
    "Grep",
    "WebFetch",
    "WebSearch",
    "ToolSearch",
    "TodoWrite",
    "ListMcpResources",
    "ReadMcpResource",
];

/// Parameters for spawning an agent
pub struct AgentSpawnParams {
    pub prompt: String,
    pub description: String,
    pub model: Option<String>,
    pub run_in_background: bool,
    pub isolation: Option<String>,
    /// Whether this is a fork subagent (inherits parent context)
    pub is_fork: bool,
    /// Pre-built fork messages (from build_forked_messages)
    pub fork_messages: Option<Vec<crate::types::message::ApiMessage>>,
    /// Query source for recursion guard (e.g., "agent:builtin:fork")
    pub query_source: Option<String>,
}

impl AgentSpawnParams {
    pub fn from_input(input: &serde_json::Value) -> Result<Self, crate::types::AtomError> {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Agent".into(),
                message: "Missing required 'prompt' parameter".into(),
            })?
            .to_string();

        let description = input
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("agent task")
            .to_string();

        let model = input
            .get("model")
            .and_then(|v| v.as_str())
            .map(String::from);

        let run_in_background = input
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let isolation = input
            .get("isolation")
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(Self {
            prompt,
            description,
            model,
            run_in_background,
            isolation,
            is_fork: false,
            fork_messages: None,
            query_source: None,
        })
    }
}

/// Result of a completed agent run
pub struct AgentResult {
    pub agent_id: String,
    pub output: String,
    pub is_error: bool,
    pub tool_count: u32,
    pub token_count: u64,
    pub duration_secs: u64,
}

/// Create a fresh set of tools for an agent.
pub fn get_agent_tools(is_background: bool) -> Vec<Box<dyn Tool>> {
    let all = get_builtin_tools();
    if !is_background {
        return all;
    }
    all.into_iter()
        .filter(|t| BACKGROUND_ALLOWED_TOOLS.contains(&t.name()))
        .collect()
}

/// Build a child QueryContext from the parent's ToolUseContext.
pub fn build_child_context(
    params: &AgentSpawnParams,
    parent_ctx: &ToolUseContext,
    tools: Vec<Box<dyn Tool>>,
) -> QueryContext {
    use crate::types::content::ContentBlock;
    use crate::types::message::{ApiMessage, Role};

    let model = params
        .model
        .clone()
        .unwrap_or_else(|| parent_ctx.model.clone());
    let max_tokens = parent_ctx.provider.model_info().max_output_tokens;

    let messages = vec![ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: params.prompt.clone(),
        }],
    }];

    QueryContext {
        messages,
        tools,
        system_prompt: Some(parent_ctx.system_prompt.clone()),
        model,
        max_tokens,
        permission_ctx: parent_ctx.permission_ctx.clone(),
        permission_tx: parent_ctx.permission_tx.clone(),
        question_tx: parent_ctx.question_tx.clone(),
        hooks: crate::services::hooks::load_all_hooks(&parent_ctx.cwd),
        cwd: parent_ctx.cwd.clone(),
        max_turns: Some(50),
        max_budget_usd: None,
        commands: parent_ctx.commands.clone(),
        provider: parent_ctx.provider.clone(),
        registry: None, // TODO: propagate parent_ctx.registry.clone() when subagents need multi-provider
        app_state: parent_ctx.app_state.clone(),
        mcp_manager: parent_ctx.mcp_manager.clone(),
        agent_depth: Some(parent_ctx.agent_depth + 1),
        query_source: parent_ctx.query_source.clone(),
        abort_signal: parent_ctx.abort_signal.child_signal(),
    }
}

/// Remove assistant messages that have tool_use blocks without matching tool_result responses.
/// Important because the parent's in-flight Agent tool_use won't have a result yet.
fn filter_incomplete_tool_calls(
    messages: &[crate::types::message::ApiMessage],
) -> Vec<crate::types::message::ApiMessage> {
    use crate::types::content::ContentBlock;
    use crate::types::message::Role;

    let answered_ids: HashSet<&str> = messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
            _ => None,
        })
        .collect();

    messages
        .iter()
        .filter(|msg| {
            if msg.role != Role::Assistant {
                return true;
            }
            !msg.content.iter().any(|block| {
                matches!(
                    block,
                    ContentBlock::ToolUse { id, .. } if !answered_ids.contains(id.as_str())
                )
            })
        })
        .cloned()
        .collect()
}

/// Build a QueryContext for fork children. Unlike `build_child_context`, this:
/// - Uses filtered parent history + fork messages (not just the prompt)
/// - Sets max_turns = 200 (fork agent limit)
/// - Propagates query_source from params
pub fn build_fork_context(
    params: &AgentSpawnParams,
    parent_ctx: &ToolUseContext,
    tools: Vec<Box<dyn Tool>>,
) -> QueryContext {
    let model = params
        .model
        .clone()
        .unwrap_or_else(|| parent_ctx.model.clone());
    let max_tokens = parent_ctx.provider.model_info().max_output_tokens;

    let mut messages = filter_incomplete_tool_calls(parent_ctx.messages.as_ref());
    if let Some(ref fork_msgs) = params.fork_messages {
        messages.extend(fork_msgs.clone());
    }

    QueryContext {
        messages,
        tools,
        system_prompt: Some(parent_ctx.system_prompt.clone()),
        model,
        max_tokens,
        permission_ctx: parent_ctx.permission_ctx.clone(),
        permission_tx: parent_ctx.permission_tx.clone(),
        question_tx: parent_ctx.question_tx.clone(),
        hooks: crate::services::hooks::load_all_hooks(&parent_ctx.cwd),
        cwd: parent_ctx.cwd.clone(),
        max_turns: Some(200),
        max_budget_usd: None,
        commands: parent_ctx.commands.clone(),
        provider: parent_ctx.provider.clone(),
        registry: None, // TODO: propagate parent_ctx.registry.clone() when subagents need multi-provider
        app_state: parent_ctx.app_state.clone(),
        mcp_manager: parent_ctx.mcp_manager.clone(),
        agent_depth: Some(parent_ctx.agent_depth + 1),
        query_source: params.query_source.clone(),
        abort_signal: parent_ctx.abort_signal.child_signal(),
    }
}

/// Extract the last assistant message text from a completed query's messages.
fn extract_agent_output(messages: &[crate::types::message::ApiMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|m| m.role == crate::types::message::Role::Assistant)
        .map(|m| {
            m.content
                .iter()
                .filter_map(|c| match c {
                    crate::types::content::ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "Agent completed with no text output.".into())
}

/// Create an isolated worktree for an agent. Returns the worktree path and session.
async fn create_agent_worktree(
    cwd: &std::path::Path,
    agent_id: &str,
) -> Result<
    (
        std::path::PathBuf,
        crate::services::worktree::WorktreeSession,
    ),
    String,
> {
    let slug = format!("agent-{}", &agent_id[..8.min(agent_id.len())]);
    let session = crate::services::worktree::WorktreeService::create(cwd, &slug, agent_id)
        .await
        .map_err(|e| format!("Failed to create worktree: {e}"))?;
    let worktree_path = session.worktree_path.clone();
    Ok((worktree_path, session))
}

/// Clean up a worktree after agent execution. Keeps if changes exist, removes if clean.
async fn cleanup_agent_worktree(session: &crate::services::worktree::WorktreeSession) {
    let uncommitted =
        crate::services::worktree::WorktreeService::count_uncommitted(&session.worktree_path)
            .await
            .unwrap_or(0);

    let new_commits = if let Some(ref head) = session.original_head_commit {
        crate::services::worktree::WorktreeService::count_new_commits(&session.worktree_path, head)
            .await
            .unwrap_or(0)
    } else {
        0
    };

    if uncommitted == 0 && new_commits == 0 {
        // No changes — safe to remove
        let _ = crate::services::worktree::WorktreeService::remove(session).await;
    }
    // If there are changes, keep the worktree for the user to review
}

/// Run an agent in foreground (blocking) mode.
/// Calls query() and waits for completion, returning the assistant's response.
pub async fn run_foreground(
    params: AgentSpawnParams,
    parent_ctx: &ToolUseContext,
) -> Result<AgentResult, crate::types::AtomError> {
    let agent_id = uuid::Uuid::new_v4().to_string();
    let mut tools = get_agent_tools(false);
    // Share parent's MCP tools with child agent
    if let Some(ref mcp) = parent_ctx.mcp_manager {
        tools.extend(mcp.get_all_tools().await);
    }
    // Create worktree if isolation requested
    let worktree_session = if params.isolation.as_deref() == Some("worktree") {
        match create_agent_worktree(&parent_ctx.cwd, &agent_id).await {
            Ok((worktree_path, session)) => Some((worktree_path, session)),
            Err(e) => {
                return Ok(AgentResult {
                    agent_id,
                    output: format!("Failed to create worktree: {e}"),
                    is_error: true,
                    tool_count: 0,
                    token_count: 0,
                    duration_secs: 0,
                });
            }
        }
    } else {
        None
    };

    let mut child_ctx = if params.is_fork {
        build_fork_context(&params, parent_ctx, tools)
    } else {
        build_child_context(&params, parent_ctx, tools)
    };
    if let Some((ref worktree_path, _)) = worktree_session {
        child_ctx.cwd = worktree_path.clone();
        // Inject worktree notice for fork children
        if params.is_fork {
            let notice = crate::services::fork_subagent::build_worktree_notice(
                &parent_ctx.cwd,
                worktree_path,
            );
            child_ctx.messages.push(crate::types::message::ApiMessage {
                role: crate::types::message::Role::User,
                content: vec![crate::types::content::ContentBlock::Text { text: notice }],
            });
        }
    }

    let start_time = std::time::Instant::now();

    // Use a separate event channel so child tool events don't leak to parent TUI.
    let (child_event_tx, mut child_event_rx) = tokio::sync::mpsc::unbounded_channel();
    let result = crate::query::engine::query(child_ctx, child_event_tx).await;

    // Collect stats
    let mut tool_count: u32 = 0;
    let mut token_count: u64 = 0;
    while let Ok(event) = child_event_rx.try_recv() {
        match event {
            crate::query::types::QueryEvent::ToolCallComplete { .. } => tool_count += 1,
            crate::query::types::QueryEvent::Usage {
                input_tokens,
                output_tokens,
            } => {
                token_count += input_tokens + output_tokens;
            }
            _ => {}
        }
    }
    let duration_secs = start_time.elapsed().as_secs();

    // Clean up worktree
    if let Some((_, ref session)) = worktree_session {
        cleanup_agent_worktree(session).await;
    }

    match result {
        Ok(query_result) => {
            let output = extract_agent_output(&query_result.messages);
            Ok(AgentResult {
                agent_id,
                output,
                is_error: false,
                tool_count,
                token_count,
                duration_secs,
            })
        }
        Err(e) => Ok(AgentResult {
            agent_id,
            output: format!("Agent failed: {}", e),
            is_error: true,
            tool_count,
            token_count,
            duration_secs,
        }),
    }
}

/// Run an agent in background (async) mode.
/// Spawns a tokio task, registers as BackgroundTask, returns immediately.
pub async fn run_background(
    params: AgentSpawnParams,
    parent_ctx: &ToolUseContext,
) -> Result<crate::tool::ToolResult, crate::types::AtomError> {
    let agent_id = uuid::Uuid::new_v4().to_string();
    let description = params.description.clone();
    let mut tools = if params.is_fork {
        get_agent_tools(false) // fork: full tool access
    } else {
        get_agent_tools(true) // normal background: restricted tools
    };
    // Share parent's MCP tools with child agent
    if let Some(ref mcp) = parent_ctx.mcp_manager {
        tools.extend(mcp.get_all_tools().await);
    }
    let cancel_token = crate::utils::abort_signal::AbortSignal::new();

    // Register as BackgroundTask in AppState
    {
        let mut state = parent_ctx.app_state.write().await;
        state.background_tasks.insert(
            agent_id.clone(),
            crate::state::BackgroundTask {
                id: agent_id.clone(),
                task_type: "agent".into(),
                command: Some(format!("Agent: {}", &description)),
                status: crate::state::BackgroundTaskStatus::Running,
                cancel_token: cancel_token.clone(),
                output: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
                created_at: chrono::Utc::now(),
            },
        );
    }

    // Create worktree if isolation requested (before spawn so errors can be returned immediately)
    let worktree_session = if params.isolation.as_deref() == Some("worktree") {
        match create_agent_worktree(&parent_ctx.cwd, &agent_id).await {
            Ok((worktree_path, session)) => Some((worktree_path, session)),
            Err(e) => {
                return Ok(crate::tool::ToolResult {
                    content: format!("Failed to create worktree: {e}"),
                    is_error: true,
                    content_blocks: None,
                });
            }
        }
    } else {
        None
    };

    let mut child_ctx = if params.is_fork {
        build_fork_context(&params, parent_ctx, tools)
    } else {
        build_child_context(&params, parent_ctx, tools)
    };
    // Background agents use their own independent cancel_token (not parent's child)
    child_ctx.abort_signal = cancel_token.clone();
    if let Some((ref worktree_path, _)) = worktree_session {
        child_ctx.cwd = worktree_path.clone();
        // Inject worktree notice for fork children
        if params.is_fork {
            let notice = crate::services::fork_subagent::build_worktree_notice(
                &parent_ctx.cwd,
                worktree_path,
            );
            child_ctx.messages.push(crate::types::message::ApiMessage {
                role: crate::types::message::Role::User,
                content: vec![crate::types::content::ContentBlock::Text { text: notice }],
            });
        }
    }

    let parent_event_tx = parent_ctx.event_tx.clone();
    let app_state = parent_ctx.app_state.clone();
    let agent_id_clone = agent_id.clone();
    let description_clone = description.clone();

    tokio::spawn(async move {
        let start_time = std::time::Instant::now();

        // Use a separate event channel so child tool events don't appear in parent TUI.
        // We drain it to collect stats (tool count, tokens).
        let (child_event_tx, mut child_event_rx) = tokio::sync::mpsc::unbounded_channel();
        let result = crate::query::engine::query(child_ctx, child_event_tx).await;

        // Collect stats from child events
        let mut tool_count: u32 = 0;
        let mut token_count: u64 = 0;
        while let Ok(event) = child_event_rx.try_recv() {
            match event {
                crate::query::types::QueryEvent::ToolCallComplete { .. } => tool_count += 1,
                crate::query::types::QueryEvent::Usage {
                    input_tokens,
                    output_tokens,
                } => {
                    token_count += input_tokens + output_tokens;
                }
                _ => {}
            }
        }
        let duration_secs = start_time.elapsed().as_secs();

        let (result_text, is_error) = match result {
            Ok(query_result) => {
                let output = extract_agent_output(&query_result.messages);
                (output, false)
            }
            Err(e) => (format!("Agent failed: {}", e), true),
        };

        // Clean up worktree
        if let Some((_, ref session)) = worktree_session {
            cleanup_agent_worktree(session).await;
        }

        // Update BackgroundTask status and store pending result
        {
            let mut state = app_state.write().await;
            if let Some(task) = state.background_tasks.get_mut(&agent_id_clone) {
                task.status = if is_error {
                    crate::state::BackgroundTaskStatus::Failed
                } else {
                    crate::state::BackgroundTaskStatus::Completed
                };
                *task.output.lock().unwrap() = result_text.clone();
            }

            state
                .pending_agent_results
                .push(crate::state::app_state::PendingAgentResult {
                    agent_id: agent_id_clone.clone(),
                    description: description_clone.clone(),
                    result: result_text.clone(),
                    is_error,
                });
        }

        // Notify parent TUI with stats
        let _ = parent_event_tx.send(crate::query::types::QueryEvent::AgentCompleted {
            agent_id: agent_id_clone,
            description: description_clone,
            result: Some(result_text),
            is_error,
            tool_count,
            token_count,
            duration_secs,
        });
    });

    // Return immediately
    let response = serde_json::json!({
        "status": "background_started",
        "agent_id": agent_id,
        "description": description,
        "note": "Agent is running in the background. You will be notified when it completes."
    });

    Ok(crate::tool::ToolResult {
        content: serde_json::to_string_pretty(&response).unwrap_or_default(),
        is_error: false,
        content_blocks: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_agent_tools_foreground_includes_all() {
        let tools = get_agent_tools(false);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"Agent"), "foreground should include Agent");
        assert!(names.contains(&"Write"), "foreground should include Write");
        assert!(names.contains(&"Read"), "foreground should include Read");
    }

    #[test]
    fn test_get_agent_tools_background_is_restricted() {
        let tools = get_agent_tools(true);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"Read"), "background should include Read");
        assert!(names.contains(&"Grep"), "background should include Grep");
        assert!(names.contains(&"Glob"), "background should include Glob");
        assert!(!names.contains(&"Agent"), "background should exclude Agent");
        assert!(!names.contains(&"Write"), "background should exclude Write");
        assert!(!names.contains(&"Edit"), "background should exclude Edit");
    }

    #[test]
    fn test_agent_spawn_params_from_input() {
        let input = serde_json::json!({
            "prompt": "Fix the bug",
            "description": "fix bug",
            "model": "gpt-4o",
            "run_in_background": true,
            "isolation": "worktree"
        });
        let params = AgentSpawnParams::from_input(&input).unwrap();
        assert_eq!(params.prompt, "Fix the bug");
        assert_eq!(params.description, "fix bug");
        assert_eq!(params.model, Some("gpt-4o".into()));
        assert!(params.run_in_background);
        assert_eq!(params.isolation, Some("worktree".into()));
    }

    #[test]
    fn test_agent_spawn_params_defaults() {
        let input = serde_json::json!({ "prompt": "Do something" });
        let params = AgentSpawnParams::from_input(&input).unwrap();
        assert_eq!(params.description, "agent task");
        assert_eq!(params.model, None);
        assert!(!params.run_in_background);
        assert_eq!(params.isolation, None);
    }

    #[test]
    fn test_agent_spawn_params_missing_prompt() {
        let input = serde_json::json!({});
        assert!(AgentSpawnParams::from_input(&input).is_err());
    }

    #[test]
    fn test_build_child_context() {
        let parent_ctx = ToolUseContext::test(serde_json::json!({}));
        let params = AgentSpawnParams {
            prompt: "test prompt".into(),
            description: "test".into(),
            model: Some("custom-model".into()),
            run_in_background: false,
            isolation: None,
            is_fork: false,
            fork_messages: None,
            query_source: None,
        };
        let tools = get_agent_tools(false);
        let child = build_child_context(&params, &parent_ctx, tools);

        assert_eq!(child.model, "custom-model");
        assert_eq!(child.messages.len(), 1);
        assert_eq!(child.max_turns, Some(50));
        assert_eq!(child.agent_depth, Some(1));
    }

    #[test]
    fn test_build_child_context_inherits_parent_model() {
        let parent_ctx = ToolUseContext::test(serde_json::json!({}));
        let params = AgentSpawnParams {
            prompt: "test".into(),
            description: "test".into(),
            model: None,
            run_in_background: false,
            isolation: None,
            is_fork: false,
            fork_messages: None,
            query_source: None,
        };
        let tools = get_agent_tools(false);
        let child = build_child_context(&params, &parent_ctx, tools);

        assert_eq!(child.model, "test"); // inherited from parent_ctx.model
    }

    #[test]
    fn test_filter_incomplete_tool_calls_removes_orphans() {
        use crate::types::content::{ContentBlock, ToolResultContent};
        use crate::types::message::{ApiMessage, Role};

        let messages = vec![
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "Read".into(),
                    input: serde_json::json!({}),
                }],
            },
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: ToolResultContent::Text("ok".into()),
                    is_error: false,
                }],
            },
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_2".into(),
                    name: "Agent".into(),
                    input: serde_json::json!({}),
                }],
            },
        ];

        let filtered = filter_incomplete_tool_calls(&messages);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].role, Role::Assistant);
        assert_eq!(filtered[1].role, Role::User);
    }

    #[test]
    fn test_filter_incomplete_tool_calls_keeps_complete() {
        use crate::types::content::{ContentBlock, ToolResultContent};
        use crate::types::message::{ApiMessage, Role};

        let messages = vec![
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "Read".into(),
                    input: serde_json::json!({}),
                }],
            },
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: ToolResultContent::Text("ok".into()),
                    is_error: false,
                }],
            },
        ];

        let filtered = filter_incomplete_tool_calls(&messages);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_agent_spawn_params_fork_fields() {
        let input = serde_json::json!({ "prompt": "test" });
        let params = AgentSpawnParams::from_input(&input).unwrap();
        assert!(!params.is_fork);
        assert!(params.fork_messages.is_none());
        assert!(params.query_source.is_none());
    }

    #[test]
    fn test_build_fork_context_fields() {
        use crate::types::content::ContentBlock;
        use crate::types::message::{ApiMessage, Role};

        let parent_ctx = ToolUseContext::test(serde_json::json!({}));
        let fork_msgs = vec![
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "cloned".into(),
                }],
            },
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "directive".into(),
                }],
            },
        ];
        let params = AgentSpawnParams {
            prompt: "test".into(),
            description: "test".into(),
            model: None,
            run_in_background: false,
            isolation: None,
            is_fork: true,
            fork_messages: Some(fork_msgs),
            query_source: Some("agent:builtin:fork".into()),
        };
        let tools = get_agent_tools(false);
        let ctx = build_fork_context(&params, &parent_ctx, tools);

        assert_eq!(ctx.messages.len(), 2);
        assert_eq!(ctx.max_turns, Some(200));
        assert_eq!(ctx.agent_depth, Some(1));
        assert_eq!(ctx.query_source, Some("agent:builtin:fork".into()));
    }
}
