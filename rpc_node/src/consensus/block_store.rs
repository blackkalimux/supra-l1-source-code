use committees::{
    BlockHeader, BlockInfo, CertifiedBlock, GenesisStateProvider, Height, TBlockHeader, TSmrBlock,
};
use core::fmt::{Debug, Formatter};
use genesis::Genesis;
use lifecycle::{TView, View};
use moonshot_storage_ifc::MoonshotStorageWriter;
use rocksstore::chain_storage::block_store::{ChainStore, StoreError};
use rocksstore::chain_storage::consensus_last_state::{LastStateItemKey, LastStateItemValue};
use rpc::FullBlock;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use thiserror::Error;
use tracing::{debug, error, info, warn};
use types::batches::batch::PackedSmrBatch;
use types::executed_block::ExecutedBlock;
use types::transactions::block_metadata::BlockMetadata;

#[derive(Debug, Error)]
pub enum BlockStoreError {
    #[error("PersistentStore: {0}")]
    PersistentStore(#[from] StoreError),
    #[error("StorageWriter: {0}")]
    StorageWriter(#[from] Box<dyn StdError>),
    #[error("InvalidOperation: {0}")]
    InvalidOperation(String),
    #[error("InvalidState: {0}")]
    InvalidState(String),
    #[error("Fatal: {0}")]
    Fatal(String),
}

/// Trait describing the interface of the block store that can be utilized in scope of [ChainStateAssembler].
pub trait TBlockStore {
    /// Initializes block store state with genesis state if no data is available.
    /// Returns an error if any inconsistency has been identified.
    fn initialize(
        &mut self,
        genesis_state_provider: &impl GenesisStateProvider,
    ) -> Result<(), BlockStoreError>;

    /// Persists full block on to the underlying permanent storage.
    fn persist(&mut self, full_block: FullBlock) -> Result<(), BlockStoreError>;

    /// Caches the block reference to be executed in future when it is ready to be.
    fn cache(&mut self, certified_block: CertifiedBlock) -> Result<(), BlockStoreError>;

    /// Prunes from the cache the [CertifiedBlock] matching input height if any.
    /// If cache contains a memo block data is tried to retrieve from store.
    /// Existing entry in the cache is pruned.
    fn take_next_ready_block_by_height(
        &mut self,
        height: Height,
    ) -> Result<Option<CertifiedBlock>, BlockStoreError>;

    /// Updates internal state relative to input height.
    fn retain(&mut self, lower_bound: &Height);

    /// Returns last executed block from persisted state if any.
    fn last_executed_block(&self) -> Result<Option<ExecutedBlock>, BlockStoreError>;

    /// Returns last committed block from persisted state if any.
    fn last_committed_block(&self) -> Result<Option<CertifiedBlock>, BlockStoreError>;

    /// Updates persisted state with last-committed block info.
    fn update_last_committed_block_info(
        &mut self,
        block: &CertifiedBlock,
    ) -> Result<(), BlockStoreError>;

    /// Updates persisted state with epoch boundary block info.
    fn update_epoch_boundary_block_info(
        &mut self,
        block: &CertifiedBlock,
    ) -> Result<(), BlockStoreError>;

    /// Checks whether there is an entry with input height.
    fn has_entry(&self, height: Height) -> bool;
}

/// Describes the state of the pending execution block in scope of [BlockStore].
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum BlockReference {
    /// Full [CertifiedBlock] in pending execution.
    Block(CertifiedBlock),
    /// [BlockInfo] of the block pending execution.
    Memo(BlockInfo),
    /// [BlockInfo] of the block already scheduled to be executed execution.
    Scheduled(BlockInfo),
}

impl TView for BlockReference {
    fn view(&self) -> &View {
        match self {
            BlockReference::Block(block) => block.view(),
            BlockReference::Memo(info) | BlockReference::Scheduled(info) => info.view(),
        }
    }
}

impl TBlockHeader for BlockReference {
    fn header(&self) -> &BlockHeader {
        match self {
            BlockReference::Block(block) => block.header(),
            BlockReference::Memo(info) | BlockReference::Scheduled(info) => info.header(),
        }
    }
}

/// Provides means to persist the valid blocks that are executed or ready to be executed,
/// and keep an in-memory cache for the blocks that are pending the execution.
/// Internal cache of the pending blocks is a mapping of the block height to [BlockReference].
///
/// The cache is split into 2 buckets, one for the block references enclosing full [CertifiedBlock] data,
/// the other for the references enclosing [BlockInfo] as memo of the pending block.
/// Full block bucket boundaries are
/// `(last_committed_block_height, last_committed_block_height + max_full_block_bucket_size]`.
pub struct BlockStore {
    /// Reference to the persistent storage of [ChainStore] type.
    store: ChainStore,
    /// Internal cache of the block references waiting for execution.
    pending_execution: BTreeMap<Height, Option<BlockReference>>,
    /// Maximum number of the [BlockReference]s enclosing full [CertifiedBlock] in the cache.
    certified_block_cache_bucket_size: Height,
    /// Height of the block stored as last committed block.
    last_committed_block_height: Height,
}

impl Clone for BlockStore {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            pending_execution: Default::default(),
            certified_block_cache_bucket_size: self.certified_block_cache_bucket_size,
            last_committed_block_height: self.last_committed_block_height,
        }
    }
}

