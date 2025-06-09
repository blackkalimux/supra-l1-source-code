use super::DelegationPoolResult;
use crate::common::error::CliError;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use aptos_cached_packages::aptos_stdlib;
use aptos_types::account_address::AccountAddress;
use clap::Args;

/// Arguments for setting new operator address for stake pool.
#[derive(Args, Debug)]
pub struct SetOperator {
    /// New operator account address must be "0x + 64 hex characters".
    /// The new operator will take effect immediately in stake pool.
    #[arg(short, long)]
    pub account: String,

    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,
}

impl SetOperator {
    pub async fn execute(self) -> Result<DelegationPoolResult, CliError> {
        let new_operator = AccountAddress::from_str_strict(&self.account).map_err(|e| {
            CliError::Aborted(
                format!("{} is not a valid account address. {e:?}", self.account),
                "The account address must be a valid 0x + 64 hex characters".to_owned(),
            )
        })?;

        let transaction_payload = aptos_stdlib::pbo_delegation_pool_set_operator(new_operator);
        let txn_info =
            MoveTransactionOptions::new(transaction_payload.clone(), self.transaction_option)
                .send_transaction(ResolveTransactionHash::UntilSuccess)
                .await
                .map(Box::new)?;
        Ok(DelegationPoolResult::TransactionInfo(txn_info))
    }
}
