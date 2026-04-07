mod init;
mod insights;
mod pr_comments;
mod review;
mod security_review;
mod statusline;

#[cfg(feature = "internal")]
mod commit;
#[cfg(feature = "internal")]
mod commit_push_pr;
#[cfg(feature = "internal")]
mod init_verifiers;

#[cfg(feature = "kairos_github_webhooks")]
mod subscribe_pr;
#[cfg(feature = "torch")]
mod torch;

use super::traits::Command;
use init::InitCommand;
use insights::InsightsCommand;
use pr_comments::PrCommentsCommand;
use review::ReviewCommand;
use security_review::SecurityReviewCommand;
use statusline::StatuslineCommand;

pub fn get_prompt_commands() -> Vec<Command> {
    vec![
        Command::Prompt(Box::new(ReviewCommand)),
        Command::Prompt(Box::new(SecurityReviewCommand)),
        Command::Prompt(Box::new(PrCommentsCommand)),
        Command::Prompt(Box::new(InitCommand)),
        Command::Prompt(Box::new(InsightsCommand)),
        Command::Prompt(Box::new(StatuslineCommand)),
    ]
}

#[cfg(feature = "internal")]
pub fn get_internal_prompt_commands() -> Vec<Command> {
    let mut cmds = vec![
        Command::Prompt(Box::new(commit::CommitCommand)),
        Command::Prompt(Box::new(commit_push_pr::CommitPushPrCommand)),
        Command::Prompt(Box::new(init_verifiers::InitVerifiersCommand)),
    ];

    #[cfg(feature = "kairos_github_webhooks")]
    cmds.push(Command::Prompt(Box::new(
        subscribe_pr::SubscribePrCommand,
    )));

    #[cfg(feature = "torch")]
    cmds.push(Command::Prompt(Box::new(torch::TorchCommand)));

    cmds
}
