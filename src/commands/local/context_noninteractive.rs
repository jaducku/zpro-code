use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::LocalCommand;
use crate::commands::types::{CommandMeta, LocalCommandResult};

pub struct ContextNoninteractiveCommand;

impl ContextNoninteractiveCommand {
    const META: CommandMeta = CommandMeta {
        name: "context-noninteractive",
        description: "Show context information (non-interactive)",
        aliases: &[],
        is_hidden: true,
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
impl LocalCommand for ContextNoninteractiveCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
        // TODO: show context information in non-interactive mode
        Ok(LocalCommandResult::Text {
            value: "Context information.".to_string(),
        })
    }
}
