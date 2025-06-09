use crate::common::error::CliError;
use crate::rest_utils::action::{CliEstimatedGasPrice, RpcAction, RpcResponse};
use crate::rest_utils::client::supra_rpc_client::RpcClientApi;
use crate::rest_utils::client::RPCClientIfc;
use crate::rest_utils::rpc_request_kind::signed_transaction::SignedTransactionHelper;
use crate::rest_utils::rpc_request_kind::RpcRequestKind;
use crate::rest_utils::tool_option::GasParameters;
use aptos_crypto::ed25519::{Ed25519PrivateKey, Ed25519PublicKey};
use aptos_crypto::{PrivateKey, Uniform};
use aptos_types::chain_id::ChainId;
use aptos_types::transaction::{Script, TransactionPayload};
use async_trait::async_trait;
use reqwest::Url;
use std::str::FromStr;
use types::api::v1::AccountInfo;
use types::api::v2::GasPriceResV2;
use types::api::ApiVersion;
use types::cli_profile::profile_management::{SignerKeyType, SignerProfileHelper};
use types::cli_utils::constants::{DEFAULT_GAS_UNIT_PRICE, DEFAULT_MAX_GAS_AMOUNT};
use types::settings::backlog;
use types::settings::constants::{LOCALNET_CHAIN_ID, LOCALNET_RPC_URL};
use types::transactions::transaction::AccountAddress;

pub struct RpcClientMocked {
    rpc_action: RpcAction,
    account_information: Option<AccountInfo>,
    chain_id: Option<ChainId>,
}

impl RpcClientMocked {
    pub fn with_sequence_number(self, sequence_number: u64) -> Self {
        Self {
            account_information: Some(AccountInfo {
                sequence_number,
                authentication_key: Default::default(),
            }),
            ..self
        }
    }

    pub fn with_chain_id(self, chain_id: ChainId) -> Self {
        Self {
            chain_id: Some(chain_id),
            ..self
        }
    }
}

#[async_trait]
impl RPCClientIfc for RpcClientMocked {
    fn new_action(
        _client: reqwest::Client,
        rpc_action: RpcAction,
        _rpc_request_kind: RpcRequestKind,
    ) -> Result<Self, CliError>
    where
        Self: Sized,
    {
        Ok(Self {
            rpc_action,
            account_information: None,
            chain_id: None,
        })
    }

    fn endpoint_url(&self) -> Result<Url, CliError> {
        unimplemented!()
    }

    async fn execute_action(self) -> Result<RpcResponse, CliError> {
        // TODO: This function need a proper implementation.
        match &self.rpc_action {
            RpcAction::ChainID(..) => Ok(RpcResponse::ChainID(
                self.chain_id.unwrap_or(ChainId::new(1)),
            )),
            RpcAction::EstimatedGasPrice(..) => Ok(RpcResponse::EstimatedGasPrice(
                CliEstimatedGasPrice::V2(GasPriceResV2 {
                    mean_gas_price: 100,
                    max_gas_price: 100,
                    median_gas_price: 100,
                }),
            )),
            RpcAction::AccountInformation(..) => Ok(RpcResponse::AccountInformation(
                self.account_information.unwrap_or(AccountInfo {
                    sequence_number: 0,
                    authentication_key: Default::default(),
                }),
            )),
            RpcAction::SendTransaction(..) | RpcAction::SendSignedTransaction(..) => {
                Ok(RpcResponse::AccountInformation(AccountInfo {
                    sequence_number: 0,
                    authentication_key: Default::default(),
                }))
            }
            RpcAction::SimulateTransaction(..) => {
                Ok(RpcResponse::AccountInformation(AccountInfo {
                    sequence_number: 0,
                    authentication_key: Default::default(),
                }))
            }
            RpcAction::RunViewFunction(..) => Ok(RpcResponse::AccountInformation(AccountInfo {
                sequence_number: 0,
                authentication_key: Default::default(),
            })),
            RpcAction::ListAccountAssets(..) => Ok(RpcResponse::AccountInformation(AccountInfo {
                sequence_number: 0,
                authentication_key: Default::default(),
            })),
            RpcAction::TransactionInfo(..) => Ok(RpcResponse::AccountInformation(AccountInfo {
                sequence_number: 0,
                authentication_key: Default::default(),
            })),
            RpcAction::AccountResource(..) => Ok(RpcResponse::AccountInformation(AccountInfo {
                sequence_number: 0,
                authentication_key: Default::default(),
            })),
            RpcAction::AccountModule(..) => Ok(RpcResponse::AccountInformation(AccountInfo {
                sequence_number: 0,
                authentication_key: Default::default(),
            })),
            RpcAction::FundWithFaucet(..) => Ok(RpcResponse::AccountInformation(AccountInfo {
                sequence_number: 0,
                authentication_key: Default::default(),
            })),
        }
    }
}

#[tokio::test]
async fn test_sequence_number_api() {
    let api_version = ApiVersion::V3;
    let profile_secret = Ed25519PrivateKey::generate_for_testing();
    let txn_sender_address = AccountAddress::random();
    let signed_txn_helper = SignedTransactionHelper::new(
        txn_sender_address,
        Ed25519PublicKey::from(&profile_secret),
        SignerKeyType::Execute(profile_secret),
        DEFAULT_MAX_GAS_AMOUNT,
        DEFAULT_GAS_UNIT_PRICE,
        backlog::Parameters::percentage_of_default_tx_ttl(0.75),
        Url::from_str(LOCALNET_RPC_URL).unwrap(),
        ChainId::new(LOCALNET_CHAIN_ID),
    );
    let rpc_client = RpcClientMocked::new_with_action(
        RpcAction::AccountInformation(api_version, txn_sender_address),
        RpcRequestKind::SignedTransaction(signed_txn_helper),
    )
    .unwrap()
    .with_sequence_number(1);
    let RpcResponse::AccountInformation(acc_info) = rpc_client.execute_action().await.unwrap()
    else {
        unimplemented!()
    };
    assert_eq!(acc_info.sequence_number, 1);
}

