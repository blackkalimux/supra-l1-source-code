use crate::common::error::CliError;
use crate::move_tool::error::MoveToolError;
use crate::move_tool::move_account::SupraCoinTypeResolver;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::action::{RpcAction, RpcResponse};
use crate::rest_utils::client::supra_rpc_client::RpcClientApi;
use crate::rest_utils::client::RPCClientIfc;
use crate::rest_utils::rpc_request_kind::public_query::PublicQueryHelper;
use crate::rest_utils::rpc_request_kind::RpcRequestKind;
use crate::rest_utils::tool_option::SupraTransactionOptions;
use clap::Parser;
use tracing::debug;

/// Show the account's balance of supra coins
#[derive(Debug, Parser)]
pub struct Balance {
    /// Coin type to lookup.  Defaults to 0x1::supra_coin::SupraCoin
    #[clap(long)]
    pub(crate) coin_type: Option<String>,

    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,
}

impl Balance {
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        match RpcClientApi::new_with_action(
            RpcAction::AccountResource(
                self.transaction_option.api_version(),
                *self
                    .transaction_option
                    .profile_attribute()?
                    .account_address(),
                SupraCoinTypeResolver::CoinResource.resolve(&self.coin_type)?,
            ),
            RpcRequestKind::PublicQuery(PublicQueryHelper::new(
                self.transaction_option
                    .profile_attribute()?
                    .rpc_url()
                    .clone(),
            )),
        )?
        .execute_action()
        .await?
        {
            RpcResponse::AccountResource(response) => {
                debug!("Response {:?}", response);

                let body = response.text().await.map_err(MoveToolError::ReqwestError)?;

                Ok(MoveToolResult::MoveCli(serde_json::from_str(&body)?))
            }
            _ => Err(CliError::RESTClient),
        }
    }
}
