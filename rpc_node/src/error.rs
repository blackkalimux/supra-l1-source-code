use crate::consensus::AssemblerError;
use crate::rest::faucet_call_limiter::FaucetLimiterError;
use crate::rest::PublicApiServerError;
use crate::transactions::dispatcher::TransactionDispatchError;
use archive::error::ArchiveError;
use execution::error::ExecutionError;
use move_binary_format::errors::PartialVMError;
use ntex::http::StatusCode;
use ntex::web::{HttpRequest, HttpResponse, WebResponseError};
use rocksstore::chain_storage::block_store::StoreError;
use serde::{Deserialize, Serialize};
use snapshot::SnapshotError;
use std::io;
use std::time::Duration;
use tcp_console as console;
use thiserror::Error;
use types::settings::committee::error::GenesisBlobError;

/// TODO: Split into 2 sets, RpcRestAPIError, RpcNodeError
#[derive(Debug, Error)]
pub enum Error {
    #[error("Store error: {0}")]
    Store(#[from] StoreError),
    #[error(transparent)]
    RpcClient(#[from] rpc::clients::RpcClientError),
    #[error("Archive error: {0}")]
    Archive(#[from] ArchiveError),
    #[error("TransactionAcceptStatus: {0}")]
    TransactionAcceptStatus(#[from] TransactionDispatchError),
    #[error("Information not available for {0}")]
    InformationNotAvailable(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Execution retrieval error: {0}")]
    ExecutionRetrieval(#[from] ExecutionError),
    #[error("Swagger error: {err}")]
    Swagger { err: String, code: StatusCode },
    #[error("TOML deserialization error: {0}")]
    TomlDeserialize(#[from] toml::de::Error),
    #[error("Bad transaction request: {0}")]
    BadTransactionRequest(String),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
    #[error("Too many requests. Please try again in {0:?}")]
    TooManyRequests(Duration),
    #[error("{0}")]
    PublicApiServerError(#[from] PublicApiServerError),
    #[error("RPC configuration setup error {0:?}")]
    SetupError(String),
    #[error("Operation is not supported: faucet is not configured")]
    FaucetNotEnabled,
    #[error(transparent)]
    FaucetLimiterError(#[from] FaucetLimiterError),
    #[error("SnapshotError: {0}")]
    SnapshotError(#[from] SnapshotError),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Failed to convert move data: {0}")]
    MoveConverterError(String),
    #[error(transparent)]
    MoveVmPartialError(#[from] PartialVMError),
    #[error(transparent)]
    ChainStateAssembler(#[from] AssemblerError),
    #[error(transparent)]
    GenesisBlobError(#[from] GenesisBlobError),
    #[error(transparent)]
    FileIoError(#[from] Box<file_io_types::FileIOError>),
    #[error("TCP Console error: {0}")]
    TcpConsoleError(#[from] console::Error),
    #[error(transparent)]
    TakeVariantError(#[from] version_types::TakeVariantError),
    #[error("Cursor could not be decoded")]
    CursorDecodeError,
}

impl Error {
    pub fn unwrap_or_404<T>(expected_value: Option<T>, context: String) -> Result<T, Self> {
        expected_value.ok_or(Self::InformationNotAvailable(context))
    }
}

/// Error encountered during building a TLS config.
#[derive(Debug, Error)]
pub enum TlsError {
    #[error(transparent)]
    FileError(#[from] io::Error),
    #[error(transparent)]
    RustlsError(#[from] rustls::Error),
    #[error("Certificate private key not found")]
    PrivateKeyAbsent,
    #[error("Single certificate private key expected")]
    TooManyPrivateKeys,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ErrResp {
    message: String,
}

impl ErrResp {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl WebResponseError for Error {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Store(_)
            | Self::RpcClient(_)
            | Self::Archive(_)
            | Self::TransactionAcceptStatus(_)
            | Self::Io(_)
            | Self::FileIoError(_)
            | Self::TomlDeserialize(_)
            | Self::SetupError(_)
            | Self::FaucetNotEnabled
            | Self::SnapshotError(_)
            | Self::ChainStateAssembler(_)
            | Self::GenesisBlobError(_)
            | Self::TcpConsoleError(_)
            | Self::TakeVariantError(_)
            | Self::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadTransactionRequest(_)
            | Self::SerdeJson(_)
            | Self::MoveConverterError(..)
            | Self::MoveVmPartialError(..)
            | Self::InvalidInput(..)
            | Self::CursorDecodeError => StatusCode::BAD_REQUEST,
            Self::ExecutionRetrieval(_) | Self::InformationNotAvailable(_) => StatusCode::NOT_FOUND,
            Self::Swagger { code, .. } => *code,
            Self::TooManyRequests(_) | Self::FaucetLimiterError(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::PublicApiServerError(e) => e.status_code(),
        }
    }

    fn error_response(&self, _: &HttpRequest) -> HttpResponse {
        let msg = match self {
            Self::Store(_) => "store lookup failed".to_string(),
            Self::RpcClient(_) => self.to_string(),
            Self::Archive(_) => "archive lookup failed".to_string(),
            // TODO: consider providing more info than failed.
            Self::TransactionAcceptStatus(_) => "failed".to_string(),
            Self::InformationNotAvailable(_) => self.to_string(),
            Self::Io(_) | Self::FileIoError(_) => "internal server error".to_string(),
            Self::ExecutionRetrieval(_) => "address not found".to_string(),
            Self::SerdeJson(e) => e.to_string(),
            Self::BadTransactionRequest(e) => e.to_string(),
            Self::Swagger { .. } => self.to_string(),
            Self::TomlDeserialize(_) => "TOML deserialization error".to_string(),
            Self::SetupError(msg) => msg.to_string(),
            Self::Other(msg) => msg.to_string(),
            Self::PublicApiServerError(_) => self.to_string(),
            Self::TooManyRequests(_) => self.to_string(),
            Self::FaucetNotEnabled => self.to_string(),
            Self::SnapshotError(e) => format!("Internal error: {e:?}"),
            Self::GenesisBlobError(e) => format!("Internal error: {e:?}"),
            Self::FaucetLimiterError(_) => self.to_string(),
            Self::InvalidInput(msg) => format!("Invalid input: {msg}"),
            Self::MoveConverterError(msg) => {
                format!("Failure handling request inputs/outputs: {msg}")
            }
            Self::MoveVmPartialError(err) => format!("{:?}", err.message()),
            Self::ChainStateAssembler(e) => format!("Internal error: {e:?}"),
            Self::TcpConsoleError(e) => format!("Internal error: {e:?}"),
            Self::TakeVariantError(e) => format!("Internal error: {e:?}"),
            Self::CursorDecodeError => self.to_string(),
        };
        HttpResponse::build(self.status_code())
            .set_header("content-type", "application/json")
            .json(&ErrResp { message: msg })
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
