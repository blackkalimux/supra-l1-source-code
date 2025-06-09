use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use aptos_cached_packages::aptos_stdlib;
use clap::Parser;

/// Create a new account on-chain
///
/// An account can be created by transferring coins, or by making an explicit
/// call to create an account.  This will create an account with no coins, and
/// any coins will have to transferred afterward.
#[derive(Debug, Parser)]
pub struct CreateAccount {
    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,
}

impl CreateAccount {
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        MoveTransactionOptions::new(
            aptos_stdlib::supra_account_create_account(
                *self
                    .transaction_option
                    .profile_attribute()?
                    .account_address(),
            ),
            self.transaction_option,
        )
        .send_transaction(ResolveTransactionHash::UntilSuccess)
        .await
        .map(MoveToolResult::TransactionInfo)
    }
}
