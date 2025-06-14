use archive::db::ArchiveResult;
use archive::error::ArchiveError;
use archive::reader::ArchiveReader;
use socrypto::Hash;
use std::time::Duration;
use tokio::time;
use tracing::trace;
use transactions::TxExecutionStatus;

#[derive(Clone)]
/// A utility primitive enabling to asynchronously check transaction status.
///
/// Internally, it periodically polls the blockchain for the transaction execution status
/// based on a given transaction hash.
pub struct TxExecutionNotification {
    interval: Duration,
    archive_reader: ArchiveReader,
}

impl TxExecutionNotification {
    /// Create [TxExecutionNotification] by specifying [TxStatusStore] and check periodicity.
    pub fn new(interval: Duration, archive_reader: ArchiveReader) -> Self {
        Self {
            interval,
            archive_reader,
        }
    }

    /// Wait for a transaction with `tx_hash` to be included in blockchain.
    ///
    /// Returns Ok(status), where status != TxExecutionStatus::Unexecuted.
    pub async fn wait(&self, tx_hash: Hash) -> ArchiveResult<TxExecutionStatus> {
        loop {
            // Give some time for tx to be included.
            time::sleep(self.interval).await;

            let status = self.archive_reader.get_transaction_status(tx_hash)?;
            match status {
                Some(TxExecutionStatus::Pending | TxExecutionStatus::PendingAfterExecution) => {
                    trace!("Tx `{tx_hash}` is not yet finalized")
                }
                None => trace!("Tx `{tx_hash}` is not yet included"),
                Some(status) => return Ok::<_, ArchiveError>(status),
            }
        }
    }
}