impl Debug for BlockStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BlockStore")
            .field("pending_execution", &self.pending_execution)
            .field(
                "last_committed_block_height",
                &self.last_committed_block_height,
            )
            .finish()
    }
}

impl BlockStore {
    pub fn new(store: ChainStore, certified_block_cache_bucket_size: u64) -> Self {
        Self {
            store,
            pending_execution: Default::default(),
            certified_block_cache_bucket_size,
            last_committed_block_height: Default::default(),
        }
    }

    /// Checks whether the block with the provided height can be cached.
    /// If caching is not possible an error will be returned.
    /// Cases when the block with the provided height can not be cached:
    ///     - If the block height is less or equal to the last committed block height
    ///     - If an entry with [BlockReference::Scheduled] entry is available.
    /// For the rest of the cases of entry existence warning will be reported,
    /// but caching will not be prevented.
    fn ensure_can_be_cached(&self, height: &Height) -> Result<(), BlockStoreError> {
        if height <= &self.last_committed_block_height {
            return Err(BlockStoreError::InvalidOperation(format!(
                "Trying to cache block with height({height}) lower than last committed block({})",
                self.last_committed_block_height
            )));
        }
        match self.pending_execution.get(height) {
            Some(Some(BlockReference::Scheduled(_))) => {
                Err(BlockStoreError::InvalidOperation(format!(
                    "Trying to cache block with height({height}) which is already scheduled to be executed."
                )))
            }
            Some(entry) => {
                warn!("Trying to cache a block with height {height} for which there is already an entry: {entry:?}");
                Ok(())
            }
            _ => Ok(())
        }
    }
}

