use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::LocalCommand;
use crate::commands::types::{CommandMeta, LocalCommandResult};

pub struct InstallSlackAppCommand;

impl InstallSlackAppCommand {
    const META: CommandMeta = CommandMeta {
        name: "install-slack-app",
        description: "Install Slack app integration",
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
impl LocalCommand for InstallSlackAppCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
        // TODO: implement Slack app installation flow
        Ok(LocalCommandResult::Text {
            value: "Installing Slack app...".to_string(),
        })
    }
}
