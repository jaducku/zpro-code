use super::traits::Command;
use super::types::*;

/// Names of commands considered safe for remote (non-local) execution.
const REMOTE_SAFE: &[&str] = &[
    "session", "exit", "clear", "help", "theme", "color", "vim", "cost", "usage", "copy", "btw",
    "feedback", "plan", "keybindings", "statusline", "stickers", "mobile",
];

/// Names of commands allowed across the bridge in addition to all Prompt commands.
const BRIDGE_SAFE: &[&str] = &[
    "compact",
    "clear",
    "cost",
    "summary",
    "release-notes",
    "files",
];

/// Registry that holds builtin and dynamic commands and provides lookup/filtering.
pub struct CommandRegistry {
    builtin: Vec<Command>,
    dynamic: Vec<Command>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    /// Create a registry pre-populated with all builtin commands.
    pub fn new() -> Self {
        let mut reg = Self::empty();
        for cmd in super::local::get_local_commands() {
            reg.register_builtin(cmd);
        }
        for cmd in super::prompt::get_prompt_commands() {
            reg.register_builtin(cmd);
        }
        for cmd in super::tui_cmds::get_tui_commands() {
            reg.register_builtin(cmd);
        }
        #[cfg(feature = "internal")]
        for cmd in super::local::get_internal_local_commands() {
            reg.register_builtin(cmd);
        }
        #[cfg(feature = "internal")]
        for cmd in super::tui_cmds::get_internal_tui_commands() {
            reg.register_builtin(cmd);
        }
        reg
    }

    /// Create an empty registry (useful for testing).
    pub fn empty() -> Self {
        Self {
            builtin: Vec::new(),
            dynamic: Vec::new(),
        }
    }

    /// Register a builtin command.
    pub fn register_builtin(&mut self, cmd: Command) {
        self.builtin.push(cmd);
    }

    /// Register a dynamic command.
    pub fn register_dynamic(&mut self, cmd: Command) {
        self.dynamic.push(cmd);
    }

    /// Register a dynamic skill command only if its name is not already present.
    pub fn register_dynamic_skill(&mut self, cmd: Command) {
        let name = cmd.name();
        if !self.has(name) {
            self.dynamic.push(cmd);
        }
    }

    /// Find a command by name, user-facing name, or alias.
    pub fn find(&self, name: &str) -> Option<&Command> {
        self.all_commands().find(|cmd| {
            let meta = cmd.meta();
            meta.name == name
                || get_command_name(meta) == name
                || meta.aliases.contains(&name)
        })
    }

    /// Check whether a command with the given name, user-facing name, or alias exists.
    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// Return all commands that are currently enabled.
    pub fn available_commands(&self) -> Vec<&Command> {
        self.all_commands().filter(|cmd| cmd.is_enabled()).collect()
    }

    /// Return commands whose names are in the remote-safe allow list.
    pub fn remote_safe_commands(&self) -> Vec<&Command> {
        self.all_commands()
            .filter(|cmd| REMOTE_SAFE.contains(&cmd.name()))
            .collect()
    }

    /// Return all Prompt commands plus commands whose names are in the bridge-safe list.
    pub fn bridge_safe_commands(&self) -> Vec<&Command> {
        self.all_commands()
            .filter(|cmd| {
                matches!(cmd, Command::Prompt(_)) || BRIDGE_SAFE.contains(&cmd.name())
            })
            .collect()
    }

    /// Return prompt-type commands eligible for skill-tool exposure.
    ///
    /// A command qualifies when it is a `Prompt` variant, model invocation is not
    /// disabled, its source is not `Builtin`, and at least one of:
    ///   - `loaded_from` is `Bundled`, `Skills`, or `CommandsDeprecated`
    ///   - `has_user_specified_description` is true
    ///   - `when_to_use` is `Some`
    pub fn skill_tool_commands(&self) -> Vec<&Command> {
        self.all_commands()
            .filter(|cmd| {
                if !matches!(cmd, Command::Prompt(_)) {
                    return false;
                }
                let meta = cmd.meta();
                if meta.disable_model_invocation {
                    return false;
                }
                if meta.source == CommandSource::Builtin {
                    return false;
                }
                let loaded_ok = matches!(
                    meta.loaded_from,
                    Some(
                        CommandLoadedFrom::Bundled
                            | CommandLoadedFrom::Skills
                            | CommandLoadedFrom::CommandsDeprecated
                    )
                );
                loaded_ok || meta.has_user_specified_description || meta.when_to_use.is_some()
            })
            .collect()
    }

    /// Remove all dynamic commands.
    pub fn clear_dynamic(&mut self) {
        self.dynamic.clear();
    }

    /// Load skill commands from `.agents/skills/` directories.
    ///
    /// Discovers skills by walking from `cwd` up to the git root (project-level)
    /// and from `~/.zpro-code/skills/` (user-level). Each skill with a unique
    /// name is registered via `register_dynamic_skill` (first-found wins).
    pub async fn load_skill_dir_commands(&mut self, cwd: &std::path::Path) -> anyhow::Result<()> {
        let skills = crate::services::skills::load_skills(cwd);
        for skill in skills {
            self.register_dynamic_skill(skill);
        }
        Ok(())
    }