impl TBlockStore for BlockStore {
    /// Initializes consensus last state with genesis data if non is available.
    /// TODO: Move this logic to [ChainStore] itself, and updated all required last state items.
    ///       This way no such custom code will be written here and in [Consensus::Core] as well.
    ///       Also extra validations can be enabled for the store-state such as committed_block >=
    ///       executed_block and many other things as well.
    fn initialize(
        &mut self,
        genesis_state_provider: &impl GenesisStateProvider,
    ) -> Result<(), BlockStoreError> {
        let maybe_last_committed_block = self.last_committed_block()?;
        let maybe_last_executed_block = self.last_executed_block()?;
        if maybe_last_committed_block.is_none() && maybe_last_executed_block.is_none() {
            info!("Initializes last committed and executed block info with genesis info");
            let genesis_block = genesis_state_provider.certified_block();
            self.update_last_committed_block_info(&genesis_block)?;
            let block_metadata = BlockMetadata::genesis(genesis_block.clone());
            let genesis_executed_block =
                ExecutedBlock::new(genesis_block, vec![], vec![], block_metadata)
                    .finalize()
                    .ok_or_else(|| {
                        BlockStoreError::Fatal(
                            "Failed to finalize genesis executed block".to_string(),
                        )
                    })?;
            let last_exec_block =
                LastStateItemValue::ExecutedBlock(Cow::Borrowed(&genesis_executed_block));
            self.store
                .last_state
                .write_bytes(last_exec_block.as_key(), last_exec_block.encoded())
                .map_err(BlockStoreError::from)?;
        } else if maybe_last_committed_block.is_some() && maybe_last_executed_block.is_some() {
            let committed_block = maybe_last_committed_block
                .as_ref()
                .expect("missing commited_block");
            let executed_block = maybe_last_executed_block
                .as_ref()
                .expect("missing executed_block")
                .certified_block();
            if committed_block != executed_block
                && committed_block
                    .block()
                    .correctly_extends(executed_block.qc())
                    .is_err()
            {
                return Err(BlockStoreError::Fatal(
                    format!("Invalid block-persistent-store state. \
                             Last committed block DOES NOT correctly extend executed block: \
                             Committed block:{committed_block:?}, Executed block: {executed_block:?}")));
            }
            info!(
                "Block store resumes with {maybe_last_committed_block:?} as last committed block \
                   and {maybe_last_executed_block:?} as last executed block."
            );
            self.last_committed_block_height = committed_block.height();
        } else {
            return Err(BlockStoreError::Fatal(
                format!("Invalid block-persistent-store state. Last committed block and executed \
                         block data are inconsistent: \
                         Committed block:{maybe_last_committed_block:?}, Executed block: {maybe_last_executed_block:?}")));
        }
        Ok(())
    }

    /// Stores/Persists the block and batches in the permanent storage.
    /// TODO:  Should we store data as prunable, as if snapshots are going to be used for validators,
    /// and they have pruning enabled, we should make sure that this data will eventually be
    /// pruned in scope of validators.
    fn persist(&mut self, full_block: FullBlock) -> Result<(), BlockStoreError> {
        let FullBlock { base, batches } = full_block;
        for PackedSmrBatch { key, batch } in batches.data {
            debug!("write batch to store with key as vector: {key}");
            self.store.batch.write_bytes(key, batch)?;
        }
        // Index the [CertifiedBlock] by its height. There can only be one commit-certified block
        // per height, and the monotonically increasing nature of the index should improve lookup
        // time.
        self.store
            .put_certified_block(&base)
            .map_err(BlockStoreError::from)
    }

    /// Caches input block in pending execution map.
    /// If the block height is greater than block-cache-size window starting last-committed-block height,
    /// then memo of the block is cached, otherwise full block is cached.
    /// In case if the input block height is lower or equal to the last-committed-block
    /// then regression error is reported.
    fn cache(&mut self, certified_block: CertifiedBlock) -> Result<(), BlockStoreError> {
        let height = certified_block.height();
        self.ensure_can_be_cached(&height)?;
        let block_ref = if height
            <= self.last_committed_block_height + self.certified_block_cache_bucket_size
        {
            BlockReference::Block(certified_block)
        } else {
            BlockReference::Memo(BlockInfo::from(certified_block.block()))
        };
        self.pending_execution.insert(height, Some(block_ref));
        Ok(())
    }

