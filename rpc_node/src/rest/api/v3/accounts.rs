use crate::error::{Error, Result};
use crate::rest::controller::accounts::{
    coin_transactions_controller_v3, get_module_byte_code_from_module_id, statement_controller,
};
use crate::rest::ApiServerState;
use move_core_types::account_address::AccountAddress;
use ntex::web;
use ntex::web::types::{Path, Query, State};
use ntex::web::HttpResponse;
use std::cmp::min;
use tracing::debug;
use types::api::delegates::MoveResource;
use types::api::v1::MoveListQuery;
use types::api::v2::MoveModuleBytecode;
use types::api::v3::{AccountStatementV3, Transaction};
use types::api::{MAX_NUM_OF_TRANSACTIONS_TO_RETURN, X_SUPRA_CHAIN_ID, X_SUPRA_CURSOR};
use types::pagination::{
    AccountAutomatedTxPagination, AccountCoinTxPagination, AccountPublishedListPagination,
    AccountTxPagination, DEFAULT_SIZE_OF_PAGE,
};

/// Get account transactions v3
///
/// List of finalized transactions sent by the move account. Return max 100 transactions per request.
///
// --- Below is not public API doc, but internal design doc ---
// It use sequence number as the second index to store the transactions for an account.
//
#[utoipa::path(
    get,
    path = "/rpc/v3/accounts/{address}/transactions",
    operation_id = "get_account_transactions_v3",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountTxPagination,
    ),
    responses(
        (status = 200,
            description = "List of transactions from the move account.",
            headers(
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
            ),
            content(
                (AccountStatementV3 = "application/json" ),
            ),
        ),
    ),
)]
#[web::get("/{address}/transactions")]
pub async fn get_account_transactions_v3(
    state: State<ApiServerState>,
    address: Path<AccountAddress>,
    pagination: Query<AccountTxPagination>,
) -> Result<HttpResponse> {
    let transactions: Vec<_> = statement_controller(&state, address, pagination)?
        .into_iter()
        .map(Transaction::from)
        .collect();
    Ok(HttpResponse::Ok()
        .header(X_SUPRA_CHAIN_ID, state.chain_id().to_string())
        .json(&transactions))
}

