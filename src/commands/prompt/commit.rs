use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::PromptCommand;
use crate::commands::types::CommandMeta;
use crate::types::content::ContentBlock;

pub struct CommitCommand;

impl CommitCommand {
    const META: CommandMeta = CommandMeta {
        name: "commit",
        description: "Create a git commit",
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
impl PromptCommand for CommitCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn allowed_tools(&self) -> Option<&[&str]> {
        Some(&[
            "Bash(git add:*)",
            "Bash(git status:*)",
            "Bash(git commit:*)",
        ])
    }

    fn progress_message(&self) -> &str {
        "creating commit"
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(vec![ContentBlock::Text {
            text: concat!(
                "Create a git commit for the current changes. Follow these steps:\n\n",
                "1. Run `git status` to see all untracked and modified files.\n",
                "2. Run `git diff HEAD` to see all changes that will be committed.\n",
                "3. Analyze the changes and create a concise, descriptive commit message.\n\n",
                "Git Safety Protocol:\n",
                "- NEVER amend an existing commit unless explicitly asked.\n",
                "- NEVER skip hooks (--no-verify) unless explicitly asked.\n",
                "- NEVER use --force on any git command.\n",
                "- When staging files, prefer adding specific files by name rather than `git add -A` or `git add .`.\n",
                "- Do NOT commit files that likely contain secrets (.env, credentials.json, etc.).\n",
                "- If a pre-commit hook fails, fix the issue and create a NEW commit.\n\n",
                "Commit message guidelines:\n",
                "- Summarize the nature of the changes (new feature, bug fix, refactor, etc.).\n",
                "- Keep the first line under 72 characters.\n",
                "- Focus on the \"why\" rather than the \"what\".\n",
                "- Use conventional commit format if the project uses it.\n\n",
                "Stage the relevant files and create the commit."
            )
            .to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_meta() {
        let cmd = CommitCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "commit");
        assert!(meta.is_hidden);
    }

    #[test]
    fn test_commit_allowed_tools() {
        let cmd = CommitCommand;
        let tools = cmd.allowed_tools().unwrap();
        assert!(tools.contains(&"Bash(git add:*)"));
        assert!(tools.contains(&"Bash(git status:*)"));
        assert!(tools.contains(&"Bash(git commit:*)"));
    }
}
