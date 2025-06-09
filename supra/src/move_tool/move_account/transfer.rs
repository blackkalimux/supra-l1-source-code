use crate::common::error::CliError;
use crate::move_tool::move_account::SupraCoinTypeResolver;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use aptos_cached_packages::aptos_stdlib;
use aptos_types::account_address::AccountAddress;
use aptos_types::transaction::TransactionPayload;
use clap::Parser;

/// Transfer SUPRA between accounts
///
#[derive(Debug, Parser)]
pub struct TransferCoins {
    /// Address of account to send SUPRA to
    #[clap(long)]
    pub(crate) account: AccountAddress,

    /// Amount of Quants (10^-8 SUPRA) to transfer
    #[clap(long)]
    pub(crate) amount: u64,

    /// Coin type to transfer.  Defaults to 0x1::supra_coin::SupraCoin
    #[clap(long)]
    pub(crate) coin_type: Option<String>,

    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,
}

impl TransferCoins {
    fn transaction_payload(&self) -> Result<TransactionPayload, CliError> {
        Ok(aptos_stdlib::supra_account_transfer_coins(
            SupraCoinTypeResolver::TransferCoins.resolve(&self.coin_type)?,
            self.account,
            self.amount,
        ))
    }

    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        MoveTransactionOptions::new(self.transaction_payload()?, self.transaction_option)
            .send_transaction(ResolveTransactionHash::UntilSuccess)
            .await
            .map(MoveToolResult::TransactionInfo)
    }

    pub async fn simulate(self) -> Result<MoveToolResult, CliError> {
        MoveTransactionOptions::new(self.transaction_payload()?, self.transaction_option)
            .simulate_transaction()
            .await
            .map(MoveToolResult::TransactionInfo)
    }
}
