use crate::common::error::CliError;
use crate::move_tool::automation::AutomationCommand;
use crate::move_tool::move_account::AccountTool;
use crate::move_tool::multisign_account::MultisigAccountTool;
use crate::move_tool::simulate::SimulateMoveToolCommand;
use crate::move_tool::tool::MoveToolCommand;
use crate::rest_utils::action::{CliMoveList, CliTransactionInfo};
use aptos::common::types::CliResult;
use clap::Parser;
use move_core_types::account_address::AccountAddress;
use serde::ser::Error;
use serde::Serialize;
use serde_json::Value;
use std::fmt::{Display, Formatter};
use types::api::v1::FaucetStatus;

mod automation;
pub mod bcs_decode;
pub mod bcs_encode;
pub mod error;
pub mod move_account;
pub mod multisign_account;
pub mod simulate;
pub mod tool;

/// Result type for the MoveTool operations.
#[derive(Debug, Serialize)]
pub enum MoveToolResult {
    MoveCli(Value),
    String(String),
    SimulationInfo(CliTransactionInfo),
    TransactionInfo(CliTransactionInfo),
    FundWithFaucet(FaucetStatus),
    ListAccountPackage(CliMoveList),
    DeriveMultiSigAddress(AccountAddress),
    DeriveResourceAccountAddress(AccountAddress),
}

impl MoveToolResult {
    fn write_pretty_struct<T>(res: &T, f: &mut Formatter<'_>) -> std::fmt::Result
    where
        T: ?Sized + Serialize,
    {
        writeln!(
            f,
            "{}",
            serde_json::to_string_pretty(res)
                .map_err(|e| std::fmt::Error::custom(e.to_string()))?
        )
    }

    pub(crate) fn try_as_move_tool_result(result: CliResult) -> Result<MoveToolResult, CliError> {
        result
            .map_err(CliError::GeneralError)
            .and_then(|body| serde_json::from_str(&body).map_err(CliError::SerdeJsonError))
            .map(MoveToolResult::MoveCli)
    }
}

impl Display for MoveToolResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveToolResult::MoveCli(res) => MoveToolResult::write_pretty_struct(res, f),
            MoveToolResult::SimulationInfo(res) => MoveToolResult::write_pretty_struct(res, f),
            MoveToolResult::TransactionInfo(res) => MoveToolResult::write_pretty_struct(res, f),
            MoveToolResult::FundWithFaucet(res) => MoveToolResult::write_pretty_struct(res, f),
            MoveToolResult::ListAccountPackage(res) => MoveToolResult::write_pretty_struct(res, f),
            MoveToolResult::DeriveMultiSigAddress(res) => {
                MoveToolResult::write_pretty_struct(res, f)
            }
            MoveToolResult::DeriveResourceAccountAddress(res) => {
                MoveToolResult::write_pretty_struct(res, f)
            }
            MoveToolResult::String(res) => writeln!(f, "{}", res),
        }
    }
}

/// Supra CLI target the MoveVM.
#[derive(Parser)]
pub enum MoveTool {
    #[clap(subcommand)]
    Account(AccountTool),
    #[clap(subcommand)]
    Multisig(MultisigAccountTool),
    #[clap(subcommand)]
    Tool(MoveToolCommand),
    #[clap(subcommand)]
    Simulate(SimulateMoveToolCommand),
    #[clap(subcommand)]
    Automation(AutomationCommand),
}

impl Display for MoveTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveTool::Account(tool) => write!(f, "{tool}"),
            MoveTool::Tool(tool) => write!(f, "{tool}"),
            MoveTool::Simulate(tool) => write!(f, "{tool}"),
            MoveTool::Multisig(tool) => write!(f, "{tool}"),
            MoveTool::Automation(tool) => write!(f, "{tool}"),
        }
    }
}

impl MoveTool {
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        match self {
            MoveTool::Account(tool) => tool.execute().await,
            MoveTool::Tool(tool) => tool.execute().await,
            MoveTool::Simulate(tool) => tool.execute().await,
            MoveTool::Multisig(tool) => tool.execute().await,
            MoveTool::Automation(command) => command.execute().await,
        }
    }
}
