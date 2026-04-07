use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::PromptCommand;
use crate::commands::types::CommandMeta;
use crate::types::content::ContentBlock;

pub struct StatuslineCommand;

impl StatuslineCommand {
    const META: CommandMeta = CommandMeta {
        name: "statusline",
        description: "Configure status line display",
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
impl PromptCommand for StatuslineCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn progress_message(&self) -> &str {
        "configuring statusline"
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(vec![ContentBlock::Text {
            text: concat!(
                "Please help configure the status line display. ",
                "Ask the user what information they would like shown in the status line ",
                "and update the configuration accordingly."
            )
            .to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statusline_meta() {
        let cmd = StatuslineCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "statusline");
    }
}
