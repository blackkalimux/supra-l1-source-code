use crate::error::Result;
use crate::rest::api::v2::transactions::transient_transaction_by_hash;
use crate::rest::ApiServerState;
use ntex::web;
use ntex::web::types::{Json, Path, State};
use socrypto::Hash;
use types::api::v2::TransactionInfoV2;

/// Faucet transaction v2
///
/// Get information about a faucet transaction by its hash.
#[utoipa::path(
    get,
    path = "/rpc/v2/wallet/faucet/transactions/{hash}",
    operation_id = "faucet_v2",
    params(
        ("hash" = String, Path, description = "Hash of the faucet transaction to lookup")
    ),
    responses(
        (status = 200, description = "Faucet transaction data of the given transaction hash", content((TransactionInfoV2 = "application/json")))
    ),
)]
#[web::get("/transactions/{hash}")]
pub async fn faucet_transaction_by_hash(
    state: State<ApiServerState>,
    hash: Path<Hash>,
) -> Result<Json<TransactionInfoV2>> {
    transient_transaction_by_hash(state, hash.into_inner()).await
}
