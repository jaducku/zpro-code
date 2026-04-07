use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::PromptCommand;
use crate::commands::types::CommandMeta;
use crate::types::content::ContentBlock;

pub struct InitVerifiersCommand;

impl InitVerifiersCommand {
    const META: CommandMeta = CommandMeta {
        name: "init-verifiers",
        description: "Initialize verifiers",
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
impl PromptCommand for InitVerifiersCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn progress_message(&self) -> &str {
        "initializing verifiers"
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(vec![ContentBlock::Text {
            text: concat!(
                "Please set up verifiers for this project. ",
                "Analyze the project structure to determine appropriate verification commands ",
                "(linting, type checking, tests, etc.) and configure them as verifiers ",
                "that run automatically to validate changes."
            )
            .to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_verifiers_meta() {
        let cmd = InitVerifiersCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "init-verifiers");
        assert!(meta.is_hidden);
    }
}
