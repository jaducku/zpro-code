use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::LocalCommand;
use crate::commands::types::{CommandMeta, LocalCommandResult};

pub struct ReleaseNotesCommand;

impl ReleaseNotesCommand {
    const META: CommandMeta = CommandMeta {
        name: "release-notes",
        description: "Show release notes",
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
impl LocalCommand for ReleaseNotesCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
        Ok(LocalCommandResult::Text {
            value: format!("zpro-code v{}\n\nPhase 8: CLI Commands\n- Command type system\n- ~110 commands registered\n- Slash command typeahead\n- @ file mentions\n- Inline command panel\n- CLI subcommands (mcp, auth, plugin, doctor)", env!("CARGO_PKG_VERSION")),
        })
    }
}
