use crate::error::{Error, Result};
use crate::rest::controller::accounts::{
    account_information_controller_v2, coin_transactions_controller_v2,
    get_module_byte_code_from_module_id, statement_controller,
};
use crate::rest::ApiServerState;
use aptos_api_types::MoveResource;
use aptos_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{ModuleId, StructTag};
use ntex::web;
use ntex::web::types::{Json, Path, Query, State};
use std::cmp::min;
use std::str::FromStr;
use types::api::delegates::{
    MoveResource as ApiMoveResource, MoveResourcesResponse as ApiMoveResourceResponse,
};
use types::api::v1::MoveListQuery;
use types::api::v2::MoveModuleListV2;
use types::api::v2::MoveResourcesResponse;
use types::api::{
    v1::AccountInfo,
    v2::{AccountCoinStatementV2, AccountStatementV2, MoveModuleBytecode},
    MAX_NUM_OF_TRANSACTIONS_TO_RETURN,
};
use types::pagination::{
    AccountCoinTxPagination, AccountPublishedListPagination, AccountTxPagination,
    DEFAULT_SIZE_OF_PAGE,
};

/// Get account v2
///
/// Fetch account info
/// contains auth key & sequence number for the account
#[utoipa::path(
    get,
    path = "/rpc/v2/accounts/{address}",
    operation_id = "get_account_v2",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
    ),
    responses(
        (status = 200,
            description = "Account information",
            content(
                (AccountInfo = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}")]
pub async fn get_account_v2(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
) -> Result<Json<AccountInfo>> {
    let account_info = account_information_controller_v2(state, addr)?.take_v2()?;
    Ok(Json(account_info))
}
/// Get account resources v2
///
/// Get account resources by address
#[utoipa::path(
    get,
    path = "/rpc/v2/accounts/{address}/resources",
    operation_id = "get_account_resources_v2",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountPublishedListPagination,
    ),
    responses(
        (status = 200,
            description = "Account resource list",
            content(
                (ApiMoveResourceResponse = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/resources")]
pub async fn get_account_resources_v2(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
    pagination: Query<AccountPublishedListPagination>,
) -> Result<Json<MoveResourcesResponse>> {
    let account_address = addr.into_inner();
    let (list, cursor) = state.get_move_list(
        account_address,
        pagination.start.clone(),
        pagination.count.map_or_else(
            || DEFAULT_SIZE_OF_PAGE,
            |c| min(c, MAX_NUM_OF_TRANSACTIONS_TO_RETURN),
        ),
        MoveListQuery::Resources,
    )?;
    let move_resource_ids = list.take_resourcelist()?;
    let mut resources = Vec::new();

    for tag in move_resource_ids.resources {
        let maybe_resource = state.get_move_resource(account_address, &tag)?;
        let resource = Error::unwrap_or_404(maybe_resource, account_address.to_canonical_string())?;
        resources.push(resource);
    }

    let response = MoveResourcesResponse::new(resources, cursor);
    Ok(Json(response))
}

/// Get account resource v2
///
/// View account resource by Move type
#[utoipa::path(
    get,
    path = "/rpc/v2/accounts/{address}/resources/{resource_type}",
    operation_id = "get_account_resource_v2",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        ("resource_type" = String, Path, description = "Name of struct to retrieve e.g. `0x1::account::Account`", example = "0x1::coin::CoinStore<0x1::supra_coin::SupraCoin>", pattern = "^0x[0-9a-zA-Z:_<>]+$")
    ),
    responses(
     (status = 200,
            description = "View account resource by Move Struct type",
            content(
                (ApiMoveResource = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/resources/{resource_type}")]
pub async fn get_account_resource_v2(
    state: State<ApiServerState>,
    path_param: Path<(AccountAddress, String)>,
) -> Result<Json<MoveResource>> {
    let (account_address, tag_string) = path_param.into_inner();
    // Return Invalid Input from anyhow error
    let tag =
        StructTag::from_str(tag_string.as_str()).map_err(|e| Error::InvalidInput(e.to_string()))?;
    let maybe_resource = state.get_move_resource(account_address, &tag)?;
    let resource = Error::unwrap_or_404(maybe_resource, account_address.to_canonical_string())?;
    Ok(Json(resource))
}

/// Get account module v2
///
/// Retrieves an individual module from a given account.
#[utoipa::path(
    get,
    path = "/rpc/v2/accounts/{address}/modules/{module_name}",
    operation_id = "get_account_module_v2",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        ("module_name" = String, Path, description = "Name of module to retrieve e.g. coin")
    ),
    responses(
        (status = 200,
            description = "View account module by Move module name",
            content(
                (MoveModuleBytecode = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/modules/{module_name}")]
pub async fn get_account_module_v2(
    state: State<ApiServerState>,
    path_param: Path<(AccountAddress, String)>,
) -> Result<Json<MoveModuleBytecode>> {
    let (account_address, module_name) = path_param.into_inner();
    let module_id = ModuleId::new(
        account_address,
        // Return Invalid Input from anyhow error
        Identifier::from_str(&module_name).map_err(|e| Error::InvalidInput(e.to_string()))?,
    );

    let maybe_compiled_module = get_module_byte_code_from_module_id(&state, &module_id)?;

    let move_module_byte_code =
        maybe_compiled_module.ok_or(Error::InformationNotAvailable(module_id.to_string()))?;

    Ok(Json(move_module_byte_code))
}

/// Get account modules v2
///
/// Retrieves all account modules' bytecode for a given account.
#[utoipa::path(
    get,
    path = "/rpc/v2/accounts/{address}/modules",
    operation_id = "get_account_modules_v2",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountPublishedListPagination,
    ),
    responses(
     (status = 200,
            description = "account module list",
            content(
                (MoveModuleListV2 = "application/json"),
            ),
        ),
    ),
)]
#[web::get("/{address}/modules")]
pub async fn get_account_modules_v2(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
    pagination: Query<AccountPublishedListPagination>,
) -> Result<Json<MoveModuleListV2>> {
    let (move_list, cursor) = state.get_move_list(
        addr.into_inner(),
        pagination.start.clone(),
        pagination.count.map_or_else(
            || DEFAULT_SIZE_OF_PAGE,
            |c| min(c, MAX_NUM_OF_TRANSACTIONS_TO_RETURN),
        ),
        MoveListQuery::Modules,
    )?;
    let module_list = move_list.take_modulelist()?;
    let success = MoveModuleListV2::new(
        module_list
            .modules
            .iter()
            .filter_map(|module_id| {
                get_module_byte_code_from_module_id(&state, module_id)
                    .ok()
                    .flatten()
            })
            .collect::<Vec<MoveModuleBytecode>>(),
        cursor,
    );
    Ok(Json(success))
}

/// Get account transactions v2
///
/// List of finalized transactions sent by the move account. Return max 100 transactions per request.
///
// --- Below is not public API doc, but internal design doc ---
// It use sequence number as the second index to store the transactions for an account.
//
#[utoipa::path(
    get,
    path = "/rpc/v2/accounts/{address}/transactions",
    operation_id = "get_account_transactions_v2",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountTxPagination,
    ),
    responses(
        (status = 200,
            description = "List of transactions from the move account.",
            content(
                (AccountStatementV2 = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/transactions")]
pub async fn get_account_transactions_v2(
    state: State<ApiServerState>,
    address: Path<AccountAddress>,
    pagination: Query<AccountTxPagination>,
) -> Result<Json<AccountStatementV2>> {
    let transactions = statement_controller(&state, address, pagination)?;
    Ok(Json(AccountStatementV2 {
        record: transactions,
    }))
}

/// Get coin transactions v2
///
/// List of finalized coin withdraw/deposit transactions relevant to the move account. Return max 100 transactions per request.
/// It includes both transactions sent from and received by the account.
///
// --- Below is not public API doc, but internal design doc ---
//
// Due to above requirements, index the transactions by sequence number can not provide the total order of the transactions,
// as the sequence number of the received transactions that higher than the sent transactions can be earlier in time.
//
// So it use `timestamp` as 2nd level index to sort the transactions.
//
// # Use cases:
//
// At present, Wallet team rely on this to track the latest ~20 transactions of an account.
// This API can be deprecated once we have indexed database so that Wallet team is able to query relevant
// transactions deposit to the account.
//
#[utoipa::path(
    get,
    path = "/rpc/v2/accounts/{address}/coin_transactions",
    operation_id = "coin_transactions_v2",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountCoinTxPagination,
    ),
    responses(
        (status = 200,
            description = "List of *finalized* transactions sent from and received by the move account.",
            content(
                (AccountCoinStatementV2 = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/coin_transactions")]
pub async fn coin_transactions_v2(
    state: State<ApiServerState>,
    address: Path<AccountAddress>,
    pagination: Query<AccountCoinTxPagination>,
) -> Result<Json<AccountCoinStatementV2>> {
    let (transactions, cursor) = coin_transactions_controller_v2(state, address, pagination)?;
    Ok(Json(AccountCoinStatementV2 {
        record: transactions,
        cursor,
    }))
}
