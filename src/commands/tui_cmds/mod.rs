mod help;

// Core
mod config;
mod status;
mod memory;
mod mcp;
mod skills;
mod tasks;
mod session;
mod resume;
mod permissions;
mod plan;
mod hooks;
mod files;
mod branch;
mod agents;
mod plugin;
mod doctor;

// UI/settings
mod theme;
mod model;
mod ide;
mod desktop;
mod mobile;
mod chrome;
mod usage;
mod stats;
mod upgrade;
mod feedback;
mod privacy_settings;
mod add_dir;
mod context;
mod terminal_setup;
mod install_github_app;

// Interactive toggles
mod diff;
mod fast;
mod color;
mod btw;
mod rename;
mod tag;
mod effort;
mod passes;
mod export_cmd;
mod output_style;
mod sandbox_toggle;
mod remote_env;
mod rate_limit_options;
mod extra_usage;
mod thinkback;
mod ultrareview;

// Internal-only
#[cfg(feature = "internal")]
mod good_claude;
#[cfg(feature = "internal")]
mod issue;
#[cfg(feature = "internal")]
mod ctx_viz;
#[cfg(feature = "internal")]
mod onboarding;
#[cfg(feature = "internal")]
mod share;
#[cfg(feature = "internal")]
mod summary;
#[cfg(feature = "internal")]
mod teleport;
#[cfg(feature = "internal")]
mod ant_trace;
#[cfg(feature = "internal")]
mod perf_issue;
#[cfg(feature = "internal")]
mod env;
#[cfg(feature = "internal")]
mod debug_tool_call;
#[cfg(feature = "internal")]
mod bughunter;
#[cfg(feature = "internal")]
mod backfill_sessions;
#[cfg(feature = "internal")]
mod autofix_pr;

// Feature-gated
#[cfg(any(feature = "kairos", feature = "kairos_brief"))]
mod brief;
#[cfg(any(feature = "proactive", feature = "kairos"))]
mod proactive;
#[cfg(feature = "kairos")]
mod assistant;
#[cfg(feature = "bridge_mode")]
mod bridge;
#[cfg(all(feature = "daemon", feature = "bridge_mode"))]
mod remote_control_server;
#[cfg(feature = "workflow_scripts")]
mod workflows;
#[cfg(feature = "ccr_remote_setup")]
mod remote_setup;
#[cfg(feature = "ultraplan")]
mod ultraplan;
#[cfg(feature = "uds_inbox")]
mod peers;
#[cfg(feature = "fork_subagent")]
mod fork_cmd;
#[cfg(feature = "buddy")]
mod buddy;
#[cfg(feature = "agents_platform")]
mod agents_platform;

use super::traits::Command;
use help::HelpCommand;

// Core
use config::ConfigCommand;
use status::StatusCommand;
use memory::MemoryCommand;
use mcp::McpCommand;
use skills::SkillsCommand;
use tasks::TasksCommand;
use session::SessionCommand;
use resume::ResumeCommand;
use permissions::PermissionsCommand;
use plan::PlanCommand;
use hooks::HooksCommand;
use files::FilesCommand;
use branch::BranchCommand;
use agents::AgentsCommand;
use plugin::PluginCommand;
use doctor::DoctorCommand;

// UI/settings
use theme::ThemeCommand;
use model::ModelCommand;
use ide::IdeCommand;
use desktop::DesktopCommand;
use mobile::MobileCommand;
use chrome::ChromeCommand;
use usage::UsageCommand;
use stats::StatsCommand;
use upgrade::UpgradeCommand;
use feedback::FeedbackCommand;
use privacy_settings::PrivacySettingsCommand;
use add_dir::AddDirCommand;
use context::ContextCommand;
use terminal_setup::TerminalSetupCommand;
use install_github_app::InstallGithubAppCommand;

// Interactive toggles
use diff::DiffCommand;
use fast::FastCommand;
use color::ColorCommand;
use btw::BtwCommand;
use rename::RenameCommand;
use tag::TagCommand;
use effort::EffortCommand;
use passes::PassesCommand;
use export_cmd::ExportCmdCommand;
use output_style::OutputStyleCommand;
use sandbox_toggle::SandboxToggleCommand;
use remote_env::RemoteEnvCommand;
use rate_limit_options::RateLimitOptionsCommand;
use extra_usage::ExtraUsageCommand;
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
    cmds.push(Command::Tui(Box::new(remote_control_server::RemoteControlServerCommand)));

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
    cmds.push(Command::Tui(Box::new(agents_platform::AgentsPlatformCommand)));

    cmds
}
