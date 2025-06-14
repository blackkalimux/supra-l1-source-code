use crate::transactions::dispatcher::{TTransactionConsumer, TransactionDispatchError};
use aptos_types::transaction::automation::AutomationTaskMetaData;
use archive::db::{ArchiveDb, ArchiveResult};
use async_trait::async_trait;
use committees::TBlockHeader;
use index_storage_ifc::ArchiveWrite;
use mempool::tx_index::{HashIndex, TxIndexKey, TxIndexKind};
use mempool::{Backlog, BacklogEntry, TCurrentSequenceNumberProvider};
use smr_timestamp::SmrTimestamp;
use socrypto::{Digest, Hash};
use soserde::SizeInBytes;
use std::sync::{Arc, RwLock};
use tracing::debug;
use transactions::{SequenceNumber, TTransactionHeaderProperties, TxExecutionStatus};
use types::executed_block::ExecutedBlock;
use types::settings::backlog;
use types::transactions::smr_transaction::SmrTransaction;

#[derive(Clone)]
pub struct RpcStorage<T> {
    /// Persistent storage for consensus-processed data.
    archive: ArchiveDb,
    /// Transient storage for data submitted to the consensus.
    backlog: Arc<RwLock<Backlog<T, RpcBacklogEntry>>>,
}

/// Backlog entries for the RPC nodes.
/// RPC nodes need to always maintain the full tx details regardless
/// if [SmrTransaction] can or cannot be scheduled by a validator.
#[derive(Debug)]
struct RpcBacklogEntry(SmrTransaction);

impl SizeInBytes for RpcBacklogEntry {
    fn size_in_bytes(&self) -> usize {
        // [SmrTransaction] memoizes own size.
        self.0.size_in_bytes()
    }
}

impl BacklogEntry for RpcBacklogEntry {
    fn schedule(
        tx: SmrTransaction,
        _expected_sequence_number: SequenceNumber,
    ) -> (Self, Option<SmrTransaction>)
    where
        Self: Sized,
    {
        (Self(tx), None)
    }

    /// RPC does not schedule transactions, thus it does not remove anything from [Backlog].
    fn take_transaction(&mut self) -> Option<SmrTransaction> {
        None
    }

    fn transaction(&self) -> &SmrTransaction {
        &self.0
    }

    fn timestamp(&self) -> SmrTimestamp {
        self.0.expiration_timestamp()
    }
}

impl<T: TCurrentSequenceNumberProvider> RpcStorage<T> {
    pub fn new(
        archive: ArchiveDb,
        current_sequence_number_provider: T,
        backlog_parameters: &backlog::Parameters,
    ) -> Self {
        let mut backlog =
            Backlog::new(current_sequence_number_provider, backlog_parameters.clone());

        backlog
            .add_index_1to1(Box::new(HashIndex::new()))
            .expect("Must be the first index");

        Self {
            archive,
            backlog: Arc::new(RwLock::new(backlog)),
        }
    }

    pub fn archive(&self) -> &ArchiveDb {
        &self.archive
    }

    /// Get [SmrTransaction] corresponding to [Hash].
    /// This method first tries the inner [Backlog] and then the inner [ArchiveDb].
    pub fn get_transaction_by_hash(&self, hash: Hash) -> ArchiveResult<Option<SmrTransaction>> {
        let tx = self
            .backlog
            .read()
            .expect("RPC Backlog must be lockable")
            .get_by_index(&TxIndexKind::Hash, &TxIndexKey::Hash(hash))
            .map(|entry| entry.0.clone());

        if let Some(transaction) = &tx {
            // `unwrap` is safe because the previous condition verified that there is a value.
            let hash = transaction.digest();
            debug!("Tx {hash} is found in the RPC backlog");

            // Debug-verify the invariant that the tx from the backlog may not be in the archive.
            // Assume that if the archive returns an error, the tx of interest is not present there.
            debug_assert!(
                self.archive
                    .reader()
                    .get_transaction(hash)
                    .map(|tx| tx.is_none())
                    .unwrap_or(true),
                "Debug assertion violated: Tx found in Backlog also found in Archive."
            );

            return Ok(tx);
        }

        self.archive.reader().get_transaction(hash)
    }

    /// Get [TxExecutionStatus] corresponding to [Hash].
    /// This method first tries the inner [Backlog] and then the inner [ArchiveDb].
    pub fn get_transaction_status(&self, hash: Hash) -> ArchiveResult<Option<TxExecutionStatus>> {
        let status = self
            .backlog
            .read()
            .expect("RPC Backlog must be lockable")
            .get_by_index(&TxIndexKind::Hash, &TxIndexKey::Hash(hash))
            .map(|_entry| {
                // If a tx is in the backlog, it is by definition [TxExecutionStatus::Pending].
                TxExecutionStatus::Pending
            });
        if status.is_some() {
            return Ok(status);
        }

        self.archive.reader().get_transaction_status(hash)
    }

    /// Expire forcefully transactions prior the given [SmrTimestamp].
    pub fn expire(&self, timestamp: SmrTimestamp) {
        debug!("Expiring txs before {timestamp}");

        self.backlog
            .write()
            .expect("RPC Backlog must be lockable")
            .expire(timestamp);
    }

    /// Returns the max tx TTL for transaction acceptance in [Backlog].
    pub fn get_max_tx_ttl_secs(&self) -> u64 {
        self.backlog
            .read()
            .expect("RPC Backlog must be lockable")
            .parameters()
            .max_backlog_transaction_time_to_live_seconds
    }
}

#[async_trait]
impl<T: TCurrentSequenceNumberProvider + Send + Sync> ArchiveWrite for RpcStorage<T> {
    fn insert_executed_block(&self, block: ExecutedBlock) {
        // Expire all transactions that are scheduled earlier than the executed block.
        // Some of them may be included in the block, some may be dropped.
        debug!(
            "Block {} (height {}) observed.",
            block.hash(),
            block.height()
        );

        // Expire old transactions.
        self.expire(block.timestamp());

        // Remove txs contained in the block from the backlog.
        // Such txs have been processed by consensus and are not transient anymore.
        {
            let mut guard = self.backlog.write().expect("RPC Backlog must be lockable");

            for (tx, _) in block.transactions_and_outputs_iter() {
                // RPC does not schedule transaction, we can safely ignore whatever returned here.
                let _ = guard.finalize(tx);
            }
        }

        self.archive.insert_executed_block(block);
    }

    fn insert_automation_task_data(&self, tasks: &[AutomationTaskMetaData]) -> anyhow::Result<()> {
        self.archive.insert_automation_task_data(tasks)
    }
}

#[async_trait]
impl<T: TCurrentSequenceNumberProvider + Send + Sync> TTransactionConsumer for RpcStorage<T> {
    async fn consume(&self, txn: SmrTransaction) -> Result<(), TransactionDispatchError> {
        debug!(
            "RPC storage consumes a tx; hash {}; expiration: {}",
            txn.digest(),
            txn.expiration_timestamp()
        );

        // RPC node cannot schedule a tx, so ignore the returned option if any.
        let _ = self
            .backlog
            .write()
            .expect("RPC Backlog must be lockable")
            .insert(txn)
            .map_err(|err| TransactionDispatchError::ConsumptionError(err.to_string()))?;

        Ok(())
    }
}
