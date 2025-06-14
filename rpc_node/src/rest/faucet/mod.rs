mod faucet_impl;
mod minter_account;

pub use faucet_impl::Faucet;
pub use minter_account::MinterAccount;
use std::convert::identity;

use crate::rest::faucet::minter_account::MinterAccountError;
use crate::transactions::dispatcher::{TransactionDispatchError, TransactionDispatcher};
use crate::transactions::tx_execution_notification::TxExecutionNotification;
use aptos_crypto::ed25519::{
    Ed25519PrivateKey as AptosPrivateKey, Ed25519PublicKey as AptosPublicKey,
};
use aptos_types::account_config::AccountResource;
use aptos_types::chain_id::ChainId;
use aptos_types::transaction::{RawTransaction, TransactionPayload};
use archive::error::ArchiveError;
use execution::MoveStore;
use move_core_types::account_address::AccountAddress;
use smr_timestamp::SmrTimestamp;
use socrypto::Hash;
use std::time::Duration;
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio::time;
use tracing::trace;
use transactions::TxExecutionStatus;
use types::transactions::smr_transaction::SmrTransaction;

// TODO: The below two values should not be hard-coded once dynamic gas pricing is enabled properly.
const FAUCET_MAX_GAS: u64 = 500000;
const FAUCET_GAS_UNIT_PRICE: u64 = 100;
/// The number of seconds that a faucet transaction remains valid for after its creation.
const FAUCET_TRANSACTION_EXPIRY_SECONDS: u64 = 60;

/// Builds a transaction with a given `tx_payload` with current sequence number for given `sender_account`.
/// Spawns a task submitting and waiting for the tx to be assigned any [TxExecutionStatus] except for [TxExecutionStatus::Unexecuted].
/// Notice it will wait indefinitely, it is the caller responsibility to time bound the task.
/// Returns:
/// - sequence number the tx is built with,
/// - the tx [Hash], and
/// - the task [JoinHandle].
#[allow(clippy::too_many_arguments)]
async fn issue_tx(
    tx_descriptor: &str,
    tx_payload: TransactionPayload,
    sender_account: AccountAddress,
    sender_aptos_private_key: &AptosPrivateKey,
    sender_aptos_public_key: &AptosPublicKey,
    tx_sender: &TransactionDispatcher,
    move_store: &MoveStore,
    tx_execution_notification: &TxExecutionNotification,
) -> Result<(u64, Hash, JoinHandle<Result<Hash, IssuingError>>), TransactionDispatchError> {
    let sequence_number = move_store
        .read_resource::<AccountResource>(&sender_account)
        .expect("missing sender account")
        .sequence_number();
    let chain_id =
        move_store
            .chain_id()
            .map(|res| res.chain_id())
            .ok_or(TransactionDispatchError::Fatal(
                "Failed to  obtain ChainId from on chain resource".to_string(),
            ))?;
    let tx = make_smr_move_tx(
        tx_payload,
        sequence_number,
        chain_id,
        sender_account,
        sender_aptos_private_key,
        sender_aptos_public_key,
    );

    trace!("[Tx][Issuing] {tx_descriptor}\nSender: {sender_account}\nSequence number {sequence_number}");

    let tx_execution_notification = tx_execution_notification.clone();

    let tx_hash = tx_sender.send(tx).await?;
    trace!("[Tx {tx_hash}][Submission acknowledged] {tx_descriptor}");

    let issue_handle = tokio::spawn(async move {
        match tx_execution_notification.wait(tx_hash).await? {
            TxExecutionStatus::Success => Ok(tx_hash),
            status @ TxExecutionStatus::Fail | status @ TxExecutionStatus::Invalid => {
                // Tx has been submitted, however failed.
                // Retry.
                Err(IssuingError::BadTransaction { tx_hash, status })
            }
            TxExecutionStatus::Pending | TxExecutionStatus::PendingAfterExecution => {
                unreachable!("Notification is only emitted when status changes from Pending")
            }
        }
    });

    Ok((sequence_number, tx_hash, issue_handle))
}