    /// Checks and returns the block data corresponding to the input height if any exists in the
    /// cache with pending execution status.
    /// Note that for entry with [BlockReference::Scheduled] value nothing is returned as that
    /// block has already sent for execution.
    /// If there is no block data can be retrieved for the provided height although there is an entry
    /// or memo for the specified height, then [BlockStoreError::InvalidState] is reported, and
    /// invalid state is cleared, i.e. entry from the cache is removed.
    fn take_next_ready_block_by_height(
        &mut self,
        height: Height,
    ) -> Result<Option<CertifiedBlock>, BlockStoreError> {
        let maybe_entry = self.pending_execution.get_mut(&height);
        let Some(entry) = maybe_entry else {
            return Ok(None);
        };
        let block_ref = entry.take();
        let (new_entry, result) = match block_ref {
            None => {
                // Cleanup invalid state and report an error.
                self.pending_execution.remove(&height);
                return Err(BlockStoreError::InvalidState(format!(
                    "Invalid state of the block store,\
                 there is an entry for height: {height}, but no actual data is available!"
                )));
            }
            Some(BlockReference::Block(certified_block)) => {
                let scheduled_memo = BlockInfo::from(certified_block.block());
                (
                    BlockReference::Scheduled(scheduled_memo),
                    Some(certified_block),
                )
            }
            Some(BlockReference::Memo(memo)) => {
                let maybe_block = self.store.certified_block.read(memo.height(), None)?;
                match maybe_block {
                    None => {
                        // Cleanup invalid state and report an error.
                        self.pending_execution.remove(&height);
                        return Err(BlockStoreError::InvalidState(
                           format!("Invalid state of the block store, there is a memo for block at height:\
                                    {height}, but no block data is available!")));
                    }
                    Some(block) => (
                        BlockReference::Scheduled(memo),
                        Some(block.hydrate(&self.store).map_err(BlockStoreError::from)?),
                    ),
                }
            }
            Some(BlockReference::Scheduled(memo)) => (BlockReference::Scheduled(memo), None),
        };
        *entry = Some(new_entry);
        Ok(result)
    }

    /// Retain pending block references which have greater height than input height.
    fn retain(&mut self, lower_bound: &Height) {
        self.pending_execution
            .retain(|block_height, _| block_height > lower_bound)
    }

    /// Retrieves the last executed block data from persistent block-store.
    fn last_executed_block(&self) -> Result<Option<ExecutedBlock>, BlockStoreError> {
        let maybe_last_executed_block = self
            .store
            .last_state
            .read(LastStateItemKey::ExecutedBlock, None)?;
        Ok(maybe_last_executed_block
            .and_then(|entry| entry.into_last_executed_block())
            .map(|b| b.into_owned()))
    }

    /// Retrieves the last committed block data from persistent block-store.
    fn last_committed_block(&self) -> Result<Option<CertifiedBlock>, BlockStoreError> {
        let maybe_last_commit = self
            .store
            .last_state
            .read(LastStateItemKey::CommittedBlock, None)?;
        Ok(maybe_last_commit
            .and_then(|entry| entry.into_last_committed_block())
            .map(|last_commit| last_commit.into_owned()))
    }

    /// Persists current block as last committed block.
    fn update_last_committed_block_info(
        &mut self,
        block: &CertifiedBlock,
    ) -> Result<(), BlockStoreError> {
        self.last_committed_block_height = block.height();
        let last_block = LastStateItemValue::CommittedBlock(Cow::Borrowed(block));
        self.store
            .last_state
            .write_bytes(last_block.as_key(), last_block.encoded())
            .map_err(BlockStoreError::from)
    }

    /// Persists current block as epoch bounder block.
    fn update_epoch_boundary_block_info(
        &mut self,
        block: &CertifiedBlock,
    ) -> Result<(), BlockStoreError> {
        let last_epoch_boundary_block_qc =
            LastStateItemValue::EpochBoundaryBlockQC(Cow::Borrowed(block.qc()));
        self.store
            .last_state
            .write_bytes(
                last_epoch_boundary_block_qc.as_key(),
                last_epoch_boundary_block_qc.encoded(),
            )
            .map_err(BlockStoreError::from)
    }

    /// Checks whether block-store has potential block data of the input height.
    fn has_entry(&self, height: Height) -> bool {
        height <= self.last_committed_block_height || self.pending_execution.contains_key(&height)
    }
}

#[cfg(test)]
#[path = "tests/block_store.rs"]
pub(crate) mod block_store_tests;
