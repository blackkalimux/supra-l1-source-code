use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::tool_option::{MoveTransactionOptions, ResolveTransactionHash};
use aptos::common::types::CliCommand;
use aptos::governance::{
    ApproveExecutionHash, ExecuteProposal, GenerateExecutionHash, GenerateUpgradeProposal,
    SubmitProposal, SubmitVote, VerifyProposal,
};
use clap::Parser;
use std::fmt::{Display, Formatter};
use supra_aptos::SupraCommand;

/// Tool for on-chain governance
///
/// This tool allows voters that have stake to vote the ability to
/// propose changes to the chain, as well as vote and execute these
/// proposals.
#[derive(Parser)]
pub enum GovernanceTool {
    Propose(SubmitProposal),
    Vote(SubmitVote),
    VerifyProposal(VerifyProposal),
    ExecuteProposal(ExecuteProposal),
    GenerateUpgradeProposal(GenerateUpgradeProposal),
    ApproveExecutionHash(ApproveExecutionHash),
    GenerateExecutionHash(GenerateExecutionHash),
}

impl Display for GovernanceTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f)
    }
}

impl GovernanceTool {
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        match self {
            Self::Propose(tool) => {
                let txn_argument = tool
                    .supra_command_arguments()
                    .await
                    .map_err(|e| CliError::GeneralError(e.to_string()))?;
                MoveTransactionOptions::from(txn_argument)
                    .send_transaction(ResolveTransactionHash::UntilSuccess)
                    .await
                    .map(MoveToolResult::TransactionInfo)
            }
            Self::Vote(tool) => {
                let txn_argument = tool
                    .supra_command_arguments()
                    .await
                    .map_err(|e| CliError::GeneralError(e.to_string()))?;
                MoveTransactionOptions::from(txn_argument)
                    .send_transaction(ResolveTransactionHash::UntilSuccess)
                    .await
                    .map(MoveToolResult::TransactionInfo)
            }
            Self::VerifyProposal(_tool) => {
                todo!("RPC endpoint for getting items from table handler")
            }
            Self::ExecuteProposal(tool) => {
                let txn_argument = tool
                    .supra_command_arguments()
                    .await
                    .map_err(|e| CliError::GeneralError(e.to_string()))?;
                MoveTransactionOptions::from(txn_argument)
                    .send_transaction(ResolveTransactionHash::UntilSuccess)
                    .await
                    .map(MoveToolResult::TransactionInfo)
            }
            Self::GenerateUpgradeProposal(tool) => tool
                .execute_serialized()
                .await
                .map_err(CliError::GeneralError)
                .and_then(|body| serde_json::from_str(&body).map_err(CliError::SerdeJsonError))
                .map(MoveToolResult::MoveCli),
            Self::ApproveExecutionHash(tool) => {
                let txn_argument = tool
                    .supra_command_arguments()
                    .await
                    .map_err(|e| CliError::GeneralError(e.to_string()))?;
                MoveTransactionOptions::from(txn_argument)
                    .send_transaction(ResolveTransactionHash::UntilSuccess)
                    .await
                    .map(MoveToolResult::TransactionInfo)
            }
            Self::GenerateExecutionHash(tool) => tool
                .generate_hash()
                .map_err(|e| CliError::GeneralError(e.to_string()))
                .map(|hash| MoveToolResult::String(hash.1.to_hex())),
        }
    }
}
