use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::helpers::create_moved_to_plugin_prompt;
use crate::commands::traits::PromptCommand;
use crate::commands::types::CommandMeta;
use crate::types::content::ContentBlock;

pub struct SecurityReviewCommand;

impl SecurityReviewCommand {
    const META: CommandMeta = CommandMeta {
        name: "security-review",
        description: "Run a security review of the codebase",
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
impl PromptCommand for SecurityReviewCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn progress_message(&self) -> &str {
        "running security review"
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(vec![ContentBlock::Text {
            text: create_moved_to_plugin_prompt("security-review", "security"),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_review_meta() {
        let cmd = SecurityReviewCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "security-review");
    }
}
