use crate::consensus::block_store::BlockStoreError;
use consistency_warranter::StateConsistencyWarranterError;
use execution::error::ExecutionError;
use rpc::clients::RpcClientError as RpcLayerError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssemblerError {
    #[error(transparent)]
    RpcInternalError(#[from] RpcLayerError),
    #[error("Fatal: shutdown node due to: {0}")]
    Fatal(String),
    #[error("BlockVerification: Failed due to: {0}")]
    BlockVerification(String),
    #[error("InvalidBlockStoreState: {0}")]
    InvalidBlockStoreState(String),
    #[error("InvalidBlockStoreOperation: {0}")]
    InvalidBlockStoreOperation(String),
    #[error("Regression: {0}")]
    Regression(String),
    #[error(transparent)]
    ExecutionFailed(#[from] ExecutionError),
}

/// Persistent store (read-write) errors or fatal errors reported from block-store indicate that assembler
/// can not move forward anymore.
/// The other errors are reported due to invalid state or invalid usage of the [BlockStore] are
/// self-recoverable, so that's why they are not considered fatal at the moment.
/// They will be simply reported for investigation purposes.
impl From<BlockStoreError> for AssemblerError {
    fn from(value: BlockStoreError) -> Self {
        match value {
            BlockStoreError::InvalidOperation(m) => AssemblerError::InvalidBlockStoreOperation(m),
            BlockStoreError::InvalidState(m) => AssemblerError::InvalidBlockStoreState(m),
            BlockStoreError::PersistentStore(_) => AssemblerError::Fatal(format!("{value:?}")),
            BlockStoreError::Fatal(m) => AssemblerError::Fatal(m),
            BlockStoreError::StorageWriter(m) => AssemblerError::Fatal(format!("{m:?}")),
        }
    }
}

/// If state consistency warranter fails then assembler can not move forward, so it is fatal error.
impl From<StateConsistencyWarranterError> for AssemblerError {
    fn from(value: StateConsistencyWarranterError) -> Self {
        AssemblerError::Fatal(format!("{value:?}"))
    }
}

pub type ChainStateAssemblerResult<T> = Result<T, AssemblerError>;
