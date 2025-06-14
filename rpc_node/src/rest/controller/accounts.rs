use crate::error::Error;
use crate::rest::{ApiServerState, Converter, PublicApiServerError};
use anyhow::{anyhow, Context};
use aptos_api_types::{MoveModule, MoveValue};
use aptos_types::account_config::AccountResource;
use aptos_vm::data_cache::AsMoveResolver;
use archive::schemas::coin_transactions::TaggedTransactionHash;
use core::str::FromStr;
use move_binary_format::CompiledModule;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag, TypeTag};
use move_core_types::resolver::{ModuleResolver, MoveResolver};
use ntex::web::types::{Path, Query, State};
use std::cmp::min;
use tracing::debug;
use types::api::v3::Transaction;
use types::api::{
    v1::AccountInfo,
    v2::{MoveModuleBytecode, TransactionInfoV2},
    MAX_NUM_OF_TRANSACTIONS_TO_RETURN,
};
use types::pagination::{AccountCoinTxPagination, AccountTxPagination, DEFAULT_SIZE_OF_PAGE};
use version_derive::TakeVariant;

#[derive(TakeVariant)]
pub enum AccountInformationVariant {
    V1(Option<AccountInfo>),
    V2(AccountInfo),
}

pub fn account_information_controller_v1(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
) -> crate::error::Result<AccountInformationVariant> {
    let account_address = addr.into_inner();
    let resource = state
        .get_ref()
        .move_store
        .read_resource::<AccountResource>(&account_address);
    let account_info = extract_account_info(resource)?;

    Ok(AccountInformationVariant::V1(account_info))
}

pub fn account_information_controller_v2(
    state: State<ApiServerState>,
    addr: Path<AccountAddress>,
) -> crate::error::Result<AccountInformationVariant> {
    let account_address = addr.into_inner();
    let resource = state
        .get_ref()
        .move_store
        .read_resource::<AccountResource>(&account_address);
    let account_info = extract_account_info(resource)?;

    let account_info = account_info.ok_or(Error::InformationNotAvailable(
        account_address.to_canonical_string(),
    ))?;
    Ok(AccountInformationVariant::V2(account_info))
}

fn extract_account_info(
    resource: Option<AccountResource>,
) -> crate::error::Result<Option<AccountInfo>> {
    resource
        .map(|acc| {
            Ok(AccountInfo {
                sequence_number: acc.sequence_number(),
                authentication_key: acc
                    .authentication_key()
                    .try_into()
                    .context("Convert auth-key bytes to Hash")?,
            })
        })
        .transpose()
}

#[derive(TakeVariant)]
pub enum MoveResourceVariant {
    V1(Option<MoveValue>),
    V2(MoveValue),
}

pub fn move_resource_controller_v1(
    state: State<ApiServerState>,
    path_param: Path<(AccountAddress, String)>,
) -> crate::error::Result<MoveResourceVariant> {
    let (account_address, str_tag) = path_param.into_inner();
    let resolver = state.move_store.as_move_resolver();
    let converter = Converter::new(&state.move_store);
    // Return Invalid Input from anyhow error
    let str_tag =
        StructTag::from_str(str_tag.as_str()).map_err(|e| Error::InvalidInput(e.to_string()))?;
    let maybe_bytes = resolver
        .get_resource(&account_address, &str_tag)
        .map_err(|e| anyhow!(e))?;
    let type_tag = TypeTag::from(str_tag);
    let val = maybe_bytes.and_then(|data| converter.convert_to_move_value(type_tag, &data).ok());
    Ok(MoveResourceVariant::V1(val))
}

