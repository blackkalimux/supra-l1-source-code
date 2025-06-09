use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use aptos::common::types::EntryFunctionArguments;
use aptos_api_types::HexEncodedBytes;
use aptos_types::transaction::MultisigTransactionPayload;
use bcs::to_bytes;
use clap::Parser;

/// Verify entry function matches on-chain transaction proposal.
#[derive(Debug, Parser)]
pub struct SerializeProposal {
    #[clap(flatten)]
    pub(crate) entry_function_args: EntryFunctionArguments,
}

impl SerializeProposal {
    pub fn execute(self) -> Result<MoveToolResult, CliError> {
        let payload = &self.entry_function_args.try_into()?;
        let payload_bcs = to_bytes::<MultisigTransactionPayload>(payload)?;
        let payload_hex = HexEncodedBytes::from(payload_bcs);
        let hex_string = serde_json::to_value(&payload_hex)?;
        let result = MoveToolResult::MoveCli(hex_string);
        Ok(result)
    }
}
