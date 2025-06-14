use crate::error::{Error, Result};
use crate::rest::ApiServerState;
use ntex::web::{
    self,
    types::{Json, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use types::{api::v1::Events, TypeTag};
use utoipa::IntoParams;

const MAX_NUM_OF_BLOCK_EVENTS_TO_RETURN: usize = 10;
#[derive(Debug, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventQuery {
    /// Starting block height (inclusive).
    #[param(style = Form)]
    pub start: u64,
    /// Ending block height (exclusive). The max range is 10 blocks, a.k.a. end - start <= 10.
    #[param(style = Form)]
    pub end: u64,
}

/// Get events by type v1
///
/// Get events by type.
#[utoipa::path(
    get,
    path = "/rpc/v1/events/{event_type}",
    operation_id = "events_by_type",
    params(
        EventQuery,
        ("event_type" = String, Path, description = "Canonical string representation of event type. E.g. 0000000000000000000000000000000a::module_name::type_name")
    ),
    responses(
        (status = 200, description = "List of Events contained in blocks", content(( Events = "application/json")))
    ),
)]
#[web::get("/{event_type}")]
pub async fn events_by_type(
    state: State<ApiServerState>,
    event_type: Path<String>,
    query: Query<EventQuery>,
) -> Result<Json<Events>> {
    if query.end <= query.start {
        return Err(Error::InvalidInput(
            "`end` must be greater than `start`".to_string(),
        ));
    }
    if query.end - query.start > MAX_NUM_OF_BLOCK_EVENTS_TO_RETURN as u64 {
        return Err(Error::InvalidInput(
           format! ("The range of blocks to query must be less than or equal to {MAX_NUM_OF_BLOCK_EVENTS_TO_RETURN}"),
        ));
    }
    // Validate input before querying the archive.
    if event_type.is_empty() {
        return Err(Error::InvalidInput("Empty event type".to_string()));
    }
    let type_tag =
        TypeTag::from_str(&event_type).map_err(|e| Error::InvalidInput(e.to_string()))?;

    let events = state.get_events_by_type(type_tag, query.start, query.end)?;
    Ok(Json(events))
}
