use crate::common::error::CliError;
use crate::rest_utils::action::{
    CliEstimatedGasPrice, CliMoveList, CliTransactionInfo, RpcAction, RpcResponse,
};
use crate::rest_utils::client::RPCClientIfc;
use crate::rest_utils::rpc_request_kind::RpcRequestKind;
use aptos_types::chain_id::ChainId;
use aptos_types::transaction::TransactionPayload;
use async_trait::async_trait;
use hex::ToHex;
use move_core_types::account_address::AccountAddress;
use reqwest::{Client, Response, Url};
use serde::de::DeserializeOwned;
use socrypto::Hash;
use tracing::debug;
use types::api::v1::FaucetStatus;
use types::api::{
    v1::{AccountInfo, MoveListQuery},
    ApiVersion,
};
use types::cli_utils::constants::{
    ACCEPT_TYPE, API_TAG_ACCOUNTS, API_TAG_TRANSACTIONS, API_TAG_VIEW, API_TAG_WALLET,
    ENDPOINT_ACCOUNT_MODULES, ENDPOINT_ACCOUNT_RESOURCES, ENDPOINT_CHAIN_ID,
    ENDPOINT_ESTIMATED_GAS_PRICE, ENDPOINT_FAUCET, ENDPOINT_SIMULATE_TRANSACTION,
    ENDPOINT_SUBMIT_TRANSACTION,
};
use types::cli_utils::fetch_chain_id_of_url_async;
use types::transactions::transaction::SupraTransaction;

pub struct RpcClientApi {
    pub client: Client,
    pub rpc_action: RpcAction,
    pub rpc_request_kind: RpcRequestKind,
    pub max_gas_amount: u64,
    pub gas_unit_price: u64,
}

impl RpcClientApi {
    /// Encloses both error message and status code in the final error result.
    async fn map_error_response(resp: Response) -> CliError {
        let error_status = resp
            .error_for_status_ref()
            .map(drop)
            .map_err(|e| e.to_string());
        let error_message = resp
            .text()
            .await
            .map_err(|e| CliError::GeneralError(format!("{error_status:?}, {e}")));
        match error_message {
            Ok(message) => {
                CliError::GeneralError(format!("Message: {message:?}, Cause: {error_status:?}",))
            }
            Err(e) => e,
        }
    }

    async fn map_response<T: DeserializeOwned>(resp: Response) -> Result<T, CliError> {
        if resp.status().is_success() {
            resp.json::<T>().await.map_err(CliError::RequestError)
        } else {
            Err(Self::map_error_response(resp).await)
        }
    }

    /// Map response data to expected data of type [T], if status is successful but API returned null,
    /// report it to be NotFound.
    ///
    /// Useful for the cases when REST API does not fail when requested data is not available, i.e.
    /// API provides optional successful response.
    async fn _map_response_optional<T: DeserializeOwned>(
        resp: Response,
        request_context: String,
    ) -> Result<T, CliError> {
        if resp.status().is_success() {
            resp.json::<Option<T>>()
                .await
                .map_err(CliError::RequestError)
                .and_then(|r| r.ok_or_else(|| CliError::NotFound(request_context)))
        } else {
            Err(Self::map_error_response(resp).await)
        }
    }

    async fn get_request(rpc_client: &Client, endpoint_url: Url) -> Result<Response, CliError> {
        rpc_client
            .get(endpoint_url)
            .header(reqwest::header::ACCEPT, ACCEPT_TYPE)
            .send()
            .await
            .map_err(CliError::RequestError)
    }

    async fn post_request(
        rpc_client: &Client,
        endpoint_url: Url,
        payload: String,
    ) -> Result<Response, CliError> {
        rpc_client
            .post(endpoint_url)
            .header(reqwest::header::CONTENT_TYPE, ACCEPT_TYPE)
            .header(reqwest::header::ACCEPT, ACCEPT_TYPE)
            .body(payload)
            .send()
            .await
            .map_err(|e| CliError::GeneralError(e.to_string()))
    }