/// Get account auto-transaction v3
///
/// List of finalized automated transactions based on the automation tasks registered by the move account.
/// Return max 100 transactions per request.
// --- Below is not public API doc, but internal design doc ---
// It use sequence number as the second index to store the transactions for an account.
//
#[utoipa::path(
    get,
    path = "/rpc/v3/accounts/{address}/automated_transactions",
    operation_id = "statement_automated_v3",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountAutomatedTxPagination,
    ),
    responses(
        (status = 200,
            description = "List of automated transactions of the move account.",
            headers(
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
                ("x-supra-cursor" = String, description = "Cursor specifying where to start for pagination. This cursor cannot be derived manually client-side. Instead, you must call this endpoint once without this query parameter specified, and then use the cursor returned in the x-supra-cursor header in the response."),
            ),
            content(
                (AccountStatementV3 = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/automated_transactions")]
pub async fn get_account_automated_transactions_v3(
    state: State<ApiServerState>,
    address: Path<AccountAddress>,
    pagination: Query<AccountAutomatedTxPagination>,
) -> Result<HttpResponse> {
    let account: AccountAddress = address.into_inner();

    let start_block_height = pagination.block_height;
    let count = pagination.count.map_or_else(
        || DEFAULT_SIZE_OF_PAGE,
        |c| min(c, MAX_NUM_OF_TRANSACTIONS_TO_RETURN),
    );

    debug!(
        "Received automated transactions list request \
           (from {start_block_height:?}, count {count:?}) for account {account}"
    );

    let cursor = pagination
        .cursor
        .as_ref()
        .map(hex::decode)
        .transpose()
        .map_err(|_| Error::CursorDecodeError)?;

    let (transactions, cursor) = state.get_account_automated_transactions(
        account,
        start_block_height,
        cursor,
        pagination.should_traverse_ascending(),
        count,
    )?;
    let transactions: Vec<_> = transactions.into_iter().map(Transaction::from).collect();

    let cursor = cursor.map(hex::encode).unwrap_or("".to_string());

    Ok(HttpResponse::Ok()
        .header(X_SUPRA_CHAIN_ID, state.chain_id().to_string())
        .header(X_SUPRA_CURSOR, cursor)
        .json(&transactions))
}

/// Get coin transactions v3
///
/// List of finalized coin withdraw/deposit transactions relevant to the move account. Return max 100 transactions per request.
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
    path = "/rpc/v3/accounts/{address}/coin_transactions",
    operation_id = "coin_transactions_v3",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountCoinTxPagination,
    ),
    responses(
        (status = 200,
            description = "List of *finalized* transactions sent from and received by the move account.",
            headers (
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
                ( "x-supra-cursor" = String, description = "Cursor specifying where to start for pagination. This cursor cannot be derived manually client-side. Instead, you must call this endpoint once without this query parameter specified, and then use the cursor returned in the x-supra-cursor header in the response."),
            ),
            content(
                (AccountStatementV3 = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/coin_transactions")]
pub async fn coin_transactions_v3(
    state: State<ApiServerState>,
    address: Path<AccountAddress>,
    pagination: Query<AccountCoinTxPagination>,
) -> Result<HttpResponse> {
    let (record, cursor) = coin_transactions_controller_v3(&state, address, pagination)?;
    Ok(HttpResponse::Ok()
        .header(X_SUPRA_CHAIN_ID, state.chain_id().to_string())
        .header(X_SUPRA_CURSOR, cursor)
        .json(&record))
}

/// Get account resources v3
///
/// Retrieves all account resources for a given account.
#[utoipa::path(
    get,
    path = "/rpc/v3/accounts/{address}/resources",
    operation_id = "get_account_resources_v3",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountPublishedListPagination,
    ),
    responses(
        (status = 200,
            description = "Account resource list",
            headers (
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
                ( "x-supra-cursor" = String, description = "Cursor specifying where to start for pagination. This cursor cannot be derived manually client-side. Instead, you must call this endpoint once without this query parameter specified, and then use the cursor returned in the x-supra-cursor header in the response."),
            ),
            content(
                (Vec<MoveResource> = "application/json" ),
            )
        ),
    ),
)]
#[web::get("/{address}/resources")]
pub async fn get_account_resources_v3(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
    pagination: Query<AccountPublishedListPagination>,
) -> Result<HttpResponse> {
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
    let cursor = cursor.map(hex::encode).unwrap_or("".to_string());
    Ok(HttpResponse::Ok()
        .header(X_SUPRA_CHAIN_ID, state.chain_id().to_string())
        .header(X_SUPRA_CURSOR, cursor)
        .json(&resources))
}

/// Get account modules v3
///
/// Retrieves all account modules' bytecode for a given account.
#[utoipa::path(
    get,
    path = "/rpc/v3/accounts/{address}/modules",
    operation_id = "get_account_modules_v3",
    params(
        ("address" = String, Path, description = "Address of account with or without a 0x prefix", example = "0x88fbd33f54e1126269769780feb24480428179f552e2313fbe571b72e62a1ca1"),
        AccountPublishedListPagination,
    ),
    responses(
     (status = 200,
            description = "Account module list",
            headers (
                ("x-supra-chain-id" = u8, description = "Chain ID of the current chain"),
                ( "x-supra-cursor" = String, description = "Cursor specifying where to start for pagination. This cursor cannot be derived manually client-side. Instead, you must call this endpoint once without this query parameter specified, and then use the cursor returned in the x-supra-cursor header in the response."),
            ),
            content(
                (Vec<MoveModuleBytecode> = "application/json"),
            ),
        ),
    ),
)]
#[web::get("/{address}/modules")]
pub async fn get_account_modules_v3(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
    pagination: Query<AccountPublishedListPagination>,
) -> Result<HttpResponse> {
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
    let success = module_list
        .modules
        .iter()
        .filter_map(|module_id| {
            get_module_byte_code_from_module_id(&state, module_id)
                .ok()
                .flatten()
        })
        .collect::<Vec<MoveModuleBytecode>>();
    let cursor = cursor.map(hex::encode).unwrap_or("".to_string());
    Ok(HttpResponse::Ok()
        .header(X_SUPRA_CHAIN_ID, state.chain_id().to_string())
        .header(X_SUPRA_CURSOR, cursor)
        .json(&success))
}
