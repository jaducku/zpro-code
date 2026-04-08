use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::LocalCommand;
use crate::commands::types::{CommandMeta, LocalCommandResult};
use crate::services::compact::types::ERROR_MESSAGE_NOT_ENOUGH_MESSAGES;

pub struct CompactCommand;

impl CompactCommand {
    const META: CommandMeta = CommandMeta {
        name: "compact",
        description: "Clear conversation history but keep a summary in context",
        aliases: &[],
        is_hidden: false,
        immediate: false,
        argument_hint: Some("<optional custom summarization instructions>"),
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
impl LocalCommand for CompactCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn call(&self, args: &str, ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
        let provider = ctx
            .provider
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provider available for compaction"))?;

        let all_messages = &*ctx.messages;
        let messages = crate::services::compact::get_messages_after_compact_boundary(all_messages);

        if messages.is_empty() {
            return Err(anyhow::anyhow!(ERROR_MESSAGE_NOT_ENOUGH_MESSAGES));
        }

        let custom_instructions = args.trim();
        let custom = if custom_instructions.is_empty() {
            None
        } else {
            Some(custom_instructions)
        };

        // 1. Try session memory compaction (only when no custom instructions)
        if custom.is_none() {
            if let Some(ref sm) = ctx.session_memory {
                if let Some(result) =
                    crate::services::session_memory::compaction::try_session_memory_compaction(
                        messages, sm, None,
                    )
                    .await
                {
                    sm.reset_last_summarized_index().await;
                    return Ok(LocalCommandResult::Compact { result });
                }
            }
        }

        // 2. Microcompact (reduce tokens before LLM call)
        let mut work_messages = messages.to_vec();
        crate::services::compact::microcompact::microcompact(&mut work_messages, 3);

        // 3. Full compact (LLM summarization)
        let context_window = provider.model_info().context_window;
        let result = crate::services::compact::full_compact::full_compact(
            &work_messages,
            provider.as_ref(),
            context_window,
            3,
            custom,
            &ctx.abort_signal,
        )
        .await?;

        // Reset last_summarized_index after full compact
        if let Some(ref sm) = ctx.session_memory {
            sm.reset_last_summarized_index().await;
        }

        Ok(LocalCommandResult::Compact { result })
    }
}
