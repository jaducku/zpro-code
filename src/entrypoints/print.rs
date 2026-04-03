use std::io::{self, IsTerminal, Read, Write};
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::commands::registry::CommandRegistry;
use crate::query::types::{QueryContext, QueryEvent};
use crate::services::api::provider::LlmProvider;
use crate::services::api::types::SystemBlock;
use crate::services::mcp::McpConnectionManager;
use crate::tools::get_builtin_tools;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};
use crate::types::permissions::{PermissionMode, PermissionRule, ToolPermissionContext};

pub enum OutputFormat {
    Text,
    Json,
    StreamJson,
}

pub async fn run_print_mode(
    prompt: Option<String>,
    provider: Arc<dyn LlmProvider>,
    system_prompt: Vec<SystemBlock>,
    mcp_manager: McpConnectionManager,
    output_format: OutputFormat,
    max_turns: Option<u32>,
    max_budget_usd: Option<f64>,
    allow_rules: Vec<PermissionRule>,
    deny_rules: Vec<PermissionRule>,
    tools_filter: Option<Vec<String>>,
) -> anyhow::Result<()> {
    // 1. Get prompt
    let prompt_text = match prompt {
        Some(p) => p,
        None => {
            if io::stdin().is_terminal() {
                anyhow::bail!(
                    "No prompt provided and stdin is a terminal. Use: atom -p \"your prompt\""
                );
            }
            let mut input = String::new();
            io::stdin().read_to_string(&mut input)?;
            input.trim().to_string()
        }
    };

    if prompt_text.is_empty() {
        anyhow::bail!("No prompt provided. Use: atom -p \"your prompt\" or pipe input.");
    }

    // 2. Build tools
    let mut tools = get_builtin_tools();
    tools.extend(mcp_manager.get_all_tools().await);

    // Apply --tools filter
    if let Some(ref filter) = tools_filter {
        if filter.is_empty() || (filter.len() == 1 && filter[0].is_empty()) {
            tools.clear();
        } else if !filter.contains(&"default".to_string()) {
            tools.retain(|t| filter.iter().any(|f| t.name() == f));
        }
    }

    // 3. Build query context (bypass permissions -- no TUI for dialogs)
    let (perm_tx, _perm_rx) = mpsc::unbounded_channel();
    let (question_tx, _question_rx) = mpsc::unbounded_channel();
    let cwd = std::env::current_dir().unwrap_or_default();
    let hooks = crate::services::hooks::load_all_hooks(&cwd);

    let ctx = QueryContext {
        messages: vec![ApiMessage {
            role: Role::User,
            content: vec![ContentBlock::Text { text: prompt_text }],
        }],
        tools,
        system_prompt: Some(system_prompt),
        model: provider.model_info().id.clone(),
        max_tokens: provider.model_info().max_output_tokens,
        permission_ctx: ToolPermissionContext {
            mode: PermissionMode::BypassPermissions,
            always_allow_rules: allow_rules,
            always_deny_rules: deny_rules,
            ..Default::default()
        },
        permission_tx: perm_tx,
        question_tx,
        hooks,
        cwd,
        max_turns,
        max_budget_usd,
        commands: Arc::new(CommandRegistry::empty()),
        provider: provider.clone(),
    };

    // 4. Run query
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<QueryEvent>();
    let cancel = tokio_util::sync::CancellationToken::new();

    let query_handle = tokio::spawn(async move {
        crate::query::engine::query(ctx, event_tx, cancel).await
    });

    // 5. Process events based on output format
    let mut final_text = String::new();
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;

    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    while let Some(event) = event_rx.recv().await {
        match &event {
            QueryEvent::TextDelta(text) => {
                final_text.push_str(text);
                match output_format {
                    OutputFormat::Text => {
                        write!(stdout_lock, "{}", text)?;
                        stdout_lock.flush()?;
                    }
                    OutputFormat::StreamJson => {
                        writeln!(
                            stdout_lock,
                            "{}",
                            serde_json::json!({"type": "text", "content": text})
                        )?;
                        stdout_lock.flush()?;
                    }
                    OutputFormat::Json => {} // collect, output at end
                }
            }
            QueryEvent::ToolCallStart { name, id } => {
                eprintln!("[tool] {} ({})", name, id);
                if matches!(output_format, OutputFormat::StreamJson) {
                    writeln!(
                        stdout_lock,
                        "{}",
                        serde_json::json!({"type": "tool_start", "name": name, "id": id})
                    )?;
                }
            }
            QueryEvent::ToolCallComplete { id, result } => {
                eprintln!(
                    "[tool] {} -> {}",
                    id,
                    if result.is_error { "error" } else { "ok" }
                );
                if matches!(output_format, OutputFormat::StreamJson) {
                    writeln!(
                        stdout_lock,
                        "{}",
                        serde_json::json!({"type": "tool_result", "id": id, "content": result.content, "is_error": result.is_error})
                    )?;
                }
            }
            QueryEvent::Usage {
                input_tokens: i,
                output_tokens: o,
            } => {
                input_tokens += i;
                output_tokens += o;
            }
            QueryEvent::ResponseComplete { .. } => {}
            QueryEvent::Error(msg) => {
                eprintln!("Error: {}", msg);
                if matches!(output_format, OutputFormat::StreamJson) {
                    writeln!(
                        stdout_lock,
                        "{}",
                        serde_json::json!({"type": "error", "message": msg})
                    )?;
                }
            }
        }
    }

    // Wait for query to finish
    match query_handle.await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => eprintln!("Query error: {}", e),
        Err(e) => eprintln!("Task error: {}", e),
    }

    // 6. Final output for json format
    if matches!(output_format, OutputFormat::Json) {
        let result = serde_json::json!({
            "result": final_text,
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "model": provider.model_info().id,
        });
        writeln!(stdout_lock, "{}", serde_json::to_string_pretty(&result)?)?;
    }

    // Newline at end for text mode
    if matches!(output_format, OutputFormat::Text) && !final_text.ends_with('\n') {
        writeln!(stdout_lock)?;
    }

    Ok(())
}
