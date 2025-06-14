use crate::rest::move_list;
use aptos_types::account_address::AccountAddress;
use archive::error::ArchiveError;
use errors::SmrError;
use ntex::http::StatusCode;
use rocksstore::chain_storage::block_store::StoreError;
use socrypto::Hash;
use thiserror::Error;

#[derive(Error, Debug)]
#[error("Internal server error: {0}")]
pub struct PublicApiServerError(#[from] pub(crate) ApiServerError);

impl PublicApiServerError {
    pub fn status_code(&self) -> StatusCode {
        match self.0 {
            ApiServerError::TooManyConcurrentSimulations => StatusCode::TOO_MANY_REQUESTS,
            ApiServerError::TransactionNotFound(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::BAD_REQUEST,
        }
    }
}

#[derive(Error, Debug)]
pub(crate) enum ApiServerError {
    #[error("Encountered inconsistent state in the archive database: {0}")]
    InconsistentArchiveState(String),
    #[error("Missing transaction {0} for account {1}")]
    MissingAccountTransaction(Hash, AccountAddress),
    #[error("Missing automated transaction {0} for account {1}")]
    MissingAccountAutomatedTransaction(Hash, AccountAddress),
    #[error("Missing transaction {0} in block {1}")]
    MissingBlockTransaction(Hash, Hash),
    #[error("Missing user transaction {0} executed as part of block {1}")]
    MissingBlockTransactionOutput(Hash, Hash),
    #[error("Missing automated transaction {0} executed as part of block {1}")]
    MissingBlockAutomatedTransaction(Hash, Hash),
    #[error("Missing the output data for automated transaction {0}")]
    MissingAutomatedTransactionOutput(Hash),
    #[error("Missing the output data for block metadata transaction {0}")]
    MissingBlockMetadataOutput(Hash),
    #[error("Missing the execution status for transaction {0}")]
    MissingTransactionStatus(Hash),
    #[error("Cannot find transaction {0}")]
    TransactionNotFound(Hash),
    #[error("Encountered inconsistent state in Store: {0}")]
    InconsistentStoreState(String),
    #[error("Too many concurrent simulations")]
    TooManyConcurrentSimulations,
    #[error("Transaction to be simulated should not have valid signature.")]
    InvalidSimulationTransaction,
    #[error("MoveList error: {0:?}")]
    MoveListError(#[from] move_list::MoveListError),

    // Derived variants are `transparent` because clients do not need to know about
    // the internal implementation. Error messages should be made unique enough such that
    // their source can be found by searching for the message.
    #[error("Archive error: {0:?}")]
    ArchiveError(#[from] ArchiveError),
    #[error("Other error: {0:?}")]
    Other(#[from] anyhow::Error),
    #[error("Store error: {0:?}")]
    StoreError(#[from] StoreError),
    #[error("SMR error: {0:?}")]
    Smr(#[from] SmrError),

    #[error("Bcs error: {0:?}")]
    Bcs(#[from] bcs::Error),
}