pub fn move_resource_controller_v2(
    state: State<ApiServerState>,
    path_param: Path<(AccountAddress, String)>,
) -> crate::error::Result<MoveResourceVariant> {
    let (account_address, str_tag) = path_param.into_inner();
    let resolver = state.move_store.as_move_resolver();
    let converter = Converter::new(&state.move_store);
    // Return Invalid Input from anyhow error
    let str_tag =
        StructTag::from_str(str_tag.as_str()).map_err(|e| Error::InvalidInput(e.to_string()))?;
    let maybe_bytes = resolver
        .get_resource(&account_address, &str_tag)
        .map_err(|e| anyhow!(e))?;
    let type_tag = TypeTag::from(str_tag);
    let bytes = maybe_bytes.ok_or(Error::InformationNotAvailable(
        account_address.to_canonical_string(),
    ))?;
    let val = converter
        .convert_to_move_value(type_tag, &bytes)
        .map_err(|e| Error::PublicApiServerError(PublicApiServerError::from(e)))?;
    Ok(MoveResourceVariant::V2(val))
}

pub fn get_module_byte_code_from_module_id(
    state: &State<ApiServerState>,
    module_id: &ModuleId,
) -> crate::error::Result<Option<MoveModuleBytecode>> {
    let resolver = state.move_store.as_move_resolver();
    let maybe_module_bytes = resolver
        .get_module(module_id)
        .map_err(|e| anyhow::anyhow!(e))?;

    let maybe_compiled_module = maybe_module_bytes
        .map(|module_bytes| {
            CompiledModule::deserialize(&module_bytes)
                .map(|compiled_mod| {
                    MoveModuleBytecode::new(module_bytes.to_vec(), MoveModule::from(compiled_mod))
                })
                .map_err(Error::MoveVmPartialError)
        })
        .transpose()?;
    Ok(maybe_compiled_module)
}

pub fn statement_controller(
    state: &State<ApiServerState>,
    address: Path<AccountAddress>,
    pagination: Query<AccountTxPagination>,
) -> crate::error::Result<Vec<TransactionInfoV2>> {
    let account: AccountAddress = address.into_inner();

    let start_sequence_number = pagination.start;
    let count = pagination.count.map_or_else(
        || DEFAULT_SIZE_OF_PAGE,
        |c| min(c, MAX_NUM_OF_TRANSACTIONS_TO_RETURN),
    );

    debug!("Received transactions list request (from {start_sequence_number:?}, count {count:?}) for account {account}");

    // TODO: When transactions are indexed by their execution log version, the [None] parameters
    //  should be set such that this function call only retrieves the information required by
    //  the pagination parameters.
    Ok(state.get_account_user_transactions(account, start_sequence_number, count)?)
}

pub fn coin_transactions_controller_v2(
    state: State<ApiServerState>,
    address: Path<AccountAddress>,
    pagination: Query<AccountCoinTxPagination>,
) -> crate::error::Result<(Vec<TransactionInfoV2>, u64)> {
    let account: AccountAddress = address.into_inner();
    let count = pagination.count.map_or_else(
        || DEFAULT_SIZE_OF_PAGE,
        |c| min(c, MAX_NUM_OF_TRANSACTIONS_TO_RETURN),
    );
    let (txn_hashes, cursor) =
        state.get_account_coin_transaction_hashes(account, pagination.start, count)?;
    let user_txn_hashes = txn_hashes
        .into_iter()
        .filter_map(TaggedTransactionHash::into_user_txn_hash)
        .collect();
    let transactions =
        state.get_account_executed_transactions_by_hashes(account, user_txn_hashes)?;
    Ok((transactions, cursor))
}

pub fn coin_transactions_controller_v3(
    state: &State<ApiServerState>,
    address: Path<AccountAddress>,
    pagination: Query<AccountCoinTxPagination>,
) -> crate::error::Result<(Vec<Transaction>, u64)> {
    let account: AccountAddress = address.into_inner();
    let count = pagination
        .count
        .unwrap_or(DEFAULT_SIZE_OF_PAGE)
        .min(MAX_NUM_OF_TRANSACTIONS_TO_RETURN);

    let (transactions, cursor) =
        state.get_account_coin_transactions(account, pagination.start, count)?;
    Ok((transactions, cursor))
}
