use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::action::{RpcAction, RpcResponse};
use crate::rest_utils::client::supra_rpc_client::RpcClientApi;
use crate::rest_utils::client::RPCClientIfc;
use crate::rest_utils::rpc_request_kind::public_query::PublicQueryHelper;
use crate::rest_utils::rpc_request_kind::RpcRequestKind;
use crate::rest_utils::tool_option::SupraTransactionOptions;
use clap::Parser;

/// Fund an account with tokens from a faucet url
///
/// This will create an account if it doesn't exist with the faucet.
#[derive(Debug, Parser)]
pub struct FundWithFaucet {
    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,
}

impl FundWithFaucet {
    pub async fn execute(&self) -> Result<MoveToolResult, CliError> {
        match RpcClientApi::new_with_action(
            RpcAction::FundWithFaucet(
                self.transaction_option.api_version(),
                *self
                    .transaction_option
                    .profile_attribute()?
                    .account_address(),
                self.transaction_option
                    .profile_attribute()?
                    .faucet_url()
                    .clone(),
            ),
            RpcRequestKind::PublicQuery(PublicQueryHelper::new(
                self.transaction_option
                    .profile_attribute()?
                    .faucet_url()
                    .clone(),
            )),
        )?
        .execute_action()
        .await?
        {
            RpcResponse::FundWithFaucet(response) => Ok(MoveToolResult::FundWithFaucet(response)),
            _ => Err(CliError::RESTClient),
        }
    }
}
