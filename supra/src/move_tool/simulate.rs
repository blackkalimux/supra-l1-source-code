use aptos::move_tool::{PublishPackage, RunFunction, RunScript};
use clap::Parser;
use std::fmt::{Display, Formatter};
use supra_aptos::SupraCommand;

use crate::common::error::CliError;
use crate::move_tool::move_account::transfer::TransferCoins;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::tool_option::MoveTransactionOptions;

/// Command-line interface for Move transaction simulation.
#[derive(Parser)]
#[clap(about = "Tool for Move transaction simulation")]
pub enum SimulateMoveToolCommand {
    #[clap(about = "Simulate a publish command of move tool.")]
    Publish(PublishPackage),
    #[clap(about = "Simulate Run function command of move tool.")]
    Run(RunFunction),
    #[clap(about = "Simulate Run-script command of move tool.")]
    RunScript(RunScript),
    #[clap(about = "Simulate coin transfer command of move account.")]
    Transfer(TransferCoins),
}

impl SimulateMoveToolCommand {
    /// Executes the MoveTool command.
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        match self {
            SimulateMoveToolCommand::Publish(tool) => {
                let txn_argument = tool
                    .supra_command_arguments()
                    .await
                    .map_err(|e| CliError::GeneralError(e.to_string()))?;
                MoveTransactionOptions::from(txn_argument)
                    .simulate_transaction()
                    .await
                    .map(MoveToolResult::TransactionInfo)
            }
            SimulateMoveToolCommand::Run(tool) => {
                let txn_argument = tool
                    .supra_command_arguments()
                    .await
                    .map_err(|e| CliError::GeneralError(e.to_string()))?;
                MoveTransactionOptions::from(txn_argument)
                    .simulate_transaction()
                    .await
                    .map(MoveToolResult::TransactionInfo)
            }
            SimulateMoveToolCommand::RunScript(tool) => {
                let txn_argument = tool
                    .supra_command_arguments()
                    .await
                    .map_err(|e| CliError::GeneralError(e.to_string()))?;
                MoveTransactionOptions::from(txn_argument)
                    .simulate_transaction()
                    .await
                    .map(MoveToolResult::TransactionInfo)
            }
            SimulateMoveToolCommand::Transfer(tool) => tool.simulate().await,
        }
    }
}

impl Display for SimulateMoveToolCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let suffix = match self {
            SimulateMoveToolCommand::Publish(_) => "Publish",
            SimulateMoveToolCommand::Run(_) => "Run",
            SimulateMoveToolCommand::RunScript(_) => "RunScript",
            SimulateMoveToolCommand::Transfer(_) => "Transfer",
        };
        write!(f, "Simulate::{}", suffix)
    }
}
