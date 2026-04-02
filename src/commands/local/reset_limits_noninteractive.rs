use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::LocalCommand;
use crate::commands::types::{CommandMeta, LocalCommandResult};

pub struct ResetLimitsNoninteractiveCommand;

impl ResetLimitsNoninteractiveCommand {
    const META: CommandMeta = CommandMeta {
        name: "reset-limits-noninteractive",
        description: "Reset rate limits (non-interactive)",
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
impl LocalCommand for ResetLimitsNoninteractiveCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
        // TODO: reset rate limits in non-interactive mode
        Ok(LocalCommandResult::Text {
            value: "Rate limits reset (non-interactive).".to_string(),
        })
    }
}