    async fn fetch_chain_id_helper(
        rpc_client: &Client,
        endpoint_url: Url,
    ) -> Result<ChainId, CliError> {
        fetch_chain_id_of_url_async(rpc_client, endpoint_url)
            .await
            .map_err(CliError::Other)
    }

    async fn fetch_estimated_gas_price_helper(
        rpc_client: &Client,
        endpoint_url: Url,
    ) -> Result<CliEstimatedGasPrice, CliError> {
        Self::map_response::<CliEstimatedGasPrice>(
            Self::get_request(rpc_client, endpoint_url).await?,
        )
        .await
    }

    async fn fetch_account_information_helper(
        rpc_client: &Client,
        endpoint_url: Url,
    ) -> Result<AccountInfo, CliError> {
        let response = Self::get_request(rpc_client, endpoint_url)
            .await
            .map_err(|e| {
                CliError::GeneralError(format!("Failed to get account information: {}", e))
            })?;
        Self::map_response::<AccountInfo>(response).await
    }

    fn chain_id_url(&self, api_version: ApiVersion) -> Result<Url, CliError> {
        self.rpc_request_kind
            .url()
            .join(&format!(
                "rpc/{}/{}/{}",
                api_version, API_TAG_TRANSACTIONS, ENDPOINT_CHAIN_ID
            ))
            .map_err(CliError::UrlError)
    }

    fn estimate_gas_price_url(&self, api_version: ApiVersion) -> Result<Url, CliError> {
        self.rpc_request_kind
            .url()
            .join(&format!(
                "rpc/{}/{}/{}",
                api_version, API_TAG_TRANSACTIONS, ENDPOINT_ESTIMATED_GAS_PRICE
            ))
            .map_err(CliError::UrlError)
    }

    fn account_information_url(
        &self,
        api_version: ApiVersion,
        address: &AccountAddress,
    ) -> Result<Url, CliError> {
        self.rpc_request_kind
            .url()
            .join(&format!(
                "rpc/{}/{}/{}",
                api_version,
                API_TAG_ACCOUNTS,
                address.to_canonical_string()
            ))
            .map_err(CliError::UrlError)
    }

    /// Submits a transaction containing [TransactionPayload] to the specified RPC node.
    ///
    /// Returns the RPC node response to the submitted transaction and
    /// its expiration timestamp (expressed as the number of seconds from the Unix epoch).
    async fn signed_transaction_request_helper(
        &self,
        endpoint_url: Url,
        transaction_payload: TransactionPayload,
        api_version: ApiVersion,
    ) -> Result<(Response, u64), CliError> {
        match &self.rpc_request_kind {
            RpcRequestKind::SignedTransaction(signed_transaction_helper) => {
                let sender_account = signed_transaction_helper.txn_sender_address();
                let chain_id = signed_transaction_helper.chain_id();
                let account_information = Self::fetch_account_information_helper(
                    &self.client,
                    self.account_information_url(api_version, sender_account)?,
                )
                .await?;

                debug!("Chain_id = {}", chain_id);
                debug!("Sequence Number = {}", account_information.sequence_number);

                let signed_move_txn = signed_transaction_helper
                    .create_transaction(
                        transaction_payload,
                        chain_id.id(),
                        account_information.sequence_number,
                    )
                    .await?;

                debug!(
                    "SignedTransaction: {:?}",
                    serde_json::to_string(&signed_move_txn)
                );
                let expiration_timestamp_secs = signed_move_txn.expiration_timestamp_secs();

                let supra_txn = SupraTransaction::Move(signed_move_txn);

                Self::post_request(
                    &self.client,
                    endpoint_url,
                    serde_json::to_string(&supra_txn)?,
                )
                .await
                .map(|r| (r, expiration_timestamp_secs))
            }
            RpcRequestKind::PublicQuery(_) => Err(CliError::Unauthorized),
        }
    }
}

