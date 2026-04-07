use std::any::Any;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::prelude::*;

use crate::types::content::ContentBlock;

use super::context::CommandContext;
use super::types::*;

/// A command that produces a prompt to send to the LLM.
#[async_trait]
pub trait PromptCommand: Send + Sync {
    fn meta(&self) -> &CommandMeta;

    fn allowed_tools(&self) -> Option<&[&str]> {
        None
    }

    fn progress_message(&self) -> &str;

    fn content_length(&self) -> usize {
        0
    }

    fn model_override(&self) -> Option<&str> {
        None
    }

    fn context_mode(&self) -> ContextMode {
        ContextMode::Inline
    }

    fn agent_type(&self) -> Option<&str> {
        None
    }

    fn arg_names(&self) -> Option<&[&str]> {
        None
    }

    fn disable_non_interactive(&self) -> bool {
        false
    }

    fn skill_root(&self) -> Option<&str> {
        None
    }

    fn paths(&self) -> Option<&[&str]> {
        None
    }

    async fn get_prompt(
        &self,
        args: &str,
        ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>>;
}

/// A command that runs locally without calling the LLM.
#[async_trait]
pub trait LocalCommand: Send + Sync {
    fn meta(&self) -> &CommandMeta;

    fn supports_non_interactive(&self) -> bool {
        false
    }

    async fn call(&self, args: &str, ctx: &CommandContext) -> anyhow::Result<LocalCommandResult>;
}

/// A command that renders its own TUI widget.
pub trait TuiCommand: Send + Sync {
    fn meta(&self) -> &CommandMeta;
    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send>;
    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any);
    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction;

    fn content_height(&self, _state: &dyn std::any::Any) -> u16 {
        8
    }
}

/// Unified command enum that wraps all three command kinds.
pub enum Command {
    Prompt(Box<dyn PromptCommand>),
    Local(Box<dyn LocalCommand>),
    Tui(Box<dyn TuiCommand>),
}

impl Command {
    /// Returns the metadata for this command, regardless of kind.
    pub fn meta(&self) -> &CommandMeta {
        match self {
            Command::Prompt(c) => c.meta(),
            Command::Local(c) => c.meta(),
            Command::Tui(c) => c.meta(),
        }
    }

    /// The primary name of this command.
    pub fn name(&self) -> &str {
        self.meta().name
    }

    /// Whether this command is enabled at runtime.
    pub fn is_enabled(&self) -> bool {
        is_command_enabled(self.meta())
    }

    /// The display name shown to users.
    pub fn user_facing_name(&self) -> &str {
        get_command_name(self.meta())
    }
}
