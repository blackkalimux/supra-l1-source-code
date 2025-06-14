use crate::rest::api_server_error::ApiServerError;
use crate::rest::{ApiServerState, PublicApiServerError};
use socrypto::Hash;
use types::api::v3::Transaction;
use types::api::TransactionType;

pub fn get_transaction_by_hash(
    txn_hash: Hash,
    maybe_txn_type: Option<TransactionType>,
    state: &ApiServerState,
) -> Result<Option<Transaction>, PublicApiServerError> {
    let Some(txn_type) = maybe_txn_type else {
        let maybe_move_txn = state.get_transaction_by_hash(txn_hash, true);
        let result = match maybe_move_txn {
            Ok(maybe_transaction) => maybe_transaction.map(Transaction::from),
            Err(PublicApiServerError(ApiServerError::TransactionNotFound(_))) => state
                .get_block_metadata_by_hash(txn_hash)?
                .map(Transaction::from),
            Err(err) => return Err(err),
        };
        return Ok(result);
    };
    let result = match txn_type {
        TransactionType::Auto => state
            .get_automated_transaction_by_hash(txn_hash)?
            .map(Transaction::from),
        TransactionType::User => state
            .get_user_transaction_by_hash(txn_hash, true)?
            .map(Transaction::from),
        TransactionType::Meta => state
            .get_block_metadata_by_hash(txn_hash)?
            .map(Transaction::from),
    };
    Ok(result)
}
