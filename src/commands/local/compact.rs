use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::LocalCommand;
use crate::commands::types::{CommandMeta, LocalCommandResult};

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

    async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
        Ok(LocalCommandResult::Compact {
            display_text: Some("Conversation compacted.".to_string()),
        })
    }
}
