use std::sync::Arc;

use crate::services::api::types::*;
use crate::services::hooks::types::HookContext;
use crate::services::hooks::{execute_event_hooks, HookOutcome};
use crate::tool::{ToolCall, ToolProgress, ToolResultBlock, ToolUseContext};
use crate::tools::find_tool_by_name;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role, StopReason};
use crate::types::permissions::{PermissionMeta, PermissionRule};
use crate::utils::permissions::{check_permission, PermissionDecision};
use futures::StreamExt;
use tokio::sync::mpsc;

use super::types::{PermissionAsk, PermissionReply, QueryContext, QueryEvent, QueryResult};

/// Maximum number of tool-use loops before forcing stop
const MAX_TOOL_LOOPS: usize = 50;

/// Execute the query loop: LLM call -> tool execution -> repeat until done
pub async fn query(
    mut ctx: QueryContext,
    event_tx: mpsc::UnboundedSender<QueryEvent>,
) -> crate::types::Result<QueryResult> {
    let tools_arc = Arc::new(std::mem::take(&mut ctx.tools));
    let mut loop_count = 0;
    let max_loops = ctx.max_turns.map(|t| t as usize).unwrap_or(MAX_TOOL_LOOPS);
    let mut total_cost: f64 = 0.0;
    let mut compact_failures: u32 = 0;
    let mut already_retried = false;

    loop {
        if loop_count >= max_loops {
            let _ = event_tx.send(QueryEvent::Error(format!(
                "Maximum turns reached ({})",
                max_loops
            )));
            break;
        }
        loop_count += 1;

        // Check budget limit
        if let Some(budget) = ctx.max_budget_usd {
            if total_cost >= budget {
                let _ = event_tx.send(QueryEvent::Error(format!(
                    "Budget limit reached (${:.4} >= ${:.4})",
                    total_cost, budget
                )));
                break;
            }
        }

        // 0. Compaction pipeline
        let cw = ctx.provider.model_info().context_window;

        // Stage 1: Snip
        if crate::services::compact::token_ratio(&ctx.messages, cw) > 0.60 {
            let freed = crate::services::compact::snip::snip(&mut ctx.messages, 3);
            if freed > 0 {
                let _ = event_tx.send(QueryEvent::Compacted {
                    stage: "snip".into(),
                    tokens_freed: freed,
                });
            }
        }

        // Stage 2: Microcompact
        if crate::services::compact::token_ratio(&ctx.messages, cw) > 0.75 {
            let freed = crate::services::compact::microcompact::microcompact(&mut ctx.messages, 3);
            if freed > 0 {
                let _ = event_tx.send(QueryEvent::Compacted {
                    stage: "microcompact".into(),
                    tokens_freed: freed,
                });
            }
        }

        // Stage 3: Full Compact (LLM call)
        if crate::services::compact::token_ratio(&ctx.messages, cw) > 0.85 && compact_failures < 3 {
            match crate::services::compact::full_compact::full_compact(
                &ctx.messages,
                ctx.provider.as_ref(),
                cw,
                3,
                None,
                &ctx.abort_signal,
            )
            .await
            {
                Ok(result) => {
                    let freed = result.tokens_freed;
                    ctx.messages = result.messages;
                    compact_failures = 0;
                    let _ = event_tx.send(QueryEvent::Compacted {
                        stage: "full".into(),
                        tokens_freed: freed,
                    });
                }
                Err(e) => {
                    compact_failures += 1;
                    let _ = event_tx.send(QueryEvent::Error(format!(
                        "Compaction failed ({}/3): {}",
                        compact_failures, e
                    )));
                }
            }
        }

        // Hard block — only after all proactive stages exhausted
        if crate::services::compact::token_ratio(&ctx.messages, cw) > 0.95 {
            let _ = event_tx.send(QueryEvent::Error(
                "Context window 95% exceeded after all compaction. Cannot continue.".into(),
            ));
            break;
        }

        // 1. Build request
        let request = CreateMessageRequest {
            model: ctx.model.clone(),
            messages: ctx.messages.clone(),
            tools: Some(
                tools_arc
                    .iter()
                    .map(|t| crate::tool::ToolDefinition::from(t.as_ref()))
                    .collect(),
            ),
            max_tokens: ctx.max_tokens,
            system: ctx.system_prompt.clone(),
            temperature: None,
            stream: true,
        };

        // 2. Stream response
        let mut stream = match ctx.provider.create_message(request).await {
            Ok(s) => s,
            Err(e) if is_context_overflow_error(&e) && !already_retried => {
                // Reactive compact: run full pipeline and retry
                let cw = ctx.provider.model_info().context_window;
                let _ = crate::services::compact::snip::snip(&mut ctx.messages, 3);
                let _ = crate::services::compact::microcompact::microcompact(&mut ctx.messages, 3);
                if let Ok(result) = crate::services::compact::full_compact::full_compact(
                    &ctx.messages,
                    ctx.provider.as_ref(),
                    cw,
                    3,
                    None,
                    &ctx.abort_signal,
                )
                .await
                {
                    ctx.messages = result.messages;
                    let _ = event_tx.send(QueryEvent::Compacted {
                        stage: "reactive".into(),
                        tokens_freed: result.tokens_freed,
                    });
                }
                already_retried = true;
                continue; // retry the outer loop
            }
            Err(e) => return Err(e),
        };

        let mut text_content = String::new();
        let mut reasoning_content = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut stop_reason = StopReason::EndTurn;
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_input = String::new();
        let mut stream_failed = false;

        // 5-minute timeout between stream chunks — if API stops sending data, don't hang forever.
        let chunk_timeout = std::time::Duration::from_secs(300);

        loop {
            if ctx.abort_signal.is_cancelled() {
                let reason = ctx
                    .abort_signal
                    .reason()
                    .cloned()
                    .unwrap_or(crate::utils::abort_signal::AbortReason::UserCancel);
                match reason {
                    crate::utils::abort_signal::AbortReason::Interrupt => {
                        // For Interrupt, we still cancel the streaming phase.
                        // Per-tool behavior is checked in execute_tool, not here.
                        if !text_content.is_empty() {
                            let partial_msg = crate::types::message::ApiMessage {
                                role: crate::types::message::Role::Assistant,
                                content: vec![crate::types::content::ContentBlock::Text {
                                    text: text_content.clone(),
                                }],
                            };
                            ctx.messages.push(partial_msg);
                        }
                        let _ = event_tx.send(QueryEvent::QueryCancelled {
                            partial_messages: ctx.messages.clone(),
                            reason,
                        });
                        return Err(crate::types::AtomError::Cancelled);
                    }
                    _ => {
                        // UserCancel, Background, SiblingError → always cancel immediately
                        if !text_content.is_empty() {
                            let partial_msg = crate::types::message::ApiMessage {
                                role: crate::types::message::Role::Assistant,
                                content: vec![crate::types::content::ContentBlock::Text {
                                    text: text_content.clone(),
                                }],
                            };
                            ctx.messages.push(partial_msg);
                        }
                        let _ = event_tx.send(QueryEvent::QueryCancelled {
                            partial_messages: ctx.messages.clone(),
                            reason,
                        });
                        return Err(crate::types::AtomError::Cancelled);
                    }
                }
            }

            let next = tokio::time::timeout(chunk_timeout, stream.next()).await;
            let event = match next {
                Ok(Some(Ok(event))) => event,
                Ok(Some(Err(e))) => {
                    let _ = event_tx.send(QueryEvent::Error(format!("Stream error: {}", e)));
                    stream_failed = true;
                    break;
                }
                Ok(None) => break, // stream ended normally
                Err(_) => {
                    let _ = event_tx.send(QueryEvent::Error(
                        "Stream timeout: no response from API for 300 seconds".into(),
                    ));
                    stream_failed = true;
                    break;
                }
            };

            match event {
                StreamEvent::ContentBlockStart {
                    content_block: ContentBlock::ToolUse { id, name, .. },
                    ..
                } => {
                    current_tool_id = id.clone();
                    current_tool_name = name.clone();
                    current_tool_input.clear();
                    let _ = event_tx.send(QueryEvent::ToolCallStart {
                        id: id.clone(),
                        name: name.clone(),
                    });
                }
                StreamEvent::ContentBlockStart { .. } => {}
                StreamEvent::ContentBlockDelta { delta, .. } => match delta {
                    ContentDelta::TextDelta { text } => {
                        text_content.push_str(&text);
                        let _ = event_tx.send(QueryEvent::TextDelta(text));
                    }
                    ContentDelta::InputJsonDelta { partial_json } => {
                        current_tool_input.push_str(&partial_json);
                    }
                    ContentDelta::ThinkingDelta { thinking } => {
                        reasoning_content.push_str(&thinking);
                    }
                },
                StreamEvent::ContentBlockStop { .. } => {
                    if !current_tool_id.is_empty() {
                        let input: serde_json::Value =
                            match serde_json::from_str(&current_tool_input) {
                                Ok(v) => v,
                                Err(e) => {
                                    let _ = event_tx.send(QueryEvent::Error(format!(
                                        "Malformed tool input for '{}': {}",
                                        current_tool_name, e
                                    )));
                                    serde_json::Value::Object(serde_json::Map::new())
                                }
                            };
                        tool_calls.push(ToolCall {
                            id: current_tool_id.clone(),
                            name: current_tool_name.clone(),
                            input: input.clone(),
                        });
                        let _ = event_tx.send(QueryEvent::ToolCallInputReady {
                            id: current_tool_id.clone(),
                            name: current_tool_name.clone(),
                            input,
                        });
                        current_tool_id.clear();
                        current_tool_name.clear();
                        current_tool_input.clear();
                    }
                }
                StreamEvent::MessageStop { stop_reason: sr } => {
                    stop_reason = sr;
                }
                StreamEvent::Usage(usage) => {
                    // Track cost for budget limiting (~$0.01 per 1K tokens rough avg)
                    total_cost += (usage.input_tokens + usage.output_tokens) as f64 * 0.00001;
                    let _ = event_tx.send(QueryEvent::Usage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                    });
                }
                _ => {}
            }
        }

        // Stream error/timeout → stop the query loop, send partial results
        if stream_failed {
            // Still save any partial text as assistant message
            if !text_content.is_empty() {
                ctx.messages.push(ApiMessage {
                    role: Role::Assistant,
                    content: vec![ContentBlock::Text { text: text_content }],
                });
            }
            let _ = event_tx.send(QueryEvent::ResponseComplete {
                stop_reason: StopReason::EndTurn,
            });
            let _ = event_tx.send(QueryEvent::QueryCompleted {
                messages: ctx.messages.clone(),
            });
            break;
        }

        // Flush any in-progress tool call (stream ended without ContentBlockStop)
        if !current_tool_id.is_empty() {
            let _ = event_tx.send(QueryEvent::Error(format!(
                "Incomplete tool call for '{}' (stream ended early)",
                current_tool_name
            )));
            let input: serde_json::Value = match serde_json::from_str(&current_tool_input) {
                Ok(v) => v,
                Err(_) => serde_json::Value::Object(serde_json::Map::new()),
            };
            tool_calls.push(ToolCall {
                id: current_tool_id.clone(),
                name: current_tool_name.clone(),
                input: input.clone(),
            });
            let _ = event_tx.send(QueryEvent::ToolCallInputReady {
                id: current_tool_id.clone(),
                name: current_tool_name.clone(),
                input,
            });
            current_tool_id.clear();
        }

        // If model returned only reasoning/thinking content (e.g., Qwen 3, DeepSeek)
        // with no text content, use the reasoning as the response.
        // Skip if it looks like internal metadata (no spaces, all camelCase/snake_case).
        if text_content.is_empty() && !reasoning_content.is_empty() && tool_calls.is_empty() {
            let trimmed = reasoning_content.trim();
            let has_spaces = trimmed.contains(' ');
            let is_short_garbage = !has_spaces && trimmed.len() > 20;
            if !is_short_garbage {
                let _ = event_tx.send(QueryEvent::TextDelta(reasoning_content.clone()));
                text_content = std::mem::take(&mut reasoning_content);
            } else {
                tracing::warn!(
                    "Discarded suspicious reasoning content: {}...",
                    &trimmed[..trimmed.len().min(80)]
                );
                reasoning_content.clear();
            }
        }

        // Empty response: no text, no tools — notify and stop
        if text_content.is_empty() && tool_calls.is_empty() {
            let _ = event_tx.send(QueryEvent::Error(
                "Empty response from model (no text, no tool calls)".into(),
            ));
            let _ = event_tx.send(QueryEvent::ResponseComplete {
                stop_reason: StopReason::EndTurn,
            });
            let _ = event_tx.send(QueryEvent::QueryCompleted {
                messages: ctx.messages.clone(),
            });
            break;
        }

        // 3. Build assistant message content
        let mut assistant_content: Vec<ContentBlock> = Vec::new();
        if !text_content.is_empty() {
            assistant_content.push(ContentBlock::Text {
                text: text_content.clone(),
            });
        }
        for tc in &tool_calls {
            assistant_content.push(ContentBlock::ToolUse {
                id: tc.id.clone(),
                name: tc.name.clone(),
                input: tc.input.clone(),
            });
        }

        // Add assistant message to conversation
        ctx.messages.push(ApiMessage {
            role: Role::Assistant,
            content: assistant_content,
        });

        // 4. If no tool calls, we're done
        if tool_calls.is_empty() {
            let _ = event_tx.send(QueryEvent::ResponseComplete {
                stop_reason: stop_reason.clone(),
            });
            return Ok(QueryResult {
                messages: ctx.messages,
                stop_reason,
                tool_results: Vec::new(),
            });
        }

        // 5. Execute tools (with permission checks)
        let mut tool_result_blocks = Vec::new();
        for tc in &tool_calls {
            // Permission check
            let tool = find_tool_by_name(&tools_arc, &tc.name);
            let meta = tool
                .map(|t| t.permission_metadata(&tc.input))
                .unwrap_or(PermissionMeta {
                    file_path: None,
                    command: None,
                    url: None,
                });

            let decision = check_permission(&tc.name, &tc.input, &meta, &ctx.permission_ctx);

            let should_execute = match decision {
                PermissionDecision::Allow { .. } => true,
                PermissionDecision::Deny { reason } => {
                    let block = ToolResultBlock::error(
                        tc.id.clone(),
                        format!("Permission denied: {}", reason),
                    );
                    let _ = event_tx.send(QueryEvent::ToolCallComplete {
                        id: tc.id.clone(),
                        result: block.clone(),
                    });
                    tool_result_blocks.push(block);
                    continue;
                }
                PermissionDecision::Ask {
                    tool_name,
                    description,
                } => {
                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                    let _ = ctx.permission_tx.send(PermissionAsk {
                        tool_use_id: tc.id.clone(),
                        tool_name: tool_name.clone(),
                        description,
                        reply_tx,
                    });
                    let reply = tokio::select! {
                        reply = reply_rx => reply.unwrap_or(PermissionReply::Deny),
                        _ = ctx.abort_signal.cancelled() => PermissionReply::Deny,
                    };
                    match reply {
                        PermissionReply::Allow => true,
                        PermissionReply::AllowAlways {
                            tool_name,
                            rule_content,
                        } => {
                            ctx.permission_ctx.always_allow_rules.push(PermissionRule {
                                tool_name,
                                rule_content,
                            });
                            true
                        }
                        PermissionReply::Deny => {
                            let block = ToolResultBlock::error(
                                tc.id.clone(),
                                "Permission denied by user".into(),
                            );
                            let _ = event_tx.send(QueryEvent::ToolCallComplete {
                                id: tc.id.clone(),
                                result: block.clone(),
                            });
                            tool_result_blocks.push(block);
                            continue;
                        }
                    }
                }
            };

            if !should_execute {
                continue;
            }

            // PreToolUse hooks
            let hook_ctx = HookContext {
                provider: Some(ctx.provider.clone()),
                registry: ctx.registry.clone(),
            };
            if !ctx.hooks.is_empty() {
                let hook_result = execute_event_hooks(
                    "PreToolUse",
                    &tc.name,
                    &tc.input,
                    &ctx.hooks,
                    &ctx.cwd,
                    &hook_ctx,
                )
                .await;
                match hook_result.outcome {
                    HookOutcome::Success => {}
                    HookOutcome::Blocking { reason } => {
                        let block = ToolResultBlock::error(
                            tc.id.clone(),
                            format!("Blocked by hook: {}", reason),
                        );
                        let _ = event_tx.send(QueryEvent::ToolCallComplete {
                            id: tc.id.clone(),
                            result: block.clone(),
                        });
                        tool_result_blocks.push(block);
                        continue;
                    }
                    HookOutcome::NonBlockingError { message } => {
                        tracing::warn!("Hook non-blocking error: {}", message);
                    }
                }
            }

            // Execute tool
            let result = execute_tool(tc, &tools_arc, &ctx, &tools_arc, &event_tx).await;
            let block = match result {
                Ok(tr) => {
                    let mut content = tr.content.clone();

                    // PostToolUse hooks
                    if !ctx.hooks.is_empty() {
                        let hook_result = execute_event_hooks(
                            "PostToolUse",
                            &tc.name,
                            &tc.input,
                            &ctx.hooks,
                            &ctx.cwd,
                            &hook_ctx,
                        )
                        .await;
                        match hook_result.outcome {
                            HookOutcome::Blocking { reason } => {
                                tracing::warn!("PostToolUse hook blocked: {}", reason);
                            }
                            HookOutcome::NonBlockingError { message } => {
                                tracing::warn!("PostToolUse hook error: {}", message);
                            }
                            HookOutcome::Success => {}
                        }
                        // Check for updated tool output from hook_specific
                        if let Some(ref specific) = hook_result.hook_specific {
                            if let Some(updated) = specific
                                .get("updatedMCPToolOutput")
                                .and_then(|v| v.as_str())
                            {
                                content = updated.to_string();
                            }
                        }
                    }

                    let block = ToolResultBlock {
                        tool_use_id: tc.id.clone(),
                        content: content.clone(),
                        is_error: false,
                        content_blocks: tr.content_blocks.clone(),
                    };
                    let _ = event_tx.send(QueryEvent::ToolCallComplete {
                        id: tc.id.clone(),
                        result: block.clone(),
                    });
                    if tr.is_error {
                        ToolResultBlock {
                            tool_use_id: tc.id.clone(),
                            content,
                            is_error: true,
                            content_blocks: tr.content_blocks.clone(),
                        }
                    } else {
                        block
                    }
                }
                Err(e) => {
                    let block = ToolResultBlock::error(tc.id.clone(), e.to_string());
                    let _ = event_tx.send(QueryEvent::ToolCallComplete {
                        id: tc.id.clone(),
                        result: block.clone(),
                    });
                    block
                }
            };
            tool_result_blocks.push(block);
        }

        // 6. Add tool results as user message
        let tool_result_content: Vec<ContentBlock> = tool_result_blocks
            .iter()
            .map(|b| ContentBlock::ToolResult {
                tool_use_id: b.tool_use_id.clone(),
                content: if let Some(ref blocks) = b.content_blocks {
                    crate::types::content::ToolResultContent::Blocks(blocks.clone())
                } else {
                    crate::types::content::ToolResultContent::Text(b.content.clone())
                },
                is_error: b.is_error,
            })
            .collect();

        ctx.messages.push(ApiMessage {
            role: Role::Tool,
            content: tool_result_content,
        });

        // Loop back for next LLM call with tool results
    }

    Ok(QueryResult {
        messages: ctx.messages,
        stop_reason: StopReason::EndTurn,
        tool_results: Vec::new(),
    })
}