#[derive(Error, Debug)]
pub enum IssuingError {
    #[error("Tx `{tx_hash}` has unsuccessful status {status}")]
    BadTransaction {
        tx_hash: Hash,
        status: TxExecutionStatus,
    },
    #[error(transparent)]
    ArchiveError(#[from] ArchiveError),
}

/// Continuously builds transactions with a given `tx_payload` by changing the sequence number
/// and submits it until it is executed and included in a block.
#[allow(clippy::too_many_arguments)]
async fn ensure_tx(
    tx_descriptor: &str,
    max_wait_time: Duration,
    tx_payload: TransactionPayload,
    sender_account: AccountAddress,
    sender_aptos_private_key: &AptosPrivateKey,
    sender_aptos_public_key: &AptosPublicKey,
    tx_sender: &TransactionDispatcher,
    move_store: &MoveStore,
    tx_execution_notification: &TxExecutionNotification,
) -> Result<Hash, MinterAccountError> {
    trace!("[Tx][Ensuring] {tx_descriptor}\nSender: {sender_account}");

    time::timeout(max_wait_time, async move {
        loop {
            let (sequence_number, tx_hash, tx_handle) = issue_tx(
                tx_descriptor,
                tx_payload.clone(),
                sender_account,
                sender_aptos_private_key,
                sender_aptos_public_key,
                tx_sender,
                move_store,
                tx_execution_notification,
            ).await?;

            trace!("[Tx {tx_hash}][Ensuring] {tx_descriptor}\nSender: {sender_account}");

            match tx_handle.await? {
                Ok(tx_hash) => {
                    trace!("[Tx {tx_hash}][Ensured] {tx_descriptor}");
                    return Ok::<_, MinterAccountError>(tx_hash);
                }
                Err(IssuingError::BadTransaction { tx_hash, .. }) => {
                    trace!("[Tx {tx_hash}][Failure] {tx_descriptor}\nsequence number: {sequence_number}\nRetry with actual sequence number");
                    // Tx has been submitted, however failed.
                    // We assume the tx may only fail due to the sequence number,
                    // because functions `issue`/`ensure` are private
                    // and thus their arguments are expected to be correctly crafted.
                    //
                    // Retry.
                }
                Err(err) => {
                    // If the error is not [IssuingError::BadTransaction], forward the error.
                    return Err(MinterAccountError::Issuing {
                        inner: err,
                        account: sender_account,
                        stage: tx_descriptor.to_string(),
                    });
                }
            }
        }
    })
        .await
        .map_err(|_elapsed| MinterAccountError::WaitBudgetExhausted {
            tx_descriptor: tx_descriptor.to_string(),
            budget: max_wait_time,
        })
        .and_then(identity)
}

/// Creates an [SmrTransaction] of subtype [SmrTransactionProtocol::Move].
/// This function **cannot** be used to create [SmrTransaction]'s of a different subtype.
fn make_smr_move_tx(
    tx_payload: TransactionPayload,
    sequence_number: u64,
    chain_id: ChainId,
    sender_account: AccountAddress,
    sender_aptos_private_key: &AptosPrivateKey,
    sender_aptos_public_key: &AptosPublicKey,
) -> SmrTransaction {
    // Create a raw transaction.
    // As in crates/test_utils/src/execution_utils.rs :: create_transactions
    let raw_tx = RawTransaction::new(
        sender_account,
        sequence_number,
        tx_payload,
        FAUCET_MAX_GAS,
        FAUCET_GAS_UNIT_PRICE,
        SmrTimestamp::now().as_duration().as_secs() + FAUCET_TRANSACTION_EXPIRY_SECONDS,
        chain_id,
    );

    // Create a move transaction.
    // As in rpc_node/src/rest/wallet.rs :: to_smr_txn
    raw_tx
        .sign(sender_aptos_private_key, sender_aptos_public_key.clone())
        .expect("Signing with Aptos key must not fail") // SignatureCheckedTransaction
        .into_inner() // SignedTransaction
        .into() // SmrTransaction
}
