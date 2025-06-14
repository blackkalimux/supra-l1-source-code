#![allow(deprecated)] // To support API v1.

use crate::error::Result;
use crate::rest::controller::block::get_user_txns_by_block_hash;
use crate::rest::ApiServerState;
use ntex::web;
use ntex::web::types::{Json, Path, Query, State};
use serde::Deserialize;
use socrypto::Hash;
use tracing::{debug, info};
use types::api::v2::{Block, BlockHeaderInfo};
use types::api::{delegates, TransactionType};
use utoipa::IntoParams;

/// Get latest block v1
///
/// Get the header of the most recently finalized block.
#[utoipa::path(
    get,
    path = "/rpc/v1/block",
    operation_id = "latest_block_v1",
    responses(
        (status = 200,
            description = "Returns the header of the most recently finalized block.",
            content(
                (Option<BlockHeaderInfo> = "application/json" ),
            )
        ),
    ),
)]
#[web::get("")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn latest_block(state: State<ApiServerState>) -> Result<Json<Option<BlockHeaderInfo>>> {
    let latest_block = state
        .storage
        .archive()
        .reader()
        .get_latest_block_header_info()?;
    Ok(Json(latest_block))
}

/// Get block by hash v1
///
/// Get the header of the block with the given hash.
#[utoipa::path(
    get,
    path = "/rpc/v1/block/{block_hash}",
    operation_id = "block_header_info_v1",
    params(
        ("block_hash" = String, Path, description = "Hash of block to retrieve")
    ),
    responses(
        (status = 200,
            description = "The header of the block with the given hash.",
            content(
                (Option<BlockHeaderInfo> = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{block_hash}")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn block_header_info(
    state: State<ApiServerState>,
    block_hash: Path<Hash>,
) -> Result<Json<Option<BlockHeaderInfo>>> {
    info!("query for block = {}", block_hash.to_string());
    let block_header_info = state
        .storage
        .archive()
        .reader()
        .get_block_header_info(block_hash.into_inner())?;
    Ok(Json(block_header_info))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct BlockByHeightQuery {
    /// If true, returns all transactions that were finalized by this block in the order that they
    /// were executed.
    pub(crate) with_finalized_transactions: bool,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct BlockByHeightQueryV3 {
    /// If true, returns all transactions that were finalized by this block in the order that they
    /// were executed.
    // Default is false
    #[serde(default)]
    pub(crate) with_finalized_transactions: bool,
}

/// Get block by height v1
///
/// Get information about the block that has been finalized at the given height.
#[utoipa::path(
    get,
    path = "/rpc/v1/block/height/{height}",
    operation_id = "block_by_height_v1",
    params(
        ("height" = u64, Path, description = "Block height"),
        BlockByHeightQuery,
    ),
    responses(
        (status = 200,
            description = "Information about the block that has been finalized at the given height.",
            content(
                (Option<Block> = "application/json"),
            )
        )
    ),
)]
#[web::get("/height/{height}")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn block_by_height(
    state: State<ApiServerState>,
    height: Path<u64>,
    query_parameters: Query<BlockByHeightQuery>,
) -> Result<Json<Option<Block>>> {
    debug!("Received a query for the block at height {height}");
    let block = state.get_block_by_height(
        height.into_inner(),
        query_parameters.with_finalized_transactions,
        Some(TransactionType::User),
    )?;
    debug!("Returning {block:?}");
    Ok(Json(block))
}

/// Get transactions by block hash v1
///
/// Get a list containing the hashes of the transactions that were finalized in the block with
/// the given hash in the order that they were executed.
#[utoipa::path(
    get,
    path = "/rpc/v1/block/{block_hash}/transactions",
    operation_id = "txn_by_block_v1",
    params(
        ("block_hash" = String, Path, description = "Hex encoded block hash")
    ),
    responses(
        (status = 200,
            description = "List of the hashes of the transactions contained in the block.",
            content(
                (Vec<delegates::Hash> = "application/json")
            )
        )
    ),
)]
#[web::get("/{block_hash}/transactions")]
pub async fn txs_by_block(
    state: State<ApiServerState>,
    block_hash: Path<Hash>,
) -> Result<Json<Vec<Hash>>> {
    let bhash = block_hash.into_inner();
    let transactions = get_user_txns_by_block_hash(&state, bhash)?;
    Ok(Json(transactions))
}
