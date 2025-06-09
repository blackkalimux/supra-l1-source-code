use move_core_types::account_address::AccountAddress;
use thiserror::Error;
use types::genesis::vesting::VestingAccount;

#[derive(Debug, Error)]
pub enum GenesisToolError {
    #[error("Key error: {0} ")]
    Key(String),
    #[error("Profile {0} already exists")]
    AlreadyExist(String),
    #[error("Verifying signature failure: {0}")]
    SignatureError(String),
    #[error("Parsing URL failure: {0}")]
    URL(String),
    #[error("HTTP request failure: {0:?}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Failed to deserialize {0} from {1}")]
    DeserializationFailed(String, String),
    #[error("Downloading faillure: {0}")]
    Download(String),
    #[error("Verification failed: The input Genesis Blob does not match the locally generated Genesis Blob")]
    InvalidInputGenesisBlob,
    #[error("I/O failure: {0:?}")]
    IO(#[from] std::io::Error),
    #[error("Unknown vesting pool IDs detected {0:?}")]
    UnknownVestingPoolIDs(Vec<VestingAccount>),
    #[error("All account balance definitions must be unique. {0:?}")]
    AccountsNotUnique(Vec<AccountAddress>),
    #[error(
        "There must be at least nine participants in the Foundation Mainnet Multisig Account."
    )]
    NotEnoughFoundationMultisigMembers,
    #[error("The total allocation of {0} Supra exceeds the maximum mintable supply of {1}.")]
    MintableSupplyExceeded(u64, u64),
    #[error("The total allocation of {0} Supra was less than the expected amount of {1}.")]
    InsufficientAllocation(u64, u64),
    #[error("Missing the default delegation pool unlock schedule.")]
    MissingDefaultDelegationPoolSchedule,
    #[error("Missing a Foundation Multisig Account definition for the account name prefix: {0}.")]
    MissingMultisigAccountWithName(String),
    #[error("The are not enough Supra Foundation-operated delegation pools to satisfy all DelegationPoolUnlockSchedules.")]
    NotEnoughFoundationDelegationPools,
    #[error("PBO owner's stake {0:?} quants short. FoundationAccountsSchema only mints {1:?}")]
    InvalidPBOOwnerStake(u64, u64),
}
