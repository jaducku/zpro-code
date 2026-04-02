mod advisor;
mod clear;
mod compact;
mod context_noninteractive;
mod copy;
mod cost;
mod exit;
mod extra_usage_noninteractive;
mod install_slack_app;
mod keybindings;
mod release_notes;
mod reload_plugins;
mod rewind;
mod stickers;
mod thinkback_play;
mod vim;

// Internal-only commands
#[cfg(feature = "internal")]
mod break_cache;
#[cfg(feature = "internal")]
mod bridge_kick;
#[cfg(feature = "internal")]
mod heapdump;
#[cfg(feature = "internal")]
mod mock_limits;
#[cfg(feature = "internal")]
mod oauth_refresh;
#[cfg(feature = "internal")]
mod reset_limits;
#[cfg(feature = "internal")]
mod reset_limits_noninteractive;
#[cfg(feature = "internal")]
mod version;

// Feature-gated internal commands
#[cfg(feature = "history_snip")]
mod force_snip;
#[cfg(feature = "voice_mode")]
mod voice;

use super::traits::Command;
use advisor::AdvisorCommand;
use clear::ClearCommand;
use compact::CompactCommand;
use context_noninteractive::ContextNoninteractiveCommand;
use copy::CopyCommand;
use cost::CostCommand;
use exit::ExitCommand;
use extra_usage_noninteractive::ExtraUsageNoninteractiveCommand;
use install_slack_app::InstallSlackAppCommand;
use keybindings::KeybindingsCommand;
use release_notes::ReleaseNotesCommand;
use reload_plugins::ReloadPluginsCommand;
use rewind::RewindCommand;
use stickers::StickersCommand;
use thinkback_play::ThinkbackPlayCommand;
use vim::VimCommand;

pub fn get_local_commands() -> Vec<Command> {
    vec![
        Command::Local(Box::new(ClearCommand)),
        Command::Local(Box::new(ExitCommand)),
        Command::Local(Box::new(CostCommand)),
        Command::Local(Box::new(CopyCommand)),
        Command::Local(Box::new(VimCommand)),
        Command::Local(Box::new(AdvisorCommand)),
        Command::Local(Box::new(ReleaseNotesCommand)),
        Command::Local(Box::new(StickersCommand)),
        Command::Local(Box::new(KeybindingsCommand)),
        Command::Local(Box::new(ReloadPluginsCommand)),
        Command::Local(Box::new(RewindCommand)),
        Command::Local(Box::new(InstallSlackAppCommand)),
        Command::Local(Box::new(ThinkbackPlayCommand)),
        Command::Local(Box::new(CompactCommand)),
        Command::Local(Box::new(ContextNoninteractiveCommand)),
        Command::Local(Box::new(ExtraUsageNoninteractiveCommand)),
    ]
}

#[cfg(feature = "internal")]
pub fn get_internal_local_commands() -> Vec<Command> {
    let mut cmds = vec![
        Command::Local(Box::new(version::VersionCommand)),
        Command::Local(Box::new(bridge_kick::BridgeKickCommand)),
        Command::Local(Box::new(break_cache::BreakCacheCommand)),
        Command::Local(Box::new(mock_limits::MockLimitsCommand)),
        Command::Local(Box::new(reset_limits::ResetLimitsCommand)),
        Command::Local(Box::new(reset_limits_noninteractive::ResetLimitsNoninteractiveCommand)),
        Command::Local(Box::new(oauth_refresh::OauthRefreshCommand)),
        Command::Local(Box::new(heapdump::HeapdumpCommand)),
    ];

    #[cfg(feature = "history_snip")]
    cmds.push(Command::Local(Box::new(force_snip::ForceSnipCommand)));

    #[cfg(feature = "voice_mode")]
    cmds.push(Command::Local(Box::new(voice::VoiceCommand)));

    cmds
}
