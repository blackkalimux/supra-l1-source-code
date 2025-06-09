use aptos_api_types::{MoveResource, ViewRequest};
use aptos_types::account_address::AccountAddress;
use aptos_types::chain_id::ChainId;
use aptos_types::transaction::{SignedTransaction, TransactionPayload};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use reqwest::Response;
use rpc_node::rest::move_list::v1::MoveList;
use serde::{Deserialize, Serialize};
use socrypto::Hash;
use transactions::{SmrTransactionHeader, TTransactionHeader, TxExecutionStatus};
use types::api::v1::{FaucetStatus, GasPriceRes, TransactionInfoV1, TransactionOutput};
use types::api::v2::{GasPriceResV2, MoveModuleBytecode, MoveModuleListV2, MoveResourcesResponse};
use types::api::{
    v1::{AccountInfo, MoveListQuery},
    v2::TransactionInfoV2,
    ApiVersion,
};
use url::Url;

#[derive(Debug, Clone)]
pub enum RpcAction {
    /// Get chain id from the rpc.
    ChainID(ApiVersion),
    /// Get estimated gas price.
    EstimatedGasPrice(ApiVersion),
    /// Get sequence number of the account address.
    AccountInformation(ApiVersion, AccountAddress),
    /// Simulate the transaction from the rpc node.
    SimulateTransaction(ApiVersion, TransactionPayload),
    /// Send the transaction to the rpc node.
    SendTransaction(ApiVersion, TransactionPayload),
    /// Send Signed Transaction without querying the chain and account information.
    SendSignedTransaction(ApiVersion, SignedTransaction),
    /// Execute a view function.
    RunViewFunction(ApiVersion, ViewRequest),
    /// List modules and packages.
    ListAccountAssets(ApiVersion, AccountAddress, MoveListQuery),
    /// Transaction hash information.
    TransactionInfo(ApiVersion, Hash),
    /// Get account resource.
    AccountResource(ApiVersion, AccountAddress, TypeTag),
    /// Show Module ABI.
    AccountModule(ApiVersion, AccountAddress, Identifier),
    /// Create or add faucet with supra coin.
    FundWithFaucet(ApiVersion, AccountAddress, Url),
}

#[derive(Debug)]
pub enum RpcResponse {
    /// Response with chain id.
    ChainID(ChainId),
    /// Estimated gas price.
    EstimatedGasPrice(CliEstimatedGasPrice),
    /// Response with sequence number.
    AccountInformation(AccountInfo),
    /// Transaction simulation information
    SimulateTransaction {
        info: CliTransactionInfo,
        expiration_timestamp_secs: u64,
    },
    /// Transaction Hash.
    SendTransaction {
        hash: Hash,
        expiration_timestamp_secs: u64,
    },
    /// Transaction Hash.
    SendSignedTransaction {
        hash: Hash,
        expiration_timestamp_secs: u64,
    },
    /// Http response as text.
    RunViewFunction(Response),
    /// Http response as text.
    ListAccountAssets(CliMoveList),
    /// Transaction information after executing from cli.
    TransactionInfo(CliTransactionInfo),
    /// Http response as text
    AccountResource(Response),
    /// Http response as text
    AccountModule(Response),
    /// Http response as text
    FundWithFaucet(FaucetStatus),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CliMoveList {
    V1(MoveList),
    ResourcesV2(MoveResourcesResponse),
    ModulesV2(MoveModuleListV2),
    ResourcesV3(Vec<MoveResource>),
    ModulesV3(Vec<MoveModuleBytecode>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CliEstimatedGasPrice {
    V1(GasPriceRes),
    V2(GasPriceResV2),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CliTransactionInfo {
    V1(TransactionInfoV1),
    V2(TransactionInfoV2),
}

impl TTransactionHeader for CliTransactionInfo {
    fn header(&self) -> &SmrTransactionHeader {
        match self {
            CliTransactionInfo::V1(t) => t.header(),
            CliTransactionInfo::V2(t) => t.header(),
        }
    }
}

impl CliTransactionInfo {
    pub fn status(&self) -> &TxExecutionStatus {
        match self {
            CliTransactionInfo::V1(t) => t.status(),
            CliTransactionInfo::V2(t) => t.status(),
        }
    }

    pub fn output(&self) -> &Option<TransactionOutput> {
        match self {
            CliTransactionInfo::V1(t) => t.output(),
            CliTransactionInfo::V2(t) => t.output(),
        }
    }
}
