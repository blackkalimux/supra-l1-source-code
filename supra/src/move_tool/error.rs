use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MoveToolError {
    #[error("General Error: {0:?}")]
    GeneralError(#[from] anyhow::Error),
    #[error("Failed to remove {path}, cause: {source:?}")]
    FailedToRemove {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Resource {path} not exist, cause: {source:?}")]
    ResourceNotExist {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Executor Crypto Error: {0:?}")]
    ExecutorCryptoError(#[from] aptos_crypto::CryptoMaterialError),
    #[error("Reqwest Error: {0:?}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Bcs Error: {0:?}")]
    BcsError(#[from] bcs::Error),
    #[error("Failed to parse address {0}")]
    FailedToParseAddress(String),
}
