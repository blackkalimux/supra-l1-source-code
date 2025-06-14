use thiserror::Error;

pub mod v1;
pub mod v3;

#[derive(Error, Debug)]
pub enum MoveListError {
    #[error("StateValue for resource group is missing from MoveStore: {0}")]
    MissingResourceGroupData(String),
    #[error(transparent)]
    StateKeyDecode(#[from] aptos_types::state_store::state_key::inner::StateKeyDecodeErr),
    #[error(transparent)]
    RocksDbError(#[from] rocksdb::Error),
    #[error(transparent)]
    AnyHowError(#[from] anyhow::Error),
    #[error(transparent)]
    Bcs(#[from] bcs::Error),
}
