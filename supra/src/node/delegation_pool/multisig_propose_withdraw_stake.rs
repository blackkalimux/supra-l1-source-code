use super::multisig::propose;
use super::DelegationPoolResult;
use crate::common::error::CliError;
use crate::rest_utils::tool_option::SupraTransactionOptions;
use crate::utils::parse_pool_address;
use aptos_cached_packages::aptos_stdlib;
use clap::Args;

/// Arguments for creating a multisig transaction proposal to withdraw stake from a PBO stake pool.
#[derive(Args, Debug)]
pub struct ProposeWithdrawStake {
    /// The amount of stake to withdraw from the PBO delegation pool.
    #[arg(short, long)]
    pub amount: u64,

    /// MultiSig Owner address for the Pool
    #[arg(short, long)]
    pub multisig_owner_address: String,

    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,
}

impl ProposeWithdrawStake {
    pub async fn execute(self) -> Result<DelegationPoolResult, CliError> {
        let pool_address =
            parse_pool_address(self.transaction_option.delegation_pool_address.as_deref())?;
        let payload = aptos_stdlib::pbo_delegation_pool_withdraw(pool_address, self.amount);
        propose(
            self.multisig_owner_address.clone(),
            payload.into_entry_function(),
            self.transaction_option,
        )
        .await
    }
}
