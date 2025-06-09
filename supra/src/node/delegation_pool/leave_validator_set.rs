use super::DelegationPoolResult;
use crate::common::error::CliError;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use crate::utils::parse_pool_address;
use aptos_cached_packages::aptos_stdlib;
use clap::Args;

/// Arguments for creating a transaction to remove the validator managed by the given CLI profile
/// from the active validator set. The validator will immediately be moved to the "pending inactive"
/// state and will be removed from the validator set when the next epoch begins.
#[derive(Args, Debug)]
pub struct LeaveValidatorSet {
    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,
}

impl LeaveValidatorSet {
    pub async fn execute(self) -> Result<DelegationPoolResult, CliError> {
        let pool_address =
            parse_pool_address(self.transaction_option.delegation_pool_address.as_deref())?;
        let payload = aptos_stdlib::stake_leave_validator_set(pool_address);
        MoveTransactionOptions::new(payload, self.transaction_option)
            .send_transaction(ResolveTransactionHash::UntilSuccess)
            .await
            .map(Box::new)
            .map(DelegationPoolResult::TransactionInfo)
    }
}
