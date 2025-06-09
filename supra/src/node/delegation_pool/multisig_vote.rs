use super::DelegationPoolResult;
use crate::common::error::CliError;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use aptos_cached_packages::aptos_stdlib;
use aptos_types::account_address::AccountAddress;
use clap::Args;
use std::str::FromStr;

// TODO: Does this need to be moved to another crate? Looks like it can be used for any type
// of multisig vote.
//
/// Arguments for creating a vote for a multisig proposal related to a PBO delegation pool.
#[derive(Args, Debug)]
pub struct MultisigVote {
    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,

    /// MultiSig Owner address for the Pool
    #[arg(short, long)]
    pub multisig_owner_address: String,

    /// Sequence number of the transaction to submit vote for
    #[arg(short, long)]
    pub sequence_number: u64,

    /// Vote for or Vote against
    #[arg(short, long)]
    pub approved: bool,
}

impl MultisigVote {
    pub async fn execute(self) -> Result<DelegationPoolResult, CliError> {
        let payload = aptos_stdlib::multisig_account_vote_transaction(
            AccountAddress::from_str(&self.multisig_owner_address)?,
            self.sequence_number,
            self.approved,
        );
        MoveTransactionOptions::new(payload, self.transaction_option)
            .send_transaction(ResolveTransactionHash::UntilSuccess)
            .await
            .map(Box::new)
            .map(DelegationPoolResult::TransactionInfo)
    }
}
