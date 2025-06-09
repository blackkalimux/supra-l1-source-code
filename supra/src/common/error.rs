use crate::rest_utils::action::RpcResponse;
use crate::{
    genesis::error::GenesisToolError, node::identity_rotation::error::IdentityRotationError,
};
use inquire::InquireError;
use move_core_types::account_address::AccountAddressParseError;
use thiserror::Error;
use types::cli_profile::profile_management::CliProfileError;
use types::genesis::error::GenesisConfigError;
use types::settings::committee::error::GenesisBlobError;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("Command Aborted Reason({0}), Action({1})")]
    Aborted(String, String),
    #[error("Uninitialized Reason({0}), Action({1})")]
    UnInitialized(String, String),
    #[error("Hex decode error: {0}")]
    HexDecodeError(#[from] hex::FromHexError),
    #[error("Json serde: {0:?}")]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("Standard io: {0:?}")]
    StdIoError(#[from] std::io::Error),
    #[error("SupraCrypto: {0:?}")]
    SupraCryptoError(#[from] socrypto::SupraCryptoError),
    #[error("Dkg helper: {0:?}")]
    DkgError(#[from] nidkg_helper::errors::DkgError),
    #[error("Encryption failed. Reason: {0:?}")]
    Encryption(#[from] encryption::error::EncryptionError),
    #[error("Decryption failed. Please check the password.")]
    DecryptionFailed,
    #[error("Rocksdb: {0:?}")]
    RocksDB(#[from] rocksdb::Error),
    #[error("General: {0}")]
    GeneralError(String),
    #[error("DKG Transcript does not have enough valid dealings for aggregation, required = {0}, current = {1}")]
    DKGTranscriptError(u32, usize),
    #[error("Move tool: {0:?}")]
    MoveToolingError(#[from] crate::move_tool::error::MoveToolError),
    #[error("Other: {0:?}")]
    Other(#[from] anyhow::Error),
    #[error("Data tool: {0:?}")]
    DataToolError(String),
    #[error("CliProfile: {0:?}")]
    ProfileError(#[from] CliProfileError),
    #[error("Genesis tool: {0:?}")]
    GenesisToolError(#[from] GenesisToolError),
    #[error("Genesis Blob: {0:?}")]
    GenesisBlobError(#[from] GenesisBlobError),
    #[error("Genesis accounts: {0:?}")]
    GenesisConfigError(#[from] GenesisConfigError),
    #[error("Identity rotation: {0:?}")]
    IdentityRotation(#[from] IdentityRotationError),
    #[error("Cannot parse url: {0:?}")]
    UrlError(#[from] url::ParseError),
    #[error("Unauthorized operation")]
    Unauthorized,
    #[error("Unexpected response for transaction {0:?}:  {1:?}")]
    UnexpectedResponse(String, RpcResponse),
    #[error("Request Error: {0:?}")]
    RequestError(#[from] reqwest::Error),
    #[error("Account Address Parse Error: {0:?}")]
    AccountAddressParseError(#[from] AccountAddressParseError),
    #[error("NotFound: {0}")]
    NotFound(String),
    #[error("BCS Error: {0:?}")]
    BcsError(#[from] bcs::Error),
    #[error(transparent)]
    FileIoError(#[from] Box<file_io_types::FileIOError>),
    #[error("Console error {0}")]
    ConsoleError(#[from] tcp_console::Error),
    #[error("REST client error")]
    RESTClient,
    #[error("InquireError {0}")]
    InquireError(#[from] InquireError),
}

impl From<aptos::common::types::CliError> for CliError {
    fn from(value: aptos::common::types::CliError) -> Self {
        Self::GeneralError(value.to_string())
    }
}
