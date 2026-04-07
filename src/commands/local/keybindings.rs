use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::LocalCommand;
use crate::commands::types::{CommandMeta, LocalCommandResult};

pub struct KeybindingsCommand;

impl KeybindingsCommand {
    const META: CommandMeta = CommandMeta {
        name: "keybindings",
        description: "Show keybinding configuration",
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
impl LocalCommand for KeybindingsCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
        Ok(LocalCommandResult::Text {
            value: "Keybindings:\n  Enter     — Send message\n  Ctrl+C    — Quit\n  /         — Command menu\n  @         — File mention\n  Up/Down   — Scroll\n  Tab       — Accept suggestion".to_string(),
        })
    }
}
