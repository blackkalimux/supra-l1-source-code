use crate::error::{Error, Result};
use crate::rest::api::v1::block::BlockByHeightQuery;
use crate::rest::ApiServerState;
use ntex::web;
use ntex::web::types::{Json, Path, Query, State};
use socrypto::Hash;
use tracing::{debug, info};
use types::api::v2::{Block, BlockHeaderInfo};
use types::api::TransactionType;

/// Get latest block v2
///
/// Get the header of the most recently finalized block.
#[utoipa::path(
    get,
    path = "/rpc/v2/block",
    operation_id = "latest_block_v2",
    responses(
        (status = 200,
            description = "Returns the header of the most recently finalized block.",
            content(
                (BlockHeaderInfo = "application/json" ),
            )
        ),
    ),
)]
#[web::get("")]
pub async fn latest_block(state: State<ApiServerState>) -> Result<Json<BlockHeaderInfo>> {
    let latest_block = state
        .storage
        .archive()
        .reader()
        .get_latest_block_header_info()?
        .ok_or(Error::InformationNotAvailable("Latest Block".to_string()))?;
    Ok(Json(latest_block))
}

/// Get block by hash v2
///
/// Get the header of the block with the given hash.
#[utoipa::path(
    get,
    path = "/rpc/v2/block/{block_hash}",
    operation_id = "block_header_info_v2",
    params(
        ("block_hash" = String, Path, description = "Hex encoded block hash")
    ),
    responses(
        (status = 200,
            description = "The header of the block with the given hash.",
            content(
                (BlockHeaderInfo = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{block_hash}")]
pub async fn block_header_info(
    state: State<ApiServerState>,
    block_hash: Path<Hash>,
) -> Result<Json<BlockHeaderInfo>> {
    let block_hash = block_hash.into_inner();
    info!("query for block = {}", block_hash.to_string());
    let block_header_info = state
        .storage
        .archive()
        .reader()
        .get_block_header_info(block_hash)?
        .ok_or(Error::InformationNotAvailable(block_hash.to_string()))?;
    Ok(Json(block_header_info))
}

/// Get block by height v2
///
/// Get information about the block that has been finalized at the given height.
#[utoipa::path(
    get,
    path = "/rpc/v2/block/height/{height}",
    operation_id = "block_by_height_v2",
    params(
        ("height" = u64, Path, description = "Block height"),
        BlockByHeightQuery,
    ),
    responses(
        (status = 200,
            description = "Information about the block that has been finalized at the given height.",
            content(
                (Block = "application/json"),
            )
        )
    ),
)]
#[web::get("/height/{height}")]
pub async fn block_by_height(
    state: State<ApiServerState>,
    height: Path<u64>,
    query_parameters: Query<BlockByHeightQuery>,
) -> Result<Json<Block>> {
    let height = height.into_inner();
    debug!("Received a query for the block at height {height}");
    let block = state
        .get_block_by_height(
            height,
            query_parameters.with_finalized_transactions,
            Some(TransactionType::User),
        )?
        .ok_or(Error::InformationNotAvailable(height.to_string()))?;
    debug!("Returning {block:?}");
    Ok(Json(block))
}