async fn execute_tool(
    tc: &ToolCall,
    tools: &[Box<dyn crate::tool::Tool>],
    ctx: &QueryContext,
    tools_arc: &Arc<Vec<Box<dyn crate::tool::Tool>>>,
    event_tx: &mpsc::UnboundedSender<QueryEvent>,
) -> crate::types::Result<crate::tool::ToolResult> {
    let tool = find_tool_by_name(tools, &tc.name).ok_or_else(|| crate::types::AtomError::Tool {
        tool_name: tc.name.clone(),
        message: format!("Tool '{}' not found", tc.name),
    })?;

    let (progress_tx, _progress_rx) = mpsc::unbounded_channel::<ToolProgress>();

    let tool_ctx = ToolUseContext {
        input: tc.input.clone(),
        abort_signal: ctx.abort_signal.clone(),
        progress_tx,
        provider: ctx.provider.clone(),
        registry: ctx.registry.clone(),
        tools: tools_arc.clone(),
        commands: ctx.commands.clone(),
        system_prompt: ctx.system_prompt.clone().unwrap_or_default(),
        cwd: ctx.cwd.clone(),
        permission_ctx: ctx.permission_ctx.clone(),
        permission_tx: ctx.permission_tx.clone(),
        event_tx: event_tx.clone(),
        tool_use_id: tc.id.clone(),
        question_tx: ctx.question_tx.clone(),
        model: ctx.model.clone(),
        app_state: ctx.app_state.clone(),
        mcp_manager: ctx.mcp_manager.clone(),
        agent_depth: ctx.agent_depth.unwrap_or(0),
        messages: std::sync::Arc::new(ctx.messages.clone()),
        query_source: ctx.query_source.clone(),
    };

    // Check abort signal with per-tool interrupt behavior
    if ctx.abort_signal.is_cancelled() {
        if let Some(crate::utils::abort_signal::AbortReason::Interrupt) = ctx.abort_signal.reason()
        {
            // Interrupt reason: respect the tool's declared behavior
            if tool.interrupt_behavior() == crate::tool::InterruptBehavior::Block {
                // Block behavior: let the tool complete despite the interrupt
            } else {
                // Cancel behavior: abort immediately
                return Err(crate::types::AtomError::Cancelled);
            }
        } else {
            // UserCancel, Background, SiblingError: always cancel
            return Err(crate::types::AtomError::Cancelled);
        }
    }

    // Race tool execution against abort signal so Ctrl+C/Escape always works
    let abort = ctx.abort_signal.clone();
    tokio::select! {
        result = tool.execute(tool_ctx) => result,
        _ = abort.cancelled() => Err(crate::types::AtomError::Cancelled),
    }
}

