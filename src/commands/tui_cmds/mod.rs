mod help;

// Core
mod agents;
mod branch;
mod config;
mod doctor;
mod files;
mod hooks;
mod mcp;
mod memory;
mod permissions;
mod plan;
mod plugin;
mod resume;
mod session;
mod skills;
mod status;
mod tasks;

// UI/settings
mod add_dir;
mod chrome;
mod context;
mod desktop;
mod feedback;
mod ide;
mod install_github_app;
mod mobile;
mod model;
mod privacy_settings;
mod stats;
mod terminal_setup;
mod theme;
mod upgrade;
mod usage;

// Interactive toggles
mod btw;
mod color;
mod diff;
mod effort;
mod export_cmd;
mod extra_usage;
mod fast;
mod output_style;
mod passes;
mod rate_limit_options;
mod remote_env;
mod rename;
mod sandbox_toggle;
mod tag;
mod thinkback;
mod ultrareview;

// Internal-only
#[cfg(feature = "internal")]
mod ant_trace;
#[cfg(feature = "internal")]
mod autofix_pr;
#[cfg(feature = "internal")]
mod backfill_sessions;
#[cfg(feature = "internal")]
mod bughunter;
#[cfg(feature = "internal")]
mod ctx_viz;
#[cfg(feature = "internal")]
mod debug_tool_call;
#[cfg(feature = "internal")]
mod env;
#[cfg(feature = "internal")]
mod good_claude;
#[cfg(feature = "internal")]
mod issue;
#[cfg(feature = "internal")]
mod onboarding;
#[cfg(feature = "internal")]
mod perf_issue;
#[cfg(feature = "internal")]
mod share;
#[cfg(feature = "internal")]
mod summary;
#[cfg(feature = "internal")]
mod teleport;

// Feature-gated
#[cfg(feature = "agents_platform")]
mod agents_platform;
#[cfg(feature = "kairos")]
mod assistant;
#[cfg(feature = "bridge_mode")]
mod bridge;
#[cfg(any(feature = "kairos", feature = "kairos_brief"))]
mod brief;
#[cfg(feature = "buddy")]
mod buddy;
#[cfg(feature = "fork_subagent")]
mod fork_cmd;
#[cfg(feature = "uds_inbox")]
mod peers;
#[cfg(any(feature = "proactive", feature = "kairos"))]
mod proactive;
#[cfg(all(feature = "daemon", feature = "bridge_mode"))]
mod remote_control_server;
#[cfg(feature = "ccr_remote_setup")]
mod remote_setup;
#[cfg(feature = "ultraplan")]
mod ultraplan;
#[cfg(feature = "workflow_scripts")]
mod workflows;

use super::traits::Command;
use help::HelpCommand;

// Core
use agents::AgentsCommand;
use branch::BranchCommand;
use config::ConfigCommand;
use doctor::DoctorCommand;
use files::FilesCommand;
use hooks::HooksCommand;
use mcp::McpCommand;
use memory::MemoryCommand;
use permissions::PermissionsCommand;
use plan::PlanCommand;
use plugin::PluginCommand;
use resume::ResumeCommand;
use session::SessionCommand;
use skills::SkillsCommand;
use status::StatusCommand;
use tasks::TasksCommand;

// UI/settings
use add_dir::AddDirCommand;
use chrome::ChromeCommand;
use context::ContextCommand;
use desktop::DesktopCommand;
use feedback::FeedbackCommand;
use ide::IdeCommand;
use install_github_app::InstallGithubAppCommand;
use mobile::MobileCommand;
use model::ModelCommand;
use privacy_settings::PrivacySettingsCommand;
use stats::StatsCommand;
use terminal_setup::TerminalSetupCommand;
use theme::ThemeCommand;
use upgrade::UpgradeCommand;
use usage::UsageCommand;

// Interactive toggles
use btw::BtwCommand;
use color::ColorCommand;
use diff::DiffCommand;
use effort::EffortCommand;
use export_cmd::ExportCmdCommand;
use extra_usage::ExtraUsageCommand;
use fast::FastCommand;
use output_style::OutputStyleCommand;
use passes::PassesCommand;
use rate_limit_options::RateLimitOptionsCommand;
use remote_env::RemoteEnvCommand;
use rename::RenameCommand;
use sandbox_toggle::SandboxToggleCommand;
use tag::TagCommand;
use thinkback::ThinkbackCommand;
use ultrareview::UltrareviewCommand;