    /// Load plugin commands from configured plugin sources.
    ///
    /// Implemented in Phase 9 (services).
    pub async fn load_plugin_commands(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Register a batch of MCP-sourced commands as dynamic commands.
    pub fn register_mcp_commands(&mut self, commands: Vec<Command>) {
        for cmd in commands {
            self.register_dynamic(cmd);
        }
    }

    /// Load bundled skill commands that ship with the binary.
    ///
    /// Implemented in Phase 9.
    pub fn load_bundled_skills(&mut self) {
        // Implemented in Phase 9
    }

    /// Iterate over all commands, dynamic first then builtin.
    fn all_commands(&self) -> impl Iterator<Item = &Command> {
        self.dynamic.iter().chain(self.builtin.iter())
    }
}

/// Format a human-readable description that includes the command source.
pub fn format_description_with_source(cmd: &Command) -> String {
    let meta = cmd.meta();
    let source_label = match meta.source {
        CommandSource::Builtin => "builtin",
        CommandSource::Mcp => "mcp",
        CommandSource::Plugin => "plugin",
        CommandSource::Bundled => "bundled",
        CommandSource::UserSettings => "user-settings",
        CommandSource::ProjectSettings => "project-settings",
        CommandSource::LocalSettings => "local-settings",
        CommandSource::FlagSettings => "flag-settings",
        CommandSource::PolicySettings => "policy-settings",
    };
    format!("{} ({})", meta.description, source_label)
}

/// Check whether a command meets its availability requirements.
///
/// For atom-code (open provider), all commands are considered available.
pub fn meets_availability_requirement(_cmd: &Command) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::traits::{Command, LocalCommand};
    use crate::commands::types::{CommandMeta, LocalCommandResult};
    use crate::commands::context::CommandContext;
    use async_trait::async_trait;

    /// A minimal LocalCommand for testing purposes.
    struct TestLocalCmd {
        meta: CommandMeta,
    }

    #[async_trait]
    impl LocalCommand for TestLocalCmd {
        fn meta(&self) -> &CommandMeta {
            &self.meta
        }

        async fn call(
            &self,
            _args: &str,
            _ctx: &CommandContext,
        ) -> anyhow::Result<LocalCommandResult> {
            Ok(LocalCommandResult::Skip)
        }
    }

    fn make_test_command(name: &'static str) -> Command {
        Command::Local(Box::new(TestLocalCmd {
            meta: CommandMeta {
                name,
                description: "test command",
                ..CommandMeta::default()
            },
        }))
    }

    fn make_test_command_with_aliases(
        name: &'static str,
        aliases: &'static [&'static str],
    ) -> Command {
        Command::Local(Box::new(TestLocalCmd {
            meta: CommandMeta {
                name,
                description: "test command",
                aliases,
                ..CommandMeta::default()
            },
        }))
    }

    fn make_disabled_command(name: &'static str) -> Command {
        Command::Local(Box::new(TestLocalCmd {
            meta: CommandMeta {
                name,
                description: "disabled",
                is_enabled: false,
                ..CommandMeta::default()
            },
        }))
    }

    #[test]
    fn test_find_by_name() {
        let mut reg = CommandRegistry::empty();
        reg.register_builtin(make_test_command("help"));

        assert!(reg.find("help").is_some());
        assert_eq!(reg.find("help").unwrap().name(), "help");
        assert!(reg.find("nonexistent").is_none());
    }

    #[test]
    fn test_find_by_alias() {
        let mut reg = CommandRegistry::empty();
        reg.register_builtin(make_test_command_with_aliases("help", &["h", "?"]));

        assert!(reg.find("h").is_some());
        assert_eq!(reg.find("h").unwrap().name(), "help");
        assert!(reg.find("?").is_some());
        assert_eq!(reg.find("?").unwrap().name(), "help");
    }

    #[test]
    fn test_has() {
        let mut reg = CommandRegistry::empty();
        reg.register_builtin(make_test_command("clear"));

        assert!(reg.has("clear"));
        assert!(!reg.has("missing"));
    }

    #[test]
    fn test_available_filters_disabled() {
        let mut reg = CommandRegistry::empty();
        reg.register_builtin(make_test_command("enabled_cmd"));
        reg.register_builtin(make_disabled_command("disabled_cmd"));

        let available = reg.available_commands();
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].name(), "enabled_cmd");
    }

    #[test]
    fn test_dynamic_shadows_builtin() {
        let mut reg = CommandRegistry::empty();
        reg.register_builtin(make_test_command_with_aliases("help", &["h"]));
        reg.register_dynamic(make_test_command("help"));

        // Dynamic should be found first
        let cmd = reg.find("help").unwrap();
        // Both have the same name, but the dynamic one has no aliases
        assert!(cmd.meta().aliases.is_empty());
    }

    #[test]
    fn test_clear_dynamic() {
        let mut reg = CommandRegistry::empty();
        reg.register_dynamic(make_test_command("dyn_cmd"));
        assert!(reg.has("dyn_cmd"));

        reg.clear_dynamic();
        assert!(!reg.has("dyn_cmd"));
    }

    #[test]
    fn test_register_dynamic_skill_no_duplicate() {
        let mut reg = CommandRegistry::empty();
        reg.register_builtin(make_test_command("existing"));
        reg.register_dynamic_skill(make_test_command("existing"));

        // Should still have only one (the builtin); dynamic was not added
        let count = reg.all_commands().count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_format_description_with_source() {
        let cmd = make_test_command("foo");
        let desc = format_description_with_source(&cmd);
        assert_eq!(desc, "test command (builtin)");
    }

    #[test]
    fn test_meets_availability_requirement() {
        let cmd = make_test_command("anything");
        assert!(meets_availability_requirement(&cmd));
    }
}
