#![allow(deprecated)] // To support API v1.

use crate::error::{Error, Result};
use crate::rest::controller::accounts::{
    account_information_controller_v1, coin_transactions_controller_v2,
    get_module_byte_code_from_module_id, move_resource_controller_v1, statement_controller,
};
use crate::rest::move_list::v1::{MoveList, MoveModuleList, MoveResourceList};
use crate::rest::ApiServerState;
use aptos_api_types::MoveModule;
use core::str::FromStr;
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{ModuleId, StructTag};
use ntex::web;
use ntex::web::types::{Json, Path, Query, State};
use std::cmp::min;
use types::api::v1::{self, MoveListQuery};
use types::api::{
    delegates::MoveModule as MoveModuleApi,
    v1::{AccountCoinStatementV1, AccountInfo, AccountStatementV1, MoveValueResponseV1},
    MAX_NUM_OF_TRANSACTIONS_TO_RETURN,
};
use types::ensure_hex_prefix;
use types::pagination::{
    AccountCoinTxPagination, AccountPublishedListPagination, AccountTxPagination,
    DEFAULT_SIZE_OF_PAGE,
};

/// Get account v1
///
/// Fetch account info
/// contains auth key & sequence number for the account
#[utoipa::path(
    get,
    path = "/rpc/v1/accounts/{address}",
    operation_id = "get_account_v1",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
    ),
    responses(
        (status = 200,
            description = "Account information",
            content(
                (Option<AccountInfo> = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn get_account_v1(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
) -> Result<Json<Option<AccountInfo>>> {
    let account_info = account_information_controller_v1(state, addr)?.take_v1()?;
    Ok(Json(account_info))
}

/// Get account resources v1
///
/// Get account resources by address
#[utoipa::path(
    get,
    path = "/rpc/v1/accounts/{address}/resources",
    operation_id = "get_account_resources_v1",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountPublishedListPagination,
    ),
    responses(
        (status = 200,
            description = "Account resource list",
            content(
                (MoveList = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/resources")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn get_account_resources_v1(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
    pagination: Query<AccountPublishedListPagination>,
) -> Result<Json<MoveList>> {
    let (list, cursor) = state.get_move_list(
        addr.into_inner(),
        pagination.start.clone(),
        pagination.count.map_or_else(
            || DEFAULT_SIZE_OF_PAGE,
            |c| min(c, MAX_NUM_OF_TRANSACTIONS_TO_RETURN),
        ),
        MoveListQuery::Resources,
    )?;
    let resource = list
        .take_resourcelist()?
        .resources
        .iter()
        .map(|r| (ensure_hex_prefix!(r.to_canonical_string()), r.clone()))
        .collect::<Vec<(String, StructTag)>>();
    let account_resources = MoveList::Resources(MoveResourceList { resource, cursor });
    Ok(Json(account_resources))
}

/// Get account resource v1
///
/// View account resource by Move type
#[utoipa::path(
    get,
    path = "/rpc/v1/accounts/{address}/resources/{resource_type}",
    operation_id = "get_account_resource_v1",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        ("resource_type" = String, Path, description = "Name of struct to retrieve e.g. `0x1::account::Account`", example = "0x1::coin::CoinStore<0x1::supra_coin::SupraCoin>", pattern = "^0x[0-9a-zA-Z:_<>]+$")
    ),
    responses(
     (status = 200,
            description = "View account resource by Move Struct type",
            content(
                (Option<MoveValueResponseV1> = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/resources/{resource_type}")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn get_account_resource_v1(
    state: State<ApiServerState>,
    path_param: Path<(AccountAddress, String)>,
) -> Result<Json<Option<MoveValueResponseV1>>> {
    let val = move_resource_controller_v1(state, path_param)?.take_v1()?;
    let response = MoveValueResponseV1 { result: vec![val] };
    Ok(Json(Some(response)))
}

/// Get account modules v1
///
/// Get account modules by address
#[utoipa::path(
    get,
    path = "/rpc/v1/accounts/{address}/modules",
    operation_id = "get_account_modules_v1",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountPublishedListPagination,
    ),
    responses(
     (status = 200,
            description = "Account module list",
            content(
                (MoveList = "application/json"),
            ),
        ),
    ),
)]
#[web::get("/{address}/modules")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn get_account_modules_v1(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
    pagination: Query<AccountPublishedListPagination>,
) -> Result<Json<MoveList>> {
    let (list, cursor) = state.get_move_list(
        addr.into_inner(),
        pagination.start.clone(),
        pagination.count.map_or_else(
            || DEFAULT_SIZE_OF_PAGE,
            |c| min(c, MAX_NUM_OF_TRANSACTIONS_TO_RETURN),
        ),
        MoveListQuery::Modules,
    )?;
    let modules = list
        .take_modulelist()?
        .modules
        .iter()
        .map(|r| (ensure_hex_prefix!(r.to_string()), r.clone()))
        .collect::<Vec<(String, ModuleId)>>();
    let get_account_modules_v1 = MoveList::Modules(MoveModuleList { modules, cursor });
    Ok(Json(get_account_modules_v1))
}

/// Get account module v1
///
/// Retrieves an individual module from a given account.
#[utoipa::path(
    get,
    path = "/rpc/v1/accounts/{address}/modules/{module_name}",
    operation_id = "get_account_module_v1",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        ("module_name" = String, Path, description = "Name of module to retrieve e.g. coin")
    ),
    responses(
        (status = 200,
            description = "View account module by Move module name",
            content(
                (Option<MoveModuleApi> = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/modules/{module_name}")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn get_account_module_v1(
    state: State<ApiServerState>,
    path_param: Path<(AccountAddress, String)>,
) -> Result<Json<Option<MoveModule>>> {
    let (account_address, module_name) = path_param.into_inner();
    let module_id = ModuleId::new(
        account_address,
        // Return Invalid Input from anyhow error
        Identifier::from_str(&module_name).map_err(|e| Error::InvalidInput(e.to_string()))?,
    );

    let maybe_compiled_module = get_module_byte_code_from_module_id(&state, &module_id)?;

    let maybe_move_module = maybe_compiled_module.map(|m| m.abi().clone());

    Ok(Json(maybe_move_module))
}

/// Get account transactions v1
///
/// List of finalized transactions sent by the move account. Return max 100 transactions per request.
///
// --- Below is not public API doc, but internal design doc ---
// It use sequence number as the second index to store the transactions for an account.
//
#[utoipa::path(
    get,
    path = "/rpc/v1/accounts/{address}/transactions",
    operation_id = "get_account_transactions_v1",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountTxPagination,
    ),
    responses(
        (status = 200,
            description = "List of transactions from the move account.",
            content(
                (AccountStatementV1 = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/transactions")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn get_account_transactions_v1(
    state: State<ApiServerState>,
    address: Path<AccountAddress>,
    pagination: Query<AccountTxPagination>,
) -> Result<Json<AccountStatementV1>> {
    let transactions = statement_controller(&state, address, pagination)?;

    Ok(Json(AccountStatementV1 {
        record: transactions
            .into_iter()
            .map(v1::TransactionInfoV1::from)
            .collect(),
    }))
}

/// Get coin transactions v1
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
    path = "/rpc/v1/accounts/{address}/coin_transactions",
    operation_id = "coin_transactions_v1",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountCoinTxPagination,
    ),
    responses(
        (status = 200,
            description = "List of *finalized* transactions sent from and received by the move account.",
            content(
                (AccountCoinStatementV1 = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/coin_transactions")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn coin_transactions_v1(
    state: State<ApiServerState>,
    address: Path<AccountAddress>,
    pagination: Query<AccountCoinTxPagination>,
) -> Result<Json<AccountCoinStatementV1>> {
    let (transactions, cursor) = coin_transactions_controller_v2(state, address, pagination)?;
    Ok(Json(AccountCoinStatementV1 {
        record: transactions
            .into_iter()
            .map(v1::TransactionInfoV1::from)
            .collect(),
        cursor,
    }))
}
