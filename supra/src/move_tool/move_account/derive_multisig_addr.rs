use crate::common::error::CliError;
use crate::move_tool::error::MoveToolError;
use crate::move_tool::MoveToolResult;
use aptos_types::account_address::create_multisig_account_address;
use clap::Parser;
use move_core_types::account_address::AccountAddress;
use std::str::FromStr;

#[derive(Debug, Parser)]
pub struct DeriveMultiSigAddress {
    #[clap(long)]
    owner_address: String,
    #[clap(long)]
    sequence_number: u64,
}

impl DeriveMultiSigAddress {
    pub fn execute(&self) -> Result<MoveToolResult, CliError> {
        let addr = create_multisig_account_address(
            AccountAddress::from_str(self.owner_address.as_str()).map_err(|e| {
                CliError::MoveToolingError(MoveToolError::FailedToParseAddress(e.to_string()))
            })?,
            self.sequence_number,
        );

        Ok(MoveToolResult::DeriveMultiSigAddress(addr))
    }
}
