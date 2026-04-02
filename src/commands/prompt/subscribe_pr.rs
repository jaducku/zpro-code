use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::PromptCommand;
use crate::commands::types::CommandMeta;
use crate::types::content::ContentBlock;

pub struct SubscribePrCommand;

impl SubscribePrCommand {
    const META: CommandMeta = CommandMeta {
        name: "subscribe-pr",
        description: "Subscribe to PR updates",
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
impl PromptCommand for SubscribePrCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn progress_message(&self) -> &str {
        "subscribing to PR updates"
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(vec![ContentBlock::Text {
            text: concat!(
                "Please subscribe to updates for the current pull request. ",
                "Set up webhook notifications so that new comments, reviews, ",
                "and status changes are reported automatically."
            )
            .to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe_pr_meta() {
        let cmd = SubscribePrCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "subscribe-pr");
    }
}
