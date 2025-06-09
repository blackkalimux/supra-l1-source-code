use super::multisig::execute;
use super::DelegationPoolResult;
use crate::common::error::CliError;
use crate::rest_utils::tool_option::SupraTransactionOptions;
use clap::Args;

// TODO: Should this be moved to a separate crate since it seems to be independent of pools?
//
/// Arguments for executing a transaction that was previously proposed via a MultiSig account.
#[derive(Args, Debug)]
pub struct MultisigExecuteStoredPayload {
    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,

    /// Address of the target multisig account.
    #[arg(short, long)]
    pub multisig_address: String,
}

impl MultisigExecuteStoredPayload {
    pub async fn execute(self) -> Result<DelegationPoolResult, CliError> {
        execute(&self.multisig_address, None, self.transaction_option).await
    }
}
