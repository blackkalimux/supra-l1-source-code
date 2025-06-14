#![allow(deprecated)] // To support API v1.

use crate::rest::ApiServerState;
use aptos_api_types::{MoveValue, ViewFunction, ViewRequest};
use aptos_vm::AptosVM;
use move_core_types::language_storage::TypeTag;
use ntex::web;
use ntex::web::types::{Json, State};
use types::api::delegates::ViewRequest as ViewRequestApi;
use types::api::v1::MoveValueResponseV1;

/// Execute view function v1
///
/// Execute the Move function with the given parameters and return its execution result.
///
/// # Example
/// ```json
/// {
///   "function": "0x1::timestamp::now_microseconds",
///   "type_arguments": [],
///   "arguments": []
/// }
///```
#[utoipa::path(
    post,
    path = "/rpc/v1/view",
    operation_id = "view_function_v1",
    request_body ( content = ViewRequestApi, content_type = "application/json"),
    responses(
        (status = 200, description = "Result of view function", content(( Option<MoveValueResponseV1> = "application/json")))
    ),
)]
#[web::post("")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn view_function(
    state: State<ApiServerState>,
    req: Json<ViewRequest>,
) -> crate::error::Result<Json<Option<MoveValueResponseV1>>> {
    let converter = state.converter();
    let view_function = converter.convert_view_function(req.0)?;
    let return_type = converter
        .function_return_types(&view_function)
        .and_then(|tys| {
            tys.into_iter()
                .map(TypeTag::try_from)
                .collect::<anyhow::Result<Vec<TypeTag>>>()
        })?;

    let ViewFunction {
        module,
        function,
        ty_args,
        args,
    } = view_function;

    let view_fxn_output = AptosVM::execute_view_function(
        &state.move_store,
        module,
        function,
        ty_args,
        args,
        u64::MAX,
    );

    let output = view_fxn_output.values?;

    let val = output
        .clone()
        .into_iter()
        .zip(return_type.into_iter())
        .map(|(bytes, typ)| converter.try_into_move_value(&typ, bytes.as_slice()).ok())
        .collect::<Vec<Option<MoveValue>>>();

    let response = MoveValueResponseV1 { result: val };
    Ok(Json(Some(response)))
}