#[tokio::test]
async fn test_chain_id_api() {
    let api_version = ApiVersion::V3;
    let txn_profile = Ed25519PrivateKey::generate_for_testing();
    let signed_txn_helper = SignedTransactionHelper::new(
        AccountAddress::random(),
        Ed25519PublicKey::from(&txn_profile),
        SignerKeyType::Execute(txn_profile),
        DEFAULT_MAX_GAS_AMOUNT,
        DEFAULT_GAS_UNIT_PRICE,
        backlog::Parameters::percentage_of_default_tx_ttl(0.75),
        Url::from_str(LOCALNET_RPC_URL).unwrap(),
        ChainId::new(LOCALNET_CHAIN_ID),
    );
    let rpc_client = RpcClientMocked::new_with_action(
        RpcAction::ChainID(api_version),
        RpcRequestKind::SignedTransaction(signed_txn_helper),
    )
    .unwrap()
    .with_chain_id(ChainId::new(42));
    let RpcResponse::ChainID(chain_id) = rpc_client.execute_action().await.unwrap() else {
        unimplemented!()
    };
    assert_eq!(chain_id, ChainId::new(42));
}

#[tokio::test]
async fn test_create_raw_user_transaction() {
    let user_account = AccountAddress::random();
    let secret_key = Ed25519PrivateKey::generate_for_testing();
    let transaction_payload = TransactionPayload::Script(Script::new(vec![], vec![], vec![]));
    let signed_txn_helper = SignedTransactionHelper::new_user_object(
        SignerProfileHelper::new(
            user_account,
            Ed25519PublicKey::from(&secret_key),
            SignerKeyType::Execute(secret_key),
            Url::from_str(LOCALNET_RPC_URL).unwrap(),
            ChainId::new(LOCALNET_CHAIN_ID),
            "PASSWORD".to_string(),
        ),
        GasParameters::new(DEFAULT_GAS_UNIT_PRICE, DEFAULT_MAX_GAS_AMOUNT, 0),
    );
    let result = signed_txn_helper
        .create_raw_user_transaction(transaction_payload, 1, 0)
        .await;
    assert!(result.is_ok())
}

#[tokio::test]
async fn test_error_handling() {
    let api_version = ApiVersion::V3;
    let sender_key = Ed25519PrivateKey::generate_for_testing();
    let signed_txn_helper = SignedTransactionHelper::new(
        AccountAddress::random(),
        Ed25519PublicKey::from(&sender_key),
        SignerKeyType::Execute(sender_key),
        DEFAULT_MAX_GAS_AMOUNT,
        DEFAULT_GAS_UNIT_PRICE,
        backlog::Parameters::percentage_of_default_tx_ttl(0.75),
        Url::from_str(LOCALNET_RPC_URL).unwrap(),
        ChainId::new(LOCALNET_CHAIN_ID),
    );
    let rpc_client = RpcClientApi::new_with_action(
        RpcAction::ChainID(api_version),
        RpcRequestKind::SignedTransaction(signed_txn_helper),
    )
    .unwrap();
    assert!(rpc_client.execute_action().await.is_err());
}

#[tokio::test]
async fn test_create_transaction() {
    let api_version = ApiVersion::V3;
    // Mock successful chain ID and sequence number fetching (replace with actual mocks)
    let mock_chain_id = 1;
    let mock_sequence_number = 0;

    // Generate sample keys and account address
    let private_key = Ed25519PrivateKey::generate_for_testing();
    let public_key = private_key.public_key();
    let user_account = AccountAddress::random();

    // Create a transaction payload
    let transaction_payload = TransactionPayload::Script(Script::new(vec![], vec![], vec![]));

    // Call create_transaction with mocked data
    let signed_txn_helper = SignedTransactionHelper::new(
        user_account,
        public_key,
        SignerKeyType::Execute(private_key),
        DEFAULT_MAX_GAS_AMOUNT,
        DEFAULT_GAS_UNIT_PRICE,
        backlog::Parameters::percentage_of_default_tx_ttl(0.75),
        Url::from_str(LOCALNET_RPC_URL).unwrap(),
        ChainId::new(LOCALNET_CHAIN_ID),
    );
    let rpc_client = RpcClientApi::new_with_action(
        RpcAction::ChainID(api_version),
        RpcRequestKind::SignedTransaction(signed_txn_helper),
    )
    .unwrap();

    let signed_txn_helper = match &rpc_client.rpc_request_kind {
        RpcRequestKind::SignedTransaction(helper) => helper,
        RpcRequestKind::PublicQuery(_) => {
            panic!("Transaction factory should not be public query in this test.")
        }
    };

    let result = signed_txn_helper
        .create_transaction(
            transaction_payload.clone(),
            mock_chain_id,
            mock_sequence_number,
        )
        .await;

    // Assert successful transaction creation
    assert!(result.is_ok());

    // Check some properties of the signed transaction (more validation can be added)
    let signed_txn = result.unwrap();
    assert_eq!(signed_txn.sender(), user_account);
    assert_eq!(signed_txn.sequence_number(), mock_sequence_number);
}
