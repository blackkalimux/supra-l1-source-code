use crate::error::{Error, Result};
use crate::rest::api::v1::block::BlockByHeightQueryV3;
use crate::rest::controller::block::{
    get_block_metadata_info_by_height, get_txns_by_block_hash_and_type,
};
use crate::rest::ApiServerState;
use ntex::web;
use ntex::web::types::{Path, Query, State};
use ntex::web::HttpResponse;
use socrypto::Hash;
use tracing::{debug, trace};
use types::api::v3::block_info::ExecutedBlockInfo;
use types::api::v3::BlockV3;
use types::api::{delegates, TransactionQueryType, X_SUPRA_CHAIN_ID};

/// Get latest block v3
///
/// Get the meta information of the most recently finalized and executed block.
#[utoipa::path(
    get,
    path = "/rpc/v3/block",
    operation_id = "latest_block_v3",
    responses(
        (status = 200,
            description = "Returns the header of the most recently finalized block.",
            headers (
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
            ),
            content(
                (ExecutedBlockInfo = "application/json" ),
            )
        ),
    ),
)]
#[web::get("")]
pub async fn latest_block(state: State<ApiServerState>) -> Result<HttpResponse> {
    let latest_block = state
        .storage
        .archive()
        .reader()
        .get_latest_block_header_info()?
        .ok_or(Error::InformationNotAvailable("Latest Block".to_string()))?;
    let stats = state.get_block_execution_statistics_by_height(latest_block.height)?;
    let block_info = ExecutedBlockInfo::new(latest_block, stats);
    Ok(HttpResponse::Ok()
        .header(X_SUPRA_CHAIN_ID, state.chain_id_header_value())
        .json(&block_info))
}

/// Get block by hash v3
///
/// Get the header and execution output statistics of the block with the given hash.
#[utoipa::path(
    get,
    path = "/rpc/v3/block/{block_hash}",
    operation_id = "block_info_by_hash_v3",
    params(
        ("block_hash" = String, Path, description = "Hex encoded block hash")
    ),
    responses(
        (status = 200,
            description = "The header of the block with the given hash.",
            headers (
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
            ),
            content(
                (ExecutedBlockInfo = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{block_hash}")]
pub async fn block_info_by_hash(
    state: State<ApiServerState>,
    block_hash: Path<Hash>,
) -> Result<HttpResponse> {
    let block_hash = block_hash.into_inner();
    debug!("query for block = {}", block_hash.to_string());
    let block_header_info = state
        .storage
        .archive()
        .reader()
        .get_block_header_info(block_hash)?
        .ok_or(Error::InformationNotAvailable(block_hash.to_string()))?;
    let stats = state.get_block_execution_statistics_by_height(block_header_info.height)?;
    let block_info = ExecutedBlockInfo::new(block_header_info, stats);
    Ok(HttpResponse::Ok()
        .header(X_SUPRA_CHAIN_ID, state.chain_id_header_value())
        .json(&block_info))
}

/// Get block by height v3
///
/// Get information about the block that has been finalized at the given height.
#[utoipa::path(
    get,
    path = "/rpc/v3/block/height/{height}",
    operation_id = "block_by_height_v3",
    params(
        ("height" = u64, Path, description = "Block height"),
        BlockByHeightQueryV3,
        TransactionQueryType,
    ),
    responses(
        (status = 200,
            description = "Information about the block that has been finalized at the given height.",
            headers (
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
            ),
            content(
                (BlockV3 = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/height/{height}")]
pub async fn block_by_height(
    state: State<ApiServerState>,
    height: Path<u64>,
    query_parameters: Query<BlockByHeightQueryV3>,
    txn_type: Query<TransactionQueryType>,
) -> Result<HttpResponse> {
    let height = height.into_inner();
    let txn_type_value = txn_type.into_inner().value;
    debug!("Received a query for the block at height {height}");
    let block_v2 = state.get_block_by_height(
        height,
        query_parameters.with_finalized_transactions,
        txn_type_value.clone(),
    )?;
    match block_v2 {
        None => Err(Error::InformationNotAvailable(format!(
            "No block with {height} is available"
        ))),
        Some(v2) => {
            let metadata_txn_info =
                get_block_metadata_info_by_height(&state, height, &txn_type_value)?;
            let stats = state.get_block_execution_statistics_by_height(height)?;
            let block_v3 = BlockV3::from((v2, metadata_txn_info, stats));
            trace!("Returning {block_v3:?}");
            Ok(HttpResponse::Ok()
                .header(X_SUPRA_CHAIN_ID, state.chain_id_header_value())
                .json(&block_v3))
        }
    }
}

/// Get transactions by block hash v3
///
/// Get a list containing the hashes of the transactions that were finalized in the block with
/// the given hash in the order that they were executed.
#[utoipa::path(
    get,
    path = "/rpc/v3/block/{block_hash}/transactions",
    operation_id = "txs_by_block_v3",
    params(TransactionQueryType,
        ("block_hash" = String, Path, description = "Hex encoded block hash")
    ),
    responses(
        (status = 200,
            description = "List of the hashes of the transactions contained in the block.",
            headers (
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
            ),
            content(
                (Vec<delegates::Hash> = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{block_hash}/transactions")]
pub async fn txs_by_block(
    state: State<ApiServerState>,
    block_hash: Path<Hash>,
    txn_type: Query<TransactionQueryType>,
) -> Result<HttpResponse> {
    let bhash = block_hash.into_inner();
    let txn_query_type = txn_type.into_inner();
    let txn_hashes = get_txns_by_block_hash_and_type(&state, bhash, txn_query_type.value)?;
    Ok(HttpResponse::Ok()
        .header(X_SUPRA_CHAIN_ID, state.chain_id_header_value())
        .json(&txn_hashes))
}
