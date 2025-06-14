use crate::error::{Error, Result};
use crate::rest::{controller, ApiServerState};
use ntex::web;
use ntex::web::types::{Path, Query, State};
use ntex::web::HttpResponse;
use socrypto::Hash;
use tracing::debug;
use types::api::v3::Transaction;
use types::api::{TransactionQueryType, X_SUPRA_CHAIN_ID};

/// Get transaction by hash v3
///
/// Get information about a transaction by its hash.
#[utoipa::path(
    get,
    path = "/rpc/v3/transactions/{hash}",
    operation_id = "transaction_by_hash_v3",
    params(TransactionQueryType,
        ("hash" = String, Path, description = "Hash of the transaction to lookup"),
    ),
    responses(
        (status = 200,
            description = "Transaction data of the given transaction hash",
            headers(
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
            ),
            content((Transaction = "application/json"))
        ),
    ),
)]
#[web::get("/{hash}")]
pub async fn transaction_by_hash(
    state: State<ApiServerState>,
    hash: Path<Hash>,
    typ: Query<TransactionQueryType>,
) -> Result<HttpResponse> {
    debug!("Received a V3 api query for transaction {hash}");
    let txn_hash = hash.into_inner();
    let txn_type = typ.into_inner();
    let txn = controller::transaction::get_transaction_by_hash(txn_hash, txn_type.value, &state)?
        .ok_or_else(|| Error::InformationNotAvailable(txn_hash.to_string()))?;

    Ok(HttpResponse::Ok()
        .header(X_SUPRA_CHAIN_ID, state.chain_id_header_value())
        .json(&txn))
}
