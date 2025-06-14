use crate::rest::ApiServerState;
use aptos_api_types::{MoveValue, ViewFunction, ViewRequest};
use aptos_vm::AptosVM;
use move_core_types::language_storage::TypeTag;
use ntex::web;
use ntex::web::types::{Json, State};
use types::api::delegates::ViewRequest as ViewRequestApi;
use types::api::v2::MoveValueResponseV2;

/// Execute view function v2
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
    path = "/rpc/v2/view",
    operation_id = "view_function_v2",
    request_body ( content = ViewRequestApi, content_type = "application/json"),
    responses(
        (status = 200, description = "Result of view function", content((MoveValueResponseV2 = "application/json")))
    )
)]
#[web::post("")]
pub async fn view_function(
    state: State<ApiServerState>,
    req: Json<ViewRequest>,
) -> crate::error::Result<Json<MoveValueResponseV2>> {
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
        u64::MAX, // TODO: add this to api config
    );

    let output = view_fxn_output.values?;
    let mut val: Vec<MoveValue> = Vec::new();

    for (bytes, typ) in output.clone().into_iter().zip(return_type.into_iter()) {
        val.push(converter.try_into_move_value(&typ, bytes.as_slice())?);
    }

    let response = MoveValueResponseV2 { result: val };
    Ok(Json(response))
}
