mod clear;

use super::traits::Command;
use clear::ClearCommand;

pub fn get_local_commands() -> Vec<Command> {
    vec![Command::Local(Box::new(ClearCommand))]
}