pub fn get_tui_commands() -> Vec<Command> {
    vec![
        Command::Tui(Box::new(HelpCommand)),
        // Core
        Command::Tui(Box::new(ConfigCommand)),
        Command::Tui(Box::new(StatusCommand)),
        Command::Tui(Box::new(MemoryCommand)),
        Command::Tui(Box::new(McpCommand)),
        Command::Tui(Box::new(SkillsCommand)),
        Command::Tui(Box::new(TasksCommand)),
        Command::Tui(Box::new(SessionCommand)),
        Command::Tui(Box::new(ResumeCommand)),
        Command::Tui(Box::new(PermissionsCommand)),
        Command::Tui(Box::new(PlanCommand)),
        Command::Tui(Box::new(HooksCommand)),
        Command::Tui(Box::new(FilesCommand)),
        Command::Tui(Box::new(BranchCommand)),
        Command::Tui(Box::new(AgentsCommand)),
        Command::Tui(Box::new(PluginCommand)),
        Command::Tui(Box::new(DoctorCommand)),
        // UI/settings
        Command::Tui(Box::new(ThemeCommand)),
        Command::Tui(Box::new(ModelCommand)),
        Command::Tui(Box::new(IdeCommand)),
        Command::Tui(Box::new(DesktopCommand)),
        Command::Tui(Box::new(MobileCommand)),
        Command::Tui(Box::new(ChromeCommand)),
        Command::Tui(Box::new(UsageCommand)),
        Command::Tui(Box::new(StatsCommand)),
        Command::Tui(Box::new(UpgradeCommand)),
        Command::Tui(Box::new(FeedbackCommand)),
        Command::Tui(Box::new(PrivacySettingsCommand)),
        Command::Tui(Box::new(AddDirCommand)),
        Command::Tui(Box::new(ContextCommand)),
        Command::Tui(Box::new(TerminalSetupCommand)),
        Command::Tui(Box::new(InstallGithubAppCommand)),
        // Interactive toggles
        Command::Tui(Box::new(DiffCommand)),
        Command::Tui(Box::new(FastCommand)),
        Command::Tui(Box::new(ColorCommand)),
        Command::Tui(Box::new(BtwCommand)),
        Command::Tui(Box::new(RenameCommand)),
        Command::Tui(Box::new(TagCommand)),
        Command::Tui(Box::new(EffortCommand)),
        Command::Tui(Box::new(PassesCommand)),
        Command::Tui(Box::new(ExportCmdCommand)),
        Command::Tui(Box::new(OutputStyleCommand)),
        Command::Tui(Box::new(SandboxToggleCommand)),
        Command::Tui(Box::new(RemoteEnvCommand)),
        Command::Tui(Box::new(RateLimitOptionsCommand)),
        Command::Tui(Box::new(ExtraUsageCommand)),
        Command::Tui(Box::new(ThinkbackCommand)),
        Command::Tui(Box::new(UltrareviewCommand)),
    ]
}

#[cfg(feature = "internal")]
pub fn get_internal_tui_commands() -> Vec<Command> {
    let mut cmds = vec![
        // Internal-only
        Command::Tui(Box::new(good_claude::GoodClaudeCommand)),
        Command::Tui(Box::new(issue::IssueCommand)),
        Command::Tui(Box::new(ctx_viz::CtxVizCommand)),
        Command::Tui(Box::new(onboarding::OnboardingCommand)),
        Command::Tui(Box::new(share::ShareCommand)),
        Command::Tui(Box::new(summary::SummaryCommand)),
        Command::Tui(Box::new(teleport::TeleportCommand)),
        Command::Tui(Box::new(ant_trace::AntTraceCommand)),
        Command::Tui(Box::new(perf_issue::PerfIssueCommand)),
        Command::Tui(Box::new(env::EnvCommand)),
        Command::Tui(Box::new(debug_tool_call::DebugToolCallCommand)),
        Command::Tui(Box::new(bughunter::BughunterCommand)),
        Command::Tui(Box::new(backfill_sessions::BackfillSessionsCommand)),
        Command::Tui(Box::new(autofix_pr::AutofixPrCommand)),
    ];

    // Feature-gated commands (also internal-only gated)
    #[cfg(any(feature = "kairos", feature = "kairos_brief"))]
    cmds.push(Command::Tui(Box::new(brief::BriefCommand)));

    #[cfg(any(feature = "proactive", feature = "kairos"))]
    cmds.push(Command::Tui(Box::new(proactive::ProactiveCommand)));

    #[cfg(feature = "kairos")]
    cmds.push(Command::Tui(Box::new(assistant::AssistantCommand)));

    #[cfg(feature = "bridge_mode")]
    cmds.push(Command::Tui(Box::new(bridge::BridgeCommand)));

    #[cfg(all(feature = "daemon", feature = "bridge_mode"))]
    cmds.push(Command::Tui(Box::new(
        remote_control_server::RemoteControlServerCommand,
    )));

    #[cfg(feature = "workflow_scripts")]
    cmds.push(Command::Tui(Box::new(workflows::WorkflowsCommand)));

    #[cfg(feature = "ccr_remote_setup")]
    cmds.push(Command::Tui(Box::new(remote_setup::RemoteSetupCommand)));

    #[cfg(feature = "ultraplan")]
    cmds.push(Command::Tui(Box::new(ultraplan::UltraplanCommand)));

    #[cfg(feature = "uds_inbox")]
    cmds.push(Command::Tui(Box::new(peers::PeersCommand)));

    #[cfg(feature = "fork_subagent")]
    cmds.push(Command::Tui(Box::new(fork_cmd::ForkCmdCommand)));

    #[cfg(feature = "buddy")]
    cmds.push(Command::Tui(Box::new(buddy::BuddyCommand)));

    #[cfg(feature = "agents_platform")]
    cmds.push(Command::Tui(Box::new(
        agents_platform::AgentsPlatformCommand,
    )));

    cmds
}
