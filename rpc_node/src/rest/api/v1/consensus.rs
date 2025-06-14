#![allow(deprecated)] // To support API v1.

use crate::error::Result;
use crate::rest::{ApiServerState, PublicApiServerError};
use committees::Height;
use ntex::web;
use ntex::web::types::{Json, Path, Query, State};
use serde::Deserialize;
use tracing::debug;
use types::api::v1::ConsensusBlock;
use utoipa::IntoParams;

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ConsensusBlockByHeightQuery {
    /// If true, returns all batches of transactions with certificates contained in this block.
    pub(crate) with_batches: bool,
}

/// Get consensus block by height v1
///
/// Get the consensus block at the requested height.
#[utoipa::path(
    get,
    path = "/rpc/v1/consensus/block/height/{height}",
    operation_id = "consensus_block_v1",
    params(
        ("height" = u64, Path, description = "Block height"),
        ConsensusBlockByHeightQuery,
    ),
    responses(
        (status = 200,
            description = "The consensus block for the given height.",
            content(
                (Option<ConsensusBlock> = "application/json")
            )
        )
    ),
)]
#[web::get("/block/height/{height}")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn consensus_block(
    state: State<ApiServerState>,
    height: Path<Height>,
    with_batches: Query<ConsensusBlockByHeightQuery>,
) -> Result<Json<Option<ConsensusBlock>>> {
    let height = height.into_inner();

    debug!("Received a query for the certified block at height {height}");

    let consensus_block = state
        .get_certified_block(height, with_batches.with_batches)
        .map_err(PublicApiServerError)?;

    Ok(Json(consensus_block))
}
