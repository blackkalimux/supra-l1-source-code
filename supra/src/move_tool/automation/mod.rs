mod cancel;
mod register;

use crate::common::error::CliError;
use crate::move_tool::automation::cancel::CancellationCommand;
use crate::move_tool::automation::register::RegistrationCommand;
use crate::move_tool::MoveToolResult;
use clap::Parser;
use std::fmt::{Display, Formatter};

#[derive(Debug, Parser)]
#[clap(
    about = "Tool to register and cancel automation tasks",
    long_about = "Provides means to register and cancel automation tasks."
)]
pub enum AutomationCommand {
    Register(RegistrationCommand),
    Cancel(CancellationCommand),
}

impl Display for AutomationCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AutomationCommand::Register(_) => write!(f, "Automation::Register"),
            AutomationCommand::Cancel(_) => write!(f, "Automation::Cancel"),
        }
    }
}

impl AutomationCommand {
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        match self {
            AutomationCommand::Register(command) => command.execute().await,
            AutomationCommand::Cancel(command) => command.execute().await,
        }
    }
}
