use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::helpers::create_moved_to_plugin_prompt;
use crate::commands::traits::PromptCommand;
use crate::commands::types::CommandMeta;
use crate::types::content::ContentBlock;

pub struct PrCommentsCommand;

impl PrCommentsCommand {
    const META: CommandMeta = CommandMeta {
        name: "pr-comments",
        description: "Address PR review comments",
        aliases: &["pr_comments"],
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
impl PromptCommand for PrCommentsCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn progress_message(&self) -> &str {
        "addressing PR comments"
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(vec![ContentBlock::Text {
            text: create_moved_to_plugin_prompt("pr-comments", "pr-comments"),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pr_comments_meta() {
        let cmd = PrCommentsCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "pr-comments");
        assert_eq!(meta.aliases, &["pr_comments"]);
    }
}
