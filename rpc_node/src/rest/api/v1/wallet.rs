#![allow(deprecated)] // To support API v1.

use crate::error::{Error, Result};
use crate::rest::api::v1::transactions::transaction_by_hash_internal;
use crate::rest::ApiServerState;
use move_core_types::account_address::AccountAddress;
use ntex::web;
use ntex::web::types::{Json, Path, State};
use socrypto::Hash;
use tracing::debug;
use types::api::v1::{FaucetStatus, TransactionInfoV1};

/// Faucet v1
///
/// funds account with [FUND_AMOUNT] coins
#[utoipa::path(
    get,
    path = "/rpc/v1/wallet/faucet/{address}",
    operation_id = "faucet_v1",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
    ),
    responses(
        (status = 200, description = "list of associated transactions created", content((FaucetStatus = "application/json")))
    ),
)]
#[web::get("/{address}")]
pub async fn faucet(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
) -> Result<Json<FaucetStatus>> {
    let account = addr.into_inner();
    debug!("Faucet request for {account}");

    let faucet_state = state.faucet_state.as_ref().ok_or(Error::FaucetNotEnabled)?;

    faucet_state
        .faucet_call_limiter
        .write()
        .await
        .register_account(account)?;

    let faucet_response = faucet_state
        .faucet
        .read()
        .await
        .try_fund(account)
        .await
        .map(FaucetStatus::Accepted)
        .unwrap_or(FaucetStatus::TryLater);

    // Increment the account's faucet use count if the faucet was able to dispatch a transaction
    // to the attached validator.
    if let (limiter, FaucetStatus::Accepted(_)) =
        (&faucet_state.faucet_call_limiter, &faucet_response)
    {
        limiter
            .write()
            .await
            .increment_account_use_counter_if_under_limit(account);
    }

    Ok(Json(faucet_response))
}

/// Faucet transaction v1
///
/// Get information about a faucet transaction by its hash.
#[utoipa::path(
    get,
    path = "/rpc/v1/wallet/faucet/transactions/{hash}",
    operation_id = "faucet_transaction_by_hash_v1",
    params(
        ("hash" = String, Path, description = "Hash of the faucet transaction to lookup")
    ),
    responses(
        (status = 200, description = "Faucet transaction data of the given transaction hash", content((Option<TransactionInfoV1> = "application/json")))
    ),
)]
#[web::get("/transactions/{hash}")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn faucet_transaction_by_hash(
    state: State<ApiServerState>,
    hash: Path<Hash>,
) -> Result<Json<Option<TransactionInfoV1>>> {
    transaction_by_hash_internal(state, hash.into_inner()).await
}
