use crate::common::error::CliError;
use crate::move_tool::error::MoveToolError;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::action::{RpcAction, RpcResponse};
use crate::rest_utils::client::supra_rpc_client::RpcClientApi;
use crate::rest_utils::client::RPCClientIfc;
use crate::rest_utils::rpc_request_kind::public_query::PublicQueryHelper;
use crate::rest_utils::rpc_request_kind::RpcRequestKind;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use aptos::account::derive_resource_account::ResourceAccountSeed;
use aptos_cached_packages::aptos_framework_sdk_builder::resource_account_create_resource_account;
use aptos_types::transaction::authenticator::AuthenticationKey;
use aptos_types::transaction::TransactionPayload;
use clap::Parser;
use move_core_types::language_storage::TypeTag;
use std::str::FromStr;

/// Create a resource account on-chain
///
/// This will create a resource account which can be used as an autonomous account
/// not controlled directly by one account.
#[derive(Debug, Parser)]
pub struct CreateResourceAccount {
    /// Optional Resource Account authentication key.
    #[clap(long, value_parser = AuthenticationKey::from_str)]
    pub(crate) authentication_key: Option<AuthenticationKey>,

    #[clap(flatten)]
    pub(crate) seed_args: ResourceAccountSeed,
    #[clap(flatten)]
    pub(crate) txn_options: SupraTransactionOptions,
}

impl CreateResourceAccount {
    async fn fetch_resource_account(
        transaction_option: SupraTransactionOptions,
    ) -> Result<MoveToolResult, CliError> {
        match RpcClientApi::new_with_action(
            RpcAction::AccountResource(
                transaction_option.api_version(),
                *transaction_option.profile_attribute()?.account_address(),
                TypeTag::from_str("0x1::resource_account::Container")?,
            ),
            RpcRequestKind::PublicQuery(PublicQueryHelper::new(
                transaction_option.profile_attribute()?.rpc_url().clone(),
            )),
        )?
        .execute_action()
        .await?
        {
            RpcResponse::AccountResource(response) => {
                let body = response.text().await.map_err(MoveToolError::ReqwestError)?;

                Ok(MoveToolResult::MoveCli(serde_json::from_str(&body)?))
            }
            _ => Err(CliError::RESTClient),
        }
    }

    fn transaction_payload(
        authentication_key: Option<AuthenticationKey>,
        seed_args: ResourceAccountSeed,
    ) -> Result<TransactionPayload, CliError> {
        let authentication_key: Vec<u8> = if let Some(key) = authentication_key {
            bcs::to_bytes(&key)?
        } else {
            vec![]
        };
        Ok(resource_account_create_resource_account(
            seed_args.seed()?,
            authentication_key,
        ))
    }

    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        let CreateResourceAccount {
            authentication_key,
            seed_args,
            txn_options,
        } = self;
        MoveTransactionOptions::new(
            Self::transaction_payload(authentication_key, seed_args)?,
            txn_options.clone(),
        )
        .send_transaction(ResolveTransactionHash::UntilSuccess)
        .await?;
        Self::fetch_resource_account(txn_options).await
    }
}
