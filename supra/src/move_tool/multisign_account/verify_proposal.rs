use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::action::{RpcAction, RpcResponse};
use crate::rest_utils::client::supra_rpc_client::RpcClientApi;
use crate::rest_utils::client::RPCClientIfc;
use crate::rest_utils::rpc_request_kind::public_query::PublicQueryHelper;
use crate::rest_utils::rpc_request_kind::RpcRequestKind;
use crate::rest_utils::tool_option::SupraTransactionOptions;
use aptos::common::types::{EntryFunctionArguments, MultisigAccountWithSequenceNumber};
use aptos::move_tool::ArgWithType;
use aptos_api_types::{
    Address, EntryFunctionId, HexEncodedBytes, IdentifierWrapper, MoveModuleId, ViewRequest,
};
use aptos_crypto::HashValue;
use aptos_types::transaction::MultisigTransactionPayload;
use bcs::to_bytes;
use clap::Parser;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use reqwest::Response;
use serde_json::{json, Value};

/// Verify entry function matches on-chain transaction proposal.
#[derive(Debug, Parser)]
pub struct VerifyProposal {
    #[clap(flatten)]
    pub(crate) multisig_account_with_sequence_number: MultisigAccountWithSequenceNumber,
    #[clap(flatten)]
    pub(crate) txn_options: SupraTransactionOptions,
    #[clap(flatten)]
    pub(crate) entry_function_args: EntryFunctionArguments,
}

impl VerifyProposal {
    /// Constructs a view request to retrieve transaction details for the specified multisig account.
    fn construct_view_request(&self) -> Result<ViewRequest, CliError> {
        Ok(ViewRequest {
            function: EntryFunctionId {
                module: MoveModuleId {
                    address: Address::from(AccountAddress::ONE),
                    name: IdentifierWrapper::from(ident_str!("multisig_account")),
                },
                name: IdentifierWrapper::from(ident_str!("get_transaction")),
            },
            type_arguments: vec![],
            arguments: vec![
                serde_json::to_value(
                    self.multisig_account_with_sequence_number
                        .multisig_account()
                        .multisig_address(),
                )?,
                ArgWithType::u64(self.multisig_account_with_sequence_number.sequence_number())
                    .to_json()?,
            ],
        })
    }

    /// Extracts the payload hash from the API response body.
    fn retrieve_payload_hash_from_response(body: &str) -> Result<String, CliError> {
        let value: Value = serde_json::from_str(body)?;
        let api_result = value
            .get("result")
            .and_then(|r| r.get(0))
            .ok_or_else(|| CliError::GeneralError("Api did not return result".to_string()))?;
        let may_be_payload = api_result
            .get("payload")
            .and_then(|actual_payload| actual_payload.get("vec"))
            .and_then(|v| v.as_array())
            .and_then(|v| v.first())
            .cloned();

        match may_be_payload {
            Some(hex_encoded_bytes) => Ok(HashValue::sha3_256_of(
                serde_json::from_value::<HexEncodedBytes>(hex_encoded_bytes)?.inner(),
            )
            .to_hex_literal()),
            None => {
                if let Some(payload_hash) = api_result.get("payload_hash") {
                    let inner_vec = payload_hash
                        .get("vec")
                        .and_then(|v| v.as_array())
                        .and_then(|v| v.first())
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            CliError::GeneralError("Invalid payload_hash format".to_string())
                        })?;
                    Ok(inner_vec.to_string())
                } else {
                    Err(CliError::GeneralError(
                        "Neither payload nor payload hash provided on-chain".to_string(),
                    ))
                }
            }
        }
    }

    /// Compares the calculated payload hash with the on-chain transaction payload hash.
    fn verify_payload_hashes(
        calculated_payload_hash: String,
        onchain_txn_payload_hash: &String,
    ) -> Result<MoveToolResult, CliError> {
        match calculated_payload_hash.eq(onchain_txn_payload_hash) {
            true => Ok(MoveToolResult::MoveCli(json!({
            "Status": "Transaction match"
            }))),
            false => Err(CliError::GeneralError(format!(
                "Transaction mismatch:\nThe transaction you provided has a payload hash of \
                {calculated_payload_hash}, but the on-chain transaction proposal you specified has \
                a payload hash of {onchain_txn_payload_hash}."
            ))),
        }
    }

    // Processes the API response from a view function call.
    async fn handle_view_api_response(
        self,
        response: Response,
    ) -> Result<MoveToolResult, CliError> {
        match response.status().is_success() {
            true => {
                let body = response.text().await?;
                eprintln!("{}", body);
                let onchain_txn_payload_hash = Self::retrieve_payload_hash_from_response(&body)?;
                let multisign_transaction_payload = &self.entry_function_args.try_into()?;
                let calculated_payload_hash =
                    HashValue::sha3_256_of(&to_bytes::<MultisigTransactionPayload>(
                        multisign_transaction_payload,
                    )?)
                    .to_hex_literal();
                Self::verify_payload_hashes(calculated_payload_hash, &onchain_txn_payload_hash)
            }
            false => Err(CliError::GeneralError(response.text().await?)),
        }
    }

    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        match RpcClientApi::new_with_action(
            RpcAction::RunViewFunction(
                self.txn_options.api_version(),
                self.construct_view_request()?,
            ),
            RpcRequestKind::PublicQuery(PublicQueryHelper::new(
                self.txn_options.profile_attribute()?.rpc_url().clone(),
            )),
        )?
        .execute_action()
        .await?
        {
            RpcResponse::RunViewFunction(response) => {
                Ok(self.handle_view_api_response(response).await?)
            }
            _ => Err(CliError::GeneralError("rpc client".to_string())),
        }
    }
}
