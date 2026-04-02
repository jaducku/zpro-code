use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::PromptCommand;
use crate::commands::types::CommandMeta;
use crate::types::content::ContentBlock;

pub struct CommitPushPrCommand;

impl CommitPushPrCommand {
    const META: CommandMeta = CommandMeta {
        name: "commit-push-pr",
        description: "Commit, push, and create a PR",
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
impl PromptCommand for CommitPushPrCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn progress_message(&self) -> &str {
        "committing, pushing, and creating PR"
    }

    async fn get_prompt(
        &self,
        _args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(vec![ContentBlock::Text {
            text: concat!(
                "Create a git commit, push, and open a pull request. Follow these steps:\n\n",
                "1. Run `git status` to see all untracked and modified files.\n",
                "2. Run `git diff HEAD` to see all changes.\n",
                "3. Stage relevant files and create a commit with a descriptive message.\n",
                "4. Push the branch to the remote with `git push -u origin <branch>`.\n",
                "5. Create a pull request using `gh pr create`.\n\n",
                "Git Safety Protocol:\n",
                "- NEVER amend an existing commit unless explicitly asked.\n",
                "- NEVER skip hooks (--no-verify) unless explicitly asked.\n",
                "- NEVER force push.\n",
                "- Do NOT commit files that likely contain secrets.\n\n",
                "PR guidelines:\n",
                "- Keep the PR title short (under 70 characters).\n",
                "- Include a summary and test plan in the PR body.\n",
                "- Use `gh pr create --title \"...\" --body \"...\"`."
            )
            .to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_push_pr_meta() {
        let cmd = CommitPushPrCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "commit-push-pr");
        assert!(meta.is_hidden);
    }
}
