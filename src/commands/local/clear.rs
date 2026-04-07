use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::LocalCommand;
use crate::commands::types::{CommandMeta, LocalCommandResult};

pub struct ClearCommand;

impl ClearCommand {
    const META: CommandMeta = CommandMeta {
        name: "clear",
        description: "Clear conversation history and free up context",
        aliases: &["reset", "new"],
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
impl LocalCommand for ClearCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    async fn call(&self, _args: &str, _ctx: &CommandContext) -> anyhow::Result<LocalCommandResult> {
        Ok(LocalCommandResult::Compact {
            display_text: Some("Conversation cleared.".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_meta() {
        let cmd = ClearCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "clear");
        assert_eq!(meta.aliases, &["reset", "new"]);
    }
}