#[async_trait]
impl RPCClientIfc for RpcClientApi {
    fn new_action(
        client: Client,
        rpc_action: RpcAction,
        rpc_request_kind: RpcRequestKind,
    ) -> Result<Self, CliError> {
        let mut max_gas_amount = 0;
        let mut gas_unit_price = 0;
        if let RpcRequestKind::SignedTransaction(ref t) = rpc_request_kind {
            eprintln!("account: {}", t.txn_sender_address().to_canonical_string());
            max_gas_amount = *t.max_gas_amount();
            gas_unit_price = *t.gas_unit_price();
        }
        Ok(Self {
            client,
            rpc_action,
            rpc_request_kind,
            max_gas_amount,
            gas_unit_price,
        })
    }

    fn endpoint_url(&self) -> Result<Url, CliError> {
        let endpoint_url = match &self.rpc_action {
            RpcAction::ChainID(api_version) => self.chain_id_url(*api_version),
            RpcAction::EstimatedGasPrice(api_version) => self.estimate_gas_price_url(*api_version),
            RpcAction::AccountInformation(api_version, address) => {
                self.account_information_url(*api_version, address)
            }
            RpcAction::SimulateTransaction(api_version, ..) => self
                .rpc_request_kind
                .url()
                .join(&format!(
                    "rpc/{}/{}/{}",
                    api_version, API_TAG_TRANSACTIONS, ENDPOINT_SIMULATE_TRANSACTION
                ))
                .map_err(CliError::UrlError),
            RpcAction::SendTransaction(api_version, ..)
            | RpcAction::SendSignedTransaction(api_version, ..) => self
                .rpc_request_kind
                .url()
                .join(&format!(
                    "rpc/{}/{}/{}",
                    api_version, API_TAG_TRANSACTIONS, ENDPOINT_SUBMIT_TRANSACTION
                ))
                .map_err(CliError::UrlError),
            RpcAction::RunViewFunction(api_version, ..) => self
                .rpc_request_kind
                .url()
                .join(&format!("rpc/{}/{}", api_version, API_TAG_VIEW))
                .map_err(CliError::UrlError),
            RpcAction::ListAccountAssets(api_version, account, query) => self
                .rpc_request_kind
                .url()
                .join(&format!(
                    "rpc/{}/{}/{}/{}",
                    api_version,
                    API_TAG_ACCOUNTS,
                    account.to_canonical_string(),
                    match query {
                        MoveListQuery::Modules => ENDPOINT_ACCOUNT_MODULES,
                        MoveListQuery::Resources => ENDPOINT_ACCOUNT_RESOURCES,
                    }
                ))
                .map_err(CliError::UrlError),
            RpcAction::TransactionInfo(api_version, hash) => {
                let mut transaction_info_url = self
                    .rpc_request_kind
                    .url()
                    .join(&format!("rpc/{}/{}", api_version, API_TAG_TRANSACTIONS))?;
                match transaction_info_url.path_segments_mut() {
                    Ok(mut segments) => segments.push(&hash.encode_hex::<String>()),
                    Err(_) => return Err(CliError::GeneralError("Invalid Base url".into())),
                };
                Ok(transaction_info_url)
            }
            RpcAction::AccountResource(api_version, account, resource_type) => self
                .rpc_request_kind
                .url()
                .join(&format!(
                    "rpc/{}/{}/{}/{}/{}",
                    api_version,
                    API_TAG_ACCOUNTS,
                    account.to_canonical_string(),
                    ENDPOINT_ACCOUNT_RESOURCES,
                    resource_type
                ))
                .map_err(CliError::UrlError),
            RpcAction::AccountModule(api_version, account, module_id) => self
                .rpc_request_kind
                .url()
                .join(&format!(
                    "rpc/{}/{}/{}/{}/{}",
                    api_version,
                    API_TAG_ACCOUNTS,
                    account.to_canonical_string(),
                    ENDPOINT_ACCOUNT_MODULES,
                    module_id
                ))
                .map_err(CliError::UrlError),
            RpcAction::FundWithFaucet(api_version, account, rpc_url) => {
                let mut faucet_url = rpc_url.join(&format!(
                    "rpc/{}/{}/{}",
                    api_version, API_TAG_WALLET, ENDPOINT_FAUCET
                ))?;
                match faucet_url.path_segments_mut() {
                    Ok(mut segments) => segments.push(&account.to_canonical_string()),
                    Err(_) => return Err(CliError::GeneralError("Invalid Base url".into())),
                };
                Ok(faucet_url)
            }
        }?;
        debug!("URL: {}", endpoint_url);
        Ok(endpoint_url)
    }

