use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::tool_option::{MoveTransactionOptions, ResolveTransactionHash};
use aptos::account::multisig_account;
use clap::Subcommand;
use serialize_proposal::SerializeProposal;
use std::fmt::{Display, Formatter};
use supra_aptos::SupraCommand;

mod serialize_proposal;
pub(crate) mod verify_proposal;

#[allow(clippy::large_enum_variant)] // CLI does not impact runtime performance.
/// Tool for interacting with multisig accounts
#[allow(clippy::large_enum_variant)] // OK because CLI does not impact runtime performance.
#[derive(Debug, Subcommand)]
pub enum MultisigAccountTool {
    Approve(multisig_account::Approve),
    Create(multisig_account::Create),
    CreateTransaction(multisig_account::CreateTransaction),
    Execute(multisig_account::Execute),
    ExecuteReject(multisig_account::ExecuteReject),
    ExecuteWithPayload(multisig_account::ExecuteWithPayload),
    Reject(multisig_account::Reject),
    SerializeProposal(SerializeProposal),
    VerifyProposal(Box<verify_proposal::VerifyProposal>),
}

impl Display for MultisigAccountTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl MultisigAccountTool {
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        let supra_cmd_arg = match self {
            Self::Approve(tool) => tool.supra_command_arguments().await?,
            Self::Create(tool) => tool.supra_command_arguments().await?,
            Self::CreateTransaction(tool) => tool.supra_command_arguments().await?,
            Self::Execute(tool) => tool.supra_command_arguments().await?,
            Self::ExecuteReject(tool) => tool.supra_command_arguments().await?,
            Self::ExecuteWithPayload(tool) => tool.supra_command_arguments().await?,
            Self::Reject(tool) => tool.supra_command_arguments().await?,
            Self::SerializeProposal(tool) => return tool.execute(),
            Self::VerifyProposal(tool) => return tool.execute().await,
        };
        MoveTransactionOptions::from(supra_cmd_arg)
            .send_transaction(ResolveTransactionHash::UntilSuccess)
            .await
            .map(MoveToolResult::TransactionInfo)
    }
}
