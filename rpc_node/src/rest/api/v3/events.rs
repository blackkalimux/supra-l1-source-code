use crate::error::{Error, Result};
use crate::rest::ApiServerState;
use ntex::web::HttpResponse;
use ntex::web::{
    self,
    types::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use types::api::v3::EventsV3;
use types::api::{X_SUPRA_CHAIN_ID, X_SUPRA_CURSOR};
use types::TypeTag;
use utoipa::IntoParams;

// Maximum number of events to return in a single query
const MAX_EVENTS_TO_RETURN: usize = 100;
// Default limit if not specified
const DEFAULT_EVENTS_LIMIT: usize = 20;

#[derive(Debug, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventQuery {
    /// Starting block height (inclusive). Optional.
    #[param(style = Form)]
    pub start_height: Option<u64>,

    /// Ending block height (exclusive). Optional.
    #[param(style = Form)]
    pub end_height: Option<u64>,

    /// Maximum number of events to return. Defaults to 20, max 100.
    #[param(style = Form)]
    pub limit: Option<usize>,

    /// The cursor to start the query from. Optional.
    ///
    /// During a paginated query, the cursor returned in the `X_SUPRA_CURSOR` response header
    /// should be specified as the `start` parameter of the request for the next page.
    #[param(style = Form)]
    pub start: Option<String>,
}

/// Get events by type v3
///
/// Get events by type.
#[utoipa::path(
    get,
    path = "/rpc/v3/events/{event_type}",
    operation_id = "events_by_type_v3",
    params(
        EventQuery,
        ("event_type" = String, Path, description = "The fully qualified name of the event struct, i.e.: `contract_address::module_name::event_struct_name`. E.g. `0x1::coin::CoinDeposit`"),
    ),
    responses(
        (status = 200,
            description = "List of Events contained in blocks",
            headers(
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
                ("x-supra-cursor" = String, description = "Cursor specifying where to start for pagination. This cursor cannot be derived manually client-side. Instead, you must call this endpoint once without this query parameter specified, and then use the cursor returned in the x-supra-cursor header in the response."),
            ),
            content(( EventsV3 = "application/json"))
        ),
    ),
)]
#[web::get("/{event_type}")]
pub async fn events_by_type(
    state: State<ApiServerState>,
    event_type: Path<String>,
    query: Query<EventQuery>,
) -> Result<HttpResponse> {
    // Validate input before querying the archive
    if event_type.is_empty() {
        return Err(Error::InvalidInput("Empty event type".to_string()));
    }

    let type_tag =
        TypeTag::from_str(&event_type).map_err(|e| Error::InvalidInput(e.to_string()))?;

    // Determine the limit (with defaults and caps)
    let limit = query
        .limit
        .unwrap_or(DEFAULT_EVENTS_LIMIT)
        .min(MAX_EVENTS_TO_RETURN);

    // If start and end are provided, make sure they form a valid range
    if let (Some(start), Some(end)) = (query.start_height, query.end_height) {
        if end <= start {
            return Err(Error::InvalidInput(
                "`end` must be greater than `start`".to_string(),
            ));
        }
    }

    // Use provided range or defaults
    let start_height = query.start_height.unwrap_or(0);
    let end_height = query.end_height.unwrap_or(u64::MAX);
    let cursor = query
        .start
        .as_ref()
        .map(hex::decode)
        .transpose()
        .map_err(|_| Error::CursorDecodeError)?;

    // Get events by type with the specified limit
    let (events, cursor) =
        state.get_events_by_type_with_limit(type_tag, start_height, end_height, limit, cursor)?;

    let cursor = cursor.map(hex::encode).unwrap_or("".to_string());
    Ok(HttpResponse::Ok()
        .header(X_SUPRA_CHAIN_ID, state.chain_id_header_value())
        .header(X_SUPRA_CURSOR, cursor)
        .json(&events))
}