fn is_context_overflow_error(error: &crate::types::error::AtomError) -> bool {
    let msg = error.to_string().to_lowercase();
    msg.contains("prompt_too_long")
        || msg.contains("context_length")
        || msg.contains("max_context")
        || msg.contains("too many tokens")
        || msg.contains("context_window_exceeded")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::api::provider::LlmProvider;
    use crate::types::permissions::{PermissionMeta, PermissionMode, ToolPermissionContext};
    use async_trait::async_trait;

    /// Create a test QueryContext that bypasses all permissions
    fn test_query_context(
        messages: Vec<ApiMessage>,
        tools: Vec<Box<dyn crate::tool::Tool>>,
    ) -> QueryContext {
        let (permission_tx, _rx) = mpsc::unbounded_channel();
        let (question_tx, _qrx) = mpsc::unbounded_channel();
        QueryContext {
            messages,
            tools,
            system_prompt: None,
            model: "test".into(),
            max_tokens: 4096,
            permission_ctx: ToolPermissionContext {
                mode: PermissionMode::BypassPermissions,
                ..Default::default()
            },
            permission_tx,
            question_tx,
            hooks: std::collections::HashMap::new(),
            cwd: std::path::PathBuf::from("."),
            max_turns: None,
            max_budget_usd: None,
            commands: Arc::new(crate::commands::registry::CommandRegistry::empty()),
            provider: Arc::new(crate::services::api::provider::NoopProvider),
            registry: None,
            app_state: Arc::new(tokio::sync::RwLock::new(crate::state::AppState::default())),
            mcp_manager: None,
            agent_depth: None,
            query_source: None,
            abort_signal: crate::utils::abort_signal::AbortSignal::new(),
        }
    }

    /// Mock provider that returns a fixed response
    struct MockProvider {
        response_events: Vec<StreamEvent>,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn create_message(
            &self,
            _request: CreateMessageRequest,
        ) -> crate::types::Result<crate::services::api::provider::MessageStream> {
            let events = self.response_events.clone();
            Ok(Box::pin(futures::stream::iter(events.into_iter().map(Ok))))
        }
        fn model_info(&self) -> &ModelInfo {
            // SAFETY: leaking is fine in tests — small, bounded, and only runs once per test
            Box::leak(Box::new(ModelInfo {
                id: "test".into(),
                name: "test".into(),
                context_window: 128_000,
                max_output_tokens: 4096,
                capabilities: Capabilities::default(),
            }))
        }
        fn capabilities(&self) -> &Capabilities {
            // SAFETY: leaking is fine in tests
            Box::leak(Box::new(Capabilities::default()))
        }
    }

    /// Mock tool for testing
    struct EchoTool;

    #[async_trait]
    impl crate::tool::Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echo input"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(
            &self,
            ctx: ToolUseContext,
        ) -> crate::types::Result<crate::tool::ToolResult> {
            let msg = ctx
                .input
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("no message");
            Ok(crate::tool::ToolResult {
                content: format!("Echo: {}", msg),
                is_error: false,
                content_blocks: None,
            })
        }
        fn permission_metadata(&self, _input: &serde_json::Value) -> PermissionMeta {
            PermissionMeta {
                file_path: None,
                command: None,
                url: None,
            }
        }
    }

    #[tokio::test]
    async fn test_simple_text_response() {
        let provider = MockProvider {
            response_events: vec![
                StreamEvent::MessageStart { id: "msg_1".into() },
                StreamEvent::ContentBlockStart {
                    index: 0,
                    content_block: ContentBlock::Text {
                        text: String::new(),
                    },
                },
                StreamEvent::ContentBlockDelta {
                    index: 0,
                    delta: ContentDelta::TextDelta {
                        text: "Hello!".into(),
                    },
                },
                StreamEvent::ContentBlockStop { index: 0 },
                StreamEvent::MessageStop {
                    stop_reason: StopReason::EndTurn,
                },
            ],
        };

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let mut ctx = test_query_context(
            vec![ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text { text: "Hi".into() }],
            }],
            vec![],
        );
        ctx.provider = Arc::new(provider);

        let result = query(ctx, event_tx).await.unwrap();

        assert_eq!(result.stop_reason, StopReason::EndTurn);
        assert_eq!(result.messages.len(), 2); // user + assistant

        // Check events
        let mut got_text = false;
        while let Ok(event) = event_rx.try_recv() {
            if let QueryEvent::TextDelta(t) = event {
                assert_eq!(t, "Hello!");
                got_text = true;
            }
        }
        assert!(got_text);
    }

    #[tokio::test]
    async fn test_tool_use_loop() {
        // First call: LLM requests tool use
        // Second call: LLM responds with text after getting tool result
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let cc = call_count.clone();

        struct TwoStepProvider {
            call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }

        #[async_trait]
        impl LlmProvider for TwoStepProvider {
            async fn create_message(
                &self,
                _request: CreateMessageRequest,
            ) -> crate::types::Result<crate::services::api::provider::MessageStream> {
                let n = self
                    .call_count
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let events = if n == 0 {
                    // First call: tool use
                    vec![
                        StreamEvent::MessageStart { id: "msg_1".into() },
                        StreamEvent::ContentBlockStart {
                            index: 0,
                            content_block: ContentBlock::ToolUse {
                                id: "call_1".into(),
                                name: "echo".into(),
                                input: serde_json::Value::Object(serde_json::Map::new()),
                            },
                        },
                        StreamEvent::ContentBlockDelta {
                            index: 0,
                            delta: ContentDelta::InputJsonDelta {
                                partial_json: r#"{"message":"test"}"#.into(),
                            },
                        },
                        StreamEvent::ContentBlockStop { index: 0 },
                        StreamEvent::MessageStop {
                            stop_reason: StopReason::ToolUse,
                        },
                    ]
                } else {
                    // Second call: text response
                    vec![
                        StreamEvent::MessageStart { id: "msg_2".into() },
                        StreamEvent::ContentBlockStart {
                            index: 0,
                            content_block: ContentBlock::Text {
                                text: String::new(),
                            },
                        },
                        StreamEvent::ContentBlockDelta {
                            index: 0,
                            delta: ContentDelta::TextDelta {
                                text: "Done!".into(),
                            },
                        },
                        StreamEvent::ContentBlockStop { index: 0 },
                        StreamEvent::MessageStop {
                            stop_reason: StopReason::EndTurn,
                        },
                    ]
                };
                Ok(Box::pin(futures::stream::iter(events.into_iter().map(Ok))))
            }
            fn model_info(&self) -> &ModelInfo {
                Box::leak(Box::new(ModelInfo {
                    id: "test".into(),
                    name: "test".into(),
                    context_window: 128_000,
                    max_output_tokens: 4096,
                    capabilities: Capabilities::default(),
                }))
            }
            fn capabilities(&self) -> &Capabilities {
                Box::leak(Box::new(Capabilities::default()))
            }
        }

        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let mut ctx = test_query_context(
            vec![ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "test".into(),
                }],
            }],
            vec![Box::new(EchoTool)],
        );
        ctx.provider = Arc::new(TwoStepProvider { call_count: cc });

        let result = query(ctx, event_tx).await.unwrap();

        assert_eq!(result.stop_reason, StopReason::EndTurn);
        // user + assistant(tool_use) + tool_result + assistant(text) = 4
        assert_eq!(result.messages.len(), 4);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
}
