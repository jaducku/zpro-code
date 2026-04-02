mod help;

use super::traits::Command;
use help::HelpCommand;

pub fn get_tui_commands() -> Vec<Command> {
    vec![Command::Tui(Box::new(HelpCommand))]
}
