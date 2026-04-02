use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::PromptCommand;
use crate::commands::types::CommandMeta;
use crate::types::content::ContentBlock;

pub struct InsightsCommand;

impl InsightsCommand {
    const META: CommandMeta = CommandMeta {
        name: "insights",
        description: "Generate a report analyzing your Claude Code sessions",
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
impl PromptCommand for InsightsCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn progress_message(&self) -> &str {
        "generating session insights"
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(vec![ContentBlock::Text {
            text: concat!(
                "Please analyze the current and recent Claude Code sessions. ",
                "Generate a report covering:\n",
                "- Session duration and activity patterns\n",
                "- Types of tasks performed (coding, debugging, reviewing, etc.)\n",
                "- Tool usage statistics\n",
                "- Common workflows and patterns\n",
                "- Suggestions for improving productivity"
            )
            .to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insights_meta() {
        let cmd = InsightsCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "insights");
    }
}
