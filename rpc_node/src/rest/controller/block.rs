use crate::error::Error;
use crate::error::Result;
use crate::rest::ApiServerState;
use archive::reader::ArchiveReader;
use committees::Height;
use ntex::web::types::State;
use socrypto::Hash;
use types::api::v3::BlockMetadataInfo;
use types::api::TransactionType;

pub(crate) fn get_user_txns_by_block_hash(
    state: &State<ApiServerState>,
    block_hash: Hash,
) -> Result<Vec<Hash>> {
    let reader = state.storage.archive().reader();
    let height = get_block_height_by_hash(&reader, block_hash)?;
    get_user_txns_by_block_height(state, height)
}

pub(crate) fn get_user_txns_by_block_height(
    state: &State<ApiServerState>,
    block_height: Height,
) -> Result<Vec<Hash>> {
    state
        .storage
        .archive()
        .reader()
        .get_txs_by_block_height(block_height)
        .map_err(From::from)
}

pub(crate) fn get_block_metadata_info_by_height(
    state: &State<ApiServerState>,
    height: Height,
    txn_type: &Option<TransactionType>,
) -> Result<Option<BlockMetadataInfo>> {
    match txn_type {
        None | Some(TransactionType::Meta) => state
            .get_block_metadata_by_block_height(height)
            .map_err(From::from),
        _ => Ok(None),
    }
}

pub(crate) fn get_txns_by_block_hash_and_type(
    state: &State<ApiServerState>,
    block_hash: Hash,
    txn_type: Option<TransactionType>,
) -> Result<Vec<Hash>> {
    let reader = state.storage.archive().reader();
    let block_height = get_block_height_by_hash(&reader, block_hash)?;

    let user_transactions = match txn_type {
        None | Some(TransactionType::User) => get_user_txns_by_block_height(state, block_height)?,
        _ => vec![],
    };
    let auto_transactions = match txn_type {
        None | Some(TransactionType::Auto) => state
            .storage
            .archive()
            .reader()
            .get_automated_txs_by_block_height(block_height)?,
        _ => vec![],
    };
    let metadata_txn_hash = match txn_type {
        None | Some(TransactionType::Meta) => state
            .storage
            .archive()
            .reader()
            .get_block_metadata_hash_by_block_height(block_height)?,
        _ => None,
    };
    let mut transactions = metadata_txn_hash.map(|hash| vec![hash]).unwrap_or_default();
    transactions.extend(user_transactions);
    transactions.extend(auto_transactions);
    Ok(transactions)
}

fn get_block_height_by_hash(reader: &ArchiveReader, block_hash: Hash) -> Result<Height> {
    reader
        .get_block_header_info(block_hash)?
        .ok_or(Error::InformationNotAvailable(format!(
            "Block header for hash {} not found",
            block_hash
        )))
        .map(|info| info.height)
}