    /// Executes the [RpcAction].
    async fn execute_action(self) -> Result<RpcResponse, CliError> {
        match self.rpc_action {
            RpcAction::ChainID(..) => {
                Self::fetch_chain_id_helper(&self.client, self.endpoint_url()?)
                    .await
                    .map(RpcResponse::ChainID)
            }
            RpcAction::AccountInformation(..) => {
                Self::fetch_account_information_helper(&self.client, self.endpoint_url()?)
                    .await
                    .map(RpcResponse::AccountInformation)
            }
            RpcAction::EstimatedGasPrice(..) => {
                Self::fetch_estimated_gas_price_helper(&self.client, self.endpoint_url()?)
                    .await
                    .map(RpcResponse::EstimatedGasPrice)
            }
            RpcAction::SimulateTransaction(api_version, ref transaction_payload) => {
                let endpoint_url = self.endpoint_url()?;

                let (response, expiration_timestamp_secs) = self
                    .signed_transaction_request_helper(
                        endpoint_url,
                        transaction_payload.clone(),
                        api_version,
                    )
                    .await?;

                Self::map_response::<CliTransactionInfo>(response)
                    .await
                    .map(|tx_info| RpcResponse::SimulateTransaction {
                        info: tx_info,
                        expiration_timestamp_secs,
                    })
            }
            RpcAction::SendTransaction(api_version, ref transaction_payload) => {
                let endpoint_url = self.endpoint_url()?;
                let (response, expiration_timestamp_secs) = self
                    .signed_transaction_request_helper(
                        endpoint_url,
                        transaction_payload.clone(),
                        api_version,
                    )
                    .await?;
                Self::map_response::<Hash>(response).await.map(|tx_hash| {
                    RpcResponse::SendTransaction {
                        hash: tx_hash,
                        expiration_timestamp_secs,
                    }
                })
            }
            RpcAction::SendSignedTransaction(_, ref signed_transaction) => {
                let endpoint_url = self.endpoint_url()?;
                let expiration_timestamp_secs = signed_transaction.expiration_timestamp_secs();
                let txn =
                    serde_json::to_string(&SupraTransaction::Move(signed_transaction.clone()))?;
                Self::map_response::<Hash>(
                    Self::post_request(&self.client, endpoint_url, txn).await?,
                )
                .await
                .map(|tx_hash| RpcResponse::SendSignedTransaction {
                    hash: tx_hash,
                    expiration_timestamp_secs,
                })
            }
            RpcAction::RunViewFunction(api_version, view_req) => {
                let endpoint_url = self
                    .rpc_request_kind
                    .url()
                    .join(&format!("rpc/{}/{}", api_version, API_TAG_VIEW))?;
                let view_request_payload = serde_json::to_string(&view_req)?;
                Self::post_request(&self.client, endpoint_url, view_request_payload)
                    .await
                    .map(RpcResponse::RunViewFunction)
            }
            RpcAction::ListAccountAssets(..) => Self::map_response::<CliMoveList>(
                Self::get_request(&self.client, self.endpoint_url()?).await?,
            )
            .await
            .map(RpcResponse::ListAccountAssets),
            RpcAction::TransactionInfo(..) => Self::map_response::<CliTransactionInfo>(
                Self::get_request(&self.client, self.endpoint_url()?).await?,
            )
            .await
            .map(RpcResponse::TransactionInfo),
            RpcAction::AccountResource(..) => Self::get_request(&self.client, self.endpoint_url()?)
                .await
                .map(RpcResponse::AccountResource),
            RpcAction::AccountModule(..) => Self::get_request(&self.client, self.endpoint_url()?)
                .await
                .map(RpcResponse::AccountModule),
            RpcAction::FundWithFaucet(..) => Self::map_response::<FaucetStatus>(
                Self::get_request(&self.client, self.endpoint_url()?).await?,
            )
            .await
            .map(RpcResponse::FundWithFaucet),
        }
    }
}
