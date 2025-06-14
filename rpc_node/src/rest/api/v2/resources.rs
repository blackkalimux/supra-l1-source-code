use crate::error::Error;
use crate::rest::{ApiServerState, Converter, PublicApiServerError};
use aptos_api_types::MoveValue;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::TypeTag;
use ntex::web;
use ntex::web::types::{Json, Path, State};
use std::str::FromStr;
use tracing::debug;
use types::api::delegates::MoveValue as MoveValueApi;
use types::api::v1::TableItemRequest;

/// Get table item v2
///
/// # Example of body
/// ```json
/// {
///   "key_type": "u64",
///   "value_type": "0x1::multisig_voting::Proposal<0x1::governance_proposal::GovernanceProposal>",
///   "key": "12"
/// }
///```
/// ```json
/// {
///   "key_type": "u64",
///   "value_type": "0x1::string::String",
///   "key": "42"
/// }
///```
#[utoipa::path(
    post,
    path = "/rpc/v2/tables/{table_handle}/item",
    operation_id = "table_item_by_key",
    params(
        ("table_handle" = String, Path, description = "Table handle to lookup. Should be retrieved using account resources API."),
    ),
    request_body( content = TableItemRequest,
                  description = "Request containing item key and value types and item key itself",
                  content_type = "application/json"
    ),
    responses(
        (status = 200, description = "Item of the table", content((MoveValueApi = "application/json")))
    ),
)]
#[web::post("/{table_handle}/item")]
pub async fn table_item_by_key(
    state: State<ApiServerState>,
    table_handle: Path<AccountAddress>,
    table_item_request: Json<TableItemRequest>,
) -> crate::error::Result<Json<MoveValue>> {
    let table_handle = table_handle.into_inner();
    debug!("Received table item query for {table_handle:?} table: {table_item_request:?}");
    let TableItemRequest {
        key_type,
        value_type,
        key,
    } = &table_item_request.0;
    // Retrieve local state
    let converter = Converter::new(&state.move_store);
    let key_tag =
        TypeTag::from_str(key_type.as_str()).map_err(|e| Error::InvalidInput(e.to_string()))?;

    // Convert key to lookup version for DB
    let vm_key = converter
        .try_into_vm_value(&key_tag, key.clone().into())
        .map_err(|err| Error::MoveConverterError(format!("Invalid key type: {err}")))?;
    let value_tag = TypeTag::from_str(value_type.as_str())?;
    let bytes = state
        .move_store
        .read_table_item(table_handle, vm_key)?
        .ok_or(Error::InformationNotAvailable(
            table_handle.to_canonical_string(),
        ))?;
    converter
        .convert_to_move_value(value_tag, &bytes)
        .map_err(|e| PublicApiServerError(e).into())
        .map(Json)
}
