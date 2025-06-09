use crate::common::error::CliError;
use crate::rest_utils::action::{CliTransactionInfo, RpcAction, RpcResponse};
use crate::rest_utils::rpc_request_kind::RpcRequestKind;
use async_trait::async_trait;
use reqwest::{Client, Url};
use socrypto::Hash;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time;
use tracing::debug;
use transactions::TxExecutionStatus;
use types::api::ApiVersion;
use types::cli_utils::HTTP_CLIENT;
use types::console_progressbar_template::console_spinner;

pub mod supra_rpc_client;

#[async_trait]
pub trait RPCClientIfc: Sized {
    /// Additional margin to wait beyond the transaction expiration period for its finalization.
    const TX_FINALIZATION_MARGIN_SECS: u64 = 10;

    /// It uses global shared http client to create a new instance of the client so
    /// that it can be reused across multiple requests.
    fn new_with_action(
        rpc_action: RpcAction,
        rpc_request_kind: RpcRequestKind,
    ) -> Result<Self, CliError> {
        Self::new_action(HTTP_CLIENT.clone(), rpc_action, rpc_request_kind)
    }

    fn new_action(
        client: Client,
        rpc_action: RpcAction,
        rpc_request_kind: RpcRequestKind,
    ) -> Result<Self, CliError>;

    fn endpoint_url(&self) -> Result<Url, CliError>;

    async fn execute_action(self) -> Result<RpcResponse, CliError>;

    /// Given a transaction [Hash], wait for the transaction to be finalized.
    ///
    /// Tx is expected to be finalized within its expiration timestamp + a safety margin [Self::TX_FINALIZATION_MARGIN_SECS],
    /// otherwise tx is going to be scraped by the consensus.
    async fn await_finalized(
        api_version: ApiVersion,
        tx_hash: Hash,
        tx_expiration_timestamp_secs: u64,
        rpc_request_kind: RpcRequestKind,
        retry_delay: Duration,
    ) -> Result<CliTransactionInfo, CliError> {
        debug!("Waiting for tx {tx_hash} to get finalized");
        let spinner = console_spinner(
            Duration::from_secs(1),
            "Waiting for the transaction to finalize...".to_string(),
        );

        loop {
            let action = RpcAction::TransactionInfo(api_version, tx_hash);

            match Self::new_with_action(action.clone(), rpc_request_kind.clone())?
                .execute_action()
                .await
            {
                Ok(RpcResponse::TransactionInfo(tx_info))
                    if matches!(
                        tx_info.status(),
                        TxExecutionStatus::Success
                            | TxExecutionStatus::Fail
                            | TxExecutionStatus::Invalid
                    ) =>
                {
                    debug!(
                        "Tx {tx_hash} has been finalized with status {}",
                        tx_info.status()
                    );
                    return Ok(tx_info);
                }
                Ok(RpcResponse::TransactionInfo(tx_info)) => {
                    debug!(
                        "Tx {tx_hash} has not yet been finalized. Status: {}. Waiting",
                        tx_info.status()
                    );
                    spinner.set_message(format!(
                        "Still waiting... Current status: {}",
                        tx_info.status()
                    ));
                    time::sleep(retry_delay).await;
                }
                Ok(unexpected_response) => {
                    return Err(CliError::UnexpectedResponse(
                        format!("{action:?}"),
                        unexpected_response,
                    ))
                }
                Err(e) => {
                    spinner.set_message(format!("Error occurred: {:?}", e));
                }
            }

            let since_the_epoch = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();

            debug!("Timeout check:\n{since_the_epoch}\n{tx_expiration_timestamp_secs}\n1 ?> 2 + {} = {}", Self::TX_FINALIZATION_MARGIN_SECS, since_the_epoch > tx_expiration_timestamp_secs + Self::TX_FINALIZATION_MARGIN_SECS);

            if since_the_epoch > tx_expiration_timestamp_secs + Self::TX_FINALIZATION_MARGIN_SECS {
                break;
            }
        }

        Err(CliError::Aborted(
            format!("Tx {tx_hash} was not finalized within its expiration timestamp"),
            format!("{:?}", RpcAction::TransactionInfo(api_version, tx_hash)),
        ))
    }

    /// Given a transaction [Hash], wait for the transaction to be finalized with [TxExecutionStatus::Success].
    ///
    /// Tx is expected to be finalized within its expiration timestamp + a safety margin [Self::TX_FINALIZATION_MARGIN_SECS],
    /// otherwise tx is going to be scraped by the consensus.
    async fn await_finalized_with_success(
        api_version: ApiVersion,
        tx_hash: Hash,
        tx_expiration_timestamp_secs: u64,
        rpc_request_kind: RpcRequestKind,
        retry_delay: Duration,
    ) -> Result<(), CliError> {
        let tx_info = Self::await_finalized(
            api_version,
            tx_hash,
            tx_expiration_timestamp_secs,
            rpc_request_kind,
            retry_delay,
        )
        .await?;

        if !matches!(tx_info.status(), TxExecutionStatus::Success) {
            return Err(CliError::Aborted(
                format!(
                    "Tx {tx_hash} was finalized with status different from Success. Status: {}",
                    tx_info.status()
                ),
                format!("{:?}", RpcAction::TransactionInfo(api_version, tx_hash)),
            ));
        }

        Ok(())
    }
}
