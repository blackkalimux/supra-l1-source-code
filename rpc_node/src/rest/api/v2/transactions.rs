use crate::error::{Error, Result};
use crate::rest::ApiServerState;
use ntex::web;
use ntex::web::types::{Json, Path, State};
use socrypto::Hash;
use tracing::debug;
use transactions::TransactionKind;
use types::api;
use types::api::v2::{GasPriceResV2, TransactionInfoV2};
use types::transactions::transaction::SupraTransaction;

/// Estimate gas price v2
///
/// Get statistics derived from the gas prices of recently executed transactions.
#[utoipa::path(
    get,
    path = "/rpc/v2/transactions/estimate_gas_price",
    operation_id = "estimate_gas_price_v2",
    responses(
        (status = 200, description = "Returns the mean, median and maximum gas prices of recently executed transactions.", content((GasPriceResV2 = "application/json")))
    )
)]
#[web::get("/estimate_gas_price")]
pub async fn estimate_gas_price(state: State<ApiServerState>) -> Result<Json<GasPriceResV2>> {
    Ok(state
        .gas_price_reader
        .statistics(TransactionKind::MoveVm)
        .map(GasPriceResV2::from)
        .map(Json)
        .expect("Gas prices stats for MoveVM must be maintained"))
}

pub(crate) async fn transient_transaction_by_hash(
    state: State<ApiServerState>,
    txn_hash: Hash,
) -> Result<Json<TransactionInfoV2>> {
    debug!("Received a query for transaction {txn_hash}");
    let info = state
        .get_user_transaction_by_hash(txn_hash, true)?
        .ok_or(Error::InformationNotAvailable(txn_hash.to_string()))?;
    Ok(Json(info))
}

/// Get transaction by hash v2
///
/// Get information about a transaction by its hash.
#[utoipa::path(
    get,
    path = "/rpc/v2/transactions/{hash}",
    operation_id = "transaction_by_hash_v2",
    params(
        ("hash" = String, Path, description = "Hash of the transaction to lookup")
    ),
    responses(
        (status = 200, description = "Transaction data of the given transaction hash", content((TransactionInfoV2 = "application/json")))
    ),
)]
#[web::get("/{hash}")]
pub async fn transaction_by_hash(
    state: State<ApiServerState>,
    hash: Path<Hash>,
) -> Result<Json<TransactionInfoV2>> {
    transient_transaction_by_hash(state, hash.into_inner()).await
}

/// Simulate a transaction v2
#[utoipa::path(
    post,
    path = "/rpc/v2/transactions/simulate",
    operation_id = "simulate_txn_v2",
    request_body (content = api::delegates::SupraTransaction, content_type = "application/json"),
    responses(
        (status = 200, description = "Simulate a transaction against the current state of the RPC node. The transaction must have an invalid signature.", content(( TransactionInfoV2 = "application/json")))
    ),
)]
#[web::post("/simulate")]
pub async fn simulate_txn(
    state: State<ApiServerState>,
    req: Json<SupraTransaction>,
) -> Result<Json<TransactionInfoV2>> {
    debug!("Received simulation request for transaction: {req}");
    match req.0 {
        SupraTransaction::Smr(_) => Err(Error::BadTransactionRequest(
            "Only move transaction simulation is supported for now".to_string(),
        )),
        SupraTransaction::Move(txn) => state
            .run_move_transaction_simulation(txn)
            .await
            .map(Json)
            .map_err(Error::from),
    }
}
