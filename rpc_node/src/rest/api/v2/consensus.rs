use crate::error::Result;
use crate::rest::api::v1::consensus::ConsensusBlockByHeightQuery;
use crate::rest::{ApiServerState, PublicApiServerError};
use committees::Height;
use lifecycle::Epoch;
use ntex::http::header;
use ntex::web;
use ntex::web::types::{Path, Query, State};
use ntex::web::HttpResponse;
use soserde::SmrSerialize;
use tracing::debug;
use types::api::v2::BinaryData;

/// Get consensus block by height v2
///
/// Get the BCS bytes of the consensus block at the requested height.
///
/// Returns an HTTP response containing the BCS bytes of the requested ConsensusBlock, optionally
/// including the BCS bytes of the associated transaction batches.
/// - If the block is found, responds with [StatusCode::OK] and a binary body containing the serialized block data.
/// - If the block is not found, responds with [StatusCode::NOT_FOUND] and an empty body.
#[utoipa::path(
    get,
    path = "/rpc/v2/consensus/block/height/{height}",
    operation_id = "consensus_block_v2",
    params(
        ("height" = u64, Path, description = "Block height"),
        ConsensusBlockByHeightQuery,
    ),
    responses(
        (status = 200,
            description = "Binary representation of the consensus block at the requested height.",
            content(
                (BinaryData = "application/json")
            ),
        ),
        (status = 404,
            description = "No block for for the requested height.",
            content(
                (BinaryData = "application/json"),
            ),
        )
    ),
)]
#[web::get("/block/height/{height}")]
pub async fn consensus_block(
    state: State<ApiServerState>,
    height: Path<Height>,
    with_batches: Query<ConsensusBlockByHeightQuery>,
) -> Result<HttpResponse> {
    let height = height.into_inner();

    debug!("Received a query for the certified block at height {height}");

    let block = state
        .get_certified_block(height, with_batches.with_batches)
        .map_err(PublicApiServerError)?;

    Ok(serializable_to_http_response(block))
}

/// Get latest consensus block v2
///
/// Get the BCS bytes of the latest consensus block.
///
/// Returns an HTTP response containing the BCS bytes of the requested ConsensusBlock, optionally
/// including the BCS bytes of the associated transaction batches.
/// - If the block is found, responds with [StatusCode::OK] and a binary body containing the serialized block data.
/// - If the block is not found, responds with [StatusCode::NOT_FOUND] and an empty body.
#[utoipa::path(
    get,
    path = "/rpc/v2/consensus/block",
    operation_id = "latest_consensus_block_v2",
    responses(
        (status = 200,
            description = "Binary representation of the latest consensus block.",
            content(
                (BinaryData = "application/json")
            )
        ),
        (status = 404,
            description = "No block found.",
            content(
                (BinaryData = "application/json")
            )
        )
    )
)]
#[web::get("/block")]
pub async fn latest_consensus_block(
    state: State<ApiServerState>,
    with_batches: Query<ConsensusBlockByHeightQuery>,
) -> Result<HttpResponse> {
    debug!("Received a query for the latest certified block");

    let block = state
        .get_latest_certified_block(with_batches.with_batches)
        .map_err(PublicApiServerError)?;

    Ok(serializable_to_http_response(block))
}

/// Get committee authorization v2
///
/// Get the BCS bytes of the Committee Authorization for the requested epoch.
///
/// Returns an HTTP response containing the BCS bytes of the requested CommitteeAuthorization.
/// - If the authorization is found, responds with [StatusCode::OK] and a binary body containing the serialized authorization data.
/// - If the authorization is not found, responds with [StatusCode::NOT_FOUND] and an empty body.
#[utoipa::path(
    get,
    path = "/rpc/v2/consensus/committee_authorization/{epoch}",
    operation_id = "committee_authorization_v2",
    params(
        ("epoch" = Epoch, Path, description = "Epoch")
    ),
    responses(
        (status = 200,
            description = "Binary representation of the committee authorization for the requested epoch.",
            content(
                (BinaryData = "application/json"),
            ),
        ),
        (status = 404,
            description = "No committee authorization found for the requested epoch.",
            content(
                (BinaryData = "application/json"),
            ),
        ),
    )
)]
#[web::get("/committee_authorization/{epoch}")]
pub async fn committee_authorization(
    state: State<ApiServerState>,
    epoch: Path<Epoch>,
) -> Result<HttpResponse> {
    let epoch = epoch.into_inner();

    debug!("Received a query for authorization for epoch {epoch}");

    let committee_authorization = state
        .get_authorization_for_epoch(epoch)
        .map_err(PublicApiServerError)?;

    Ok(serializable_to_http_response(committee_authorization))
}

/// Converts an [SmrSerialize]-enabled primitive (such as ConsensusBlock or [CommitteeAuthorization])
/// into an HTTP response with a binary body containing the serialized data.
pub fn serializable_to_http_response(serializable: Option<impl SmrSerialize>) -> HttpResponse {
    if let Some(data) = serializable.map(|s| s.to_bytes()) {
        HttpResponse::Ok()
            .set_header(header::CONTENT_TYPE, "application/octet-stream")
            .body(data)
    } else {
        HttpResponse::NotFound().finish()
    }
}
