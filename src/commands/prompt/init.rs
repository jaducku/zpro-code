use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::PromptCommand;
use crate::commands::types::CommandMeta;
use crate::types::content::ContentBlock;

pub struct InitCommand;

impl InitCommand {
    const META: CommandMeta = CommandMeta {
        name: "init",
        description: "Initialize CLAUDE.md with codebase context",
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
impl PromptCommand for InitCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn progress_message(&self) -> &str {
        "initializing CLAUDE.md"
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(vec![ContentBlock::Text {
            text: concat!(
                "Please create a CLAUDE.md file in the project root with codebase context. ",
                "Analyze the project structure, dependencies, build system, and coding conventions. ",
                "Include:\n",
                "- Project overview and purpose\n",
                "- Build and test commands\n",
                "- Code style and conventions\n",
                "- Architecture overview\n",
                "- Key directories and their purposes\n",
                "- Any important patterns or practices used in the codebase"
            )
            .to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_meta() {
        let cmd = InitCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "init");
        assert_eq!(meta.description, "Initialize CLAUDE.md with codebase context");
    }
}
