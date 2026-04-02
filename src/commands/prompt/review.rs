use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::PromptCommand;
use crate::commands::types::CommandMeta;
use crate::types::content::ContentBlock;

pub struct ReviewCommand;

impl ReviewCommand {
    const META: CommandMeta = CommandMeta {
        name: "review",
        description: "Review a pull request",
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
impl PromptCommand for ReviewCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn progress_message(&self) -> &str {
        "reviewing code"
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(vec![ContentBlock::Text {
            text: concat!(
                "Please review the current git changes. ",
                "Run `git diff` to see unstaged changes and `git diff --cached` for staged changes. ",
                "Provide a thorough code review covering:\n",
                "- Correctness and potential bugs\n",
                "- Code style and readability\n",
                "- Performance considerations\n",
                "- Security concerns\n",
                "- Suggestions for improvement"
            )
            .to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_meta() {
        let cmd = ReviewCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "review");
        assert_eq!(meta.description, "Review a pull request");
    }

    #[test]
    fn test_review_progress_message() {
        let cmd = ReviewCommand;
        assert_eq!(cmd.progress_message(), "reviewing code");
    }
}
