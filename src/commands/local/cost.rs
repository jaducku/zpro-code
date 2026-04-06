use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::LocalCommand;
use crate::commands::types::{CommandMeta, LocalCommandResult};

pub struct CostCommand;

impl CostCommand {
    const META: CommandMeta = CommandMeta {
        name: "cost",
        description: "Show session cost and token usage",
        aliases: &[],
        is_hidden: false,
        immediate: false,
        argument_hint: None,
        availability: &[],
        source: crate::commands::types::CommandSource::Builtin,
        loaded_from: None,
        when_to_use: None,
        is_sensitive: false,
        is_enabled: true,
        user_invocable: None,
        disable_model_invocation: false,
        kind: None,
        version: None,
        has_user_specified_description: false,
        is_mcp: false,
        plugin_info: None,
        user_facing_name: None,
    };
}

#[async_trait]
impl LocalCommand for CostCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    async fn call(&self, _args: &str, ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
        let state = ctx.app_state.read().await;
        let usage = &state.token_usage;
        let cost = state.session_cost;
        let total_tokens = usage.input_tokens + usage.output_tokens;
        let cache_info = if usage.cache_read_input_tokens > 0 || usage.cache_creation_input_tokens > 0 {
            format!(
                " | Cache: {} read, {} created",
                usage.cache_read_input_tokens, usage.cache_creation_input_tokens
            )
        } else {
            String::new()
        };
        Ok(LocalCommandResult::Text {
            value: format!(
                "Session cost: ${:.4} | Tokens: {} in / {} out ({} total){}",
                cost, usage.input_tokens, usage.output_tokens, total_tokens, cache_info
            ),
        })
    }
}
