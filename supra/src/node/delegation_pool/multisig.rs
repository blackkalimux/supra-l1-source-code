use super::DelegationPoolResult;
use crate::common::error::CliError;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use aptos_cached_packages::aptos_stdlib;
use aptos_types::account_address::AccountAddress;
use aptos_types::transaction::{
    EntryFunction, Multisig, MultisigTransactionPayload, TransactionPayload,
};
use soserde::SmrSerialize;
use std::str::FromStr;

/// Executes a multisig transaction. `payload` may be [None] if the transaction is already stored
/// on-chain.
pub async fn execute(
    multisig_address: &str,
    payload: Option<MultisigTransactionPayload>,
    transaction_options: SupraTransactionOptions,
) -> Result<DelegationPoolResult, CliError> {
    let multisig_payload = TransactionPayload::Multisig(Multisig {
        multisig_address: AccountAddress::from_str(multisig_address)?,
        transaction_payload: payload,
    });
    MoveTransactionOptions::new(multisig_payload, transaction_options)
        .send_transaction(ResolveTransactionHash::UntilSuccess)
        .await
        .map(Box::new)
        .map(DelegationPoolResult::TransactionInfo)
}

/// Proposes a transaction payload for execution subject to the approval of a threshold number of
/// the participants in the multisig. The payload will be stored on-chain until it is executed.
pub async fn propose(
    multisig_owner_address: String,
    payload: EntryFunction,
    transaction_options: SupraTransactionOptions,
) -> Result<DelegationPoolResult, CliError> {
    let multisig_transaction_payload = MultisigTransactionPayload::EntryFunction(payload);
    let multisig_payload = aptos_stdlib::multisig_account_create_transaction(
        AccountAddress::from_str(&multisig_owner_address)?,
        multisig_transaction_payload.to_bytes(),
    );
    MoveTransactionOptions::new(multisig_payload, transaction_options)
        .send_transaction(ResolveTransactionHash::UntilSuccess)
        .await
        .map(Box::new)
        .map(DelegationPoolResult::TransactionInfo)
}
