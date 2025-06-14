#![allow(clippy::unwrap_used)]

use crate::consensus::block_store::BlockReference;
use crate::consensus::{BlockStore, BlockStoreError, TBlockStore};
use committees::{CertifiedBlock, GenesisStateProvider, Height, SmrQC, TBlockHeader};
use configurations::rpc::ChainStateAssemblerConfig;
use execution::test_utils::executor::ExecutorTestDataProvider;
use rocksstore::chain_storage::block_store::ChainStore;
use rocksstore::chain_storage::consensus_last_state::{LastStateItemKey, LastStateItemValue};
use rocksstore::chain_storage::ChainStoreBuilder;
use rpc::FullBlock;
use smr_timestamp::SmrTimestamp;
use socrypto::Hash;
use soserde::SmrSerialize;
use std::borrow::Cow;
use tempfile::Builder;
use traits::Storable;
use types::batches::batch::{PackedBatchData, PackedSmrBatch, SmrBatch};
use types::executed_block::ExecutedBlock;

pub(crate) fn create_store_db() -> ChainStore {
    let timestamp = SmrTimestamp::now().as_duration().as_micros();
    let store_temp_dir = Builder::new()
        .prefix(format!("store_db_{timestamp}").as_str())
        .tempdir()
        .unwrap();

    ChainStoreBuilder::new()
        .with_path(store_temp_dir.into_path())
        .build()
        .unwrap()
}

pub(crate) fn persist_executed_block(block_store: &BlockStore, executed_block: &ExecutedBlock) {
    let last_exec_block = LastStateItemValue::ExecutedBlock(Cow::Borrowed(executed_block));
    block_store
        .store
        .last_state
        .write_bytes(last_exec_block.as_key(), last_exec_block.encoded())
        .expect("Successful DB write");
}

pub(crate) fn prepare_full_block(
    certified_block: &CertifiedBlock,
    batches: &[SmrBatch],
) -> FullBlock {
    let ser_batches = batches
        .iter()
        .map(|a| PackedSmrBatch::new(a.store_key(), a.to_bytes()))
        .collect::<Vec<_>>();
    FullBlock {
        base: certified_block.clone(),
        batches: PackedBatchData::new(ser_batches),
    }
}

#[cfg(test)]
impl BlockStore {
    /// Returns cached entry for the height if any
    pub(crate) fn get_cache_entry(&self, height: Height) -> Option<&Option<BlockReference>> {
        self.pending_execution.get(&height)
    }

    /// Checks whether the block at specified height is part of the persistent storage
    pub(crate) fn has_persisted_block(&self, height: Height) -> bool {
        self.store
            .certified_block
            .has_entry(height)
            .expect("Successful DB read")
    }

    /// Checks whether the batch with specified key is part of the persistent storage
    pub(crate) fn has_persisted_batch(&self, key: Hash) -> bool {
        self.store.batch.has_entry(key).expect("Successful DB read")
    }

    pub(crate) fn check_epoch_boundary_info(&self, expected_boundary_info: Option<&SmrQC>) {
        let entry = self
            .store
            .last_state
            .read(LastStateItemKey::EpochBoundaryBlockQC, None)
            .expect("Successful DB read");
        let maybe_boundary_info =
            entry.and_then(|lv| lv.into_last_epoch_boundary_qc().map(|v| v.into_owned()));
        assert_eq!(maybe_boundary_info.as_ref(), expected_boundary_info);
    }
}

#[test]
fn check_initialization() {
    let store = create_store_db();
    let mut block_store = BlockStore::new(
        store,
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE,
    );

    // Check non initialized block store properties.
    let last_executed_block = block_store
        .last_executed_block()
        .expect("Successful DB read");
    assert!(last_executed_block.is_none());
    let last_committed_block = block_store
        .last_committed_block()
        .expect("Successful DB read");
    assert!(last_committed_block.is_none());
    assert_eq!(block_store.last_committed_block_height, 0);
    assert!(block_store.pending_execution.is_empty());
    assert_eq!(
        block_store.certified_block_cache_bucket_size,
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE
    );

    // Check non initialized block store properties.
    // Can create blocks with single batch and each batch will have single transaction.
    let test_data_provider = ExecutorTestDataProvider::default();
    let genesis_certified_block = test_data_provider
        .genesis_state_provider()
        .certified_block();
    block_store
        .initialize(test_data_provider.genesis_state_provider())
        .expect("Successful initialization");

    let last_executed_block = block_store
        .last_executed_block()
        .expect("Successful DB read")
        .expect("ExecutedBlock is available");
    assert_eq!(
        last_executed_block.certified_block(),
        &genesis_certified_block
    );
    assert!(last_executed_block.is_finalized());
    assert_eq!(last_executed_block.transaction_count(), 0);
    assert!(last_executed_block.batches().is_empty());

    let last_committed_block = block_store
        .last_committed_block()
        .expect("Successful DB read")
        .expect("Last Committed block is available");
    assert_eq!(last_committed_block, genesis_certified_block);
    assert_eq!(
        block_store.last_committed_block_height,
        last_committed_block.height()
    );
    assert!(block_store.pending_execution.is_empty());
    assert_eq!(
        block_store.certified_block_cache_bucket_size,
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE
    );

    let executed_chain = test_data_provider.executed_chain_with_empty_outputs();
    assert!(!executed_chain.is_empty());
    // Update only last committed block state, create a new instance of the block store and try to initialize with genesis state provider.
    block_store
        .update_last_committed_block_info(executed_chain[0].certified_block())
        .expect("Successful DB write");

    let mut block_store_with_last_commit_state = BlockStore::new(
        block_store.store.clone(),
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE,
    );
    let last_executed_block = block_store_with_last_commit_state
        .last_executed_block()
        .expect("Successful DB read")
        .expect("ExecutedBlock is available");
    assert_eq!(
        last_executed_block.certified_block(),
        &genesis_certified_block
    );
    assert!(last_executed_block.is_finalized());
    assert_eq!(last_executed_block.transaction_count(), 0);
    assert!(last_executed_block.batches().is_empty());

    let last_committed_block = block_store_with_last_commit_state
        .last_committed_block()
        .expect("Successful DB read")
        .expect("Last Committed block is available");
    assert_eq!(&last_committed_block, executed_chain[0].certified_block());
    // it is still 0 as we have not yet initialized it.
    assert_eq!(
        block_store_with_last_commit_state.last_committed_block_height,
        0
    );
    assert!(block_store_with_last_commit_state
        .pending_execution
        .is_empty());
    assert_eq!(
        block_store_with_last_commit_state.certified_block_cache_bucket_size,
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE
    );

    block_store_with_last_commit_state
        .initialize(test_data_provider.genesis_state_provider())
        .expect("Successful initialization!");
    let last_executed_block = block_store_with_last_commit_state
        .last_executed_block()
        .expect("Successful DB read")
        .expect("ExecutedBlock is available");
    assert_eq!(
        last_executed_block.certified_block(),
        &genesis_certified_block
    );

    let last_committed_block = block_store_with_last_commit_state
        .last_committed_block()
        .expect("Successful DB read")
        .expect("Last Committed block is available");
    assert_eq!(&last_committed_block, executed_chain[0].certified_block());
    // it is not 0 anymore.
    assert_ne!(
        block_store_with_last_commit_state.last_committed_block_height,
        0
    );
    assert_eq!(
        block_store_with_last_commit_state.last_committed_block_height,
        executed_chain[0].certified_block().height()
    );
    assert!(block_store_with_last_commit_state
        .pending_execution
        .is_empty());

    // Inconsistent state of the store, commit-block-height 2, executed-block-height-0
    block_store
        .update_last_committed_block_info(executed_chain[1].certified_block())
        .expect("Successful DB write");
    let mut block_store_with_inconsistent_store = BlockStore::new(
        block_store.store.clone(),
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE,
    );

    assert!(matches!(
        block_store_with_inconsistent_store.initialize(test_data_provider.genesis_state_provider()),
        Err(BlockStoreError::Fatal(_))
    ));

    // Inconsistent state of the store, commit-block-height 2, executed-block-height-4
    persist_executed_block(&block_store, &executed_chain[3]);
    let mut block_store_with_inconsistent_store = BlockStore::new(
        block_store.store.clone(),
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE,
    );
    assert!(matches!(
        block_store_with_inconsistent_store.initialize(test_data_provider.genesis_state_provider()),
        Err(BlockStoreError::Fatal(_))
    ));

    // Consistent state of the store, commit-block-height 4, executed-block-height-4
    block_store
        .update_last_committed_block_info(executed_chain[3].certified_block())
        .expect("Successful DB write");
    let mut block_store_with_inconsistent_store = BlockStore::new(
        block_store.store.clone(),
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE,
    );
    assert!(block_store_with_inconsistent_store
        .initialize(test_data_provider.genesis_state_provider())
        .is_ok());

    // Executed block info is present but no committed block is available.
    let store = create_store_db();
    let mut block_store_with_invalid_state = BlockStore::new(
        store,
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE,
    );
    persist_executed_block(&block_store_with_invalid_state, &executed_chain[0]);
    assert!(matches!(
        block_store_with_invalid_state.initialize(test_data_provider.genesis_state_provider()),
        Err(BlockStoreError::Fatal(_))
    ));

    // Last committed block info is present but no executed block is available.
    let store = create_store_db();
    let mut block_store_with_invalid_state = BlockStore::new(
        store,
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE,
    );
    block_store_with_invalid_state
        .update_last_committed_block_info(executed_chain[0].certified_block())
        .expect("Successful DB write");
    assert!(matches!(
        block_store_with_invalid_state.initialize(test_data_provider.genesis_state_provider()),
        Err(BlockStoreError::Fatal(_))
    ));
}

#[test]
fn check_persist_api() {
    let store = create_store_db();
    let mut block_store = BlockStore::new(
        store,
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE,
    );
    // Can create blocks with single batch and each batch will have single transaction.
    let test_data_provider = ExecutorTestDataProvider::default();
    block_store
        .initialize(test_data_provider.genesis_state_provider())
        .expect("Successful initialization");
    assert_eq!(block_store.last_committed_block_height, 0);
    assert!(block_store.pending_execution.is_empty());

    let (certified_blocks, batches) = test_data_provider.generate_block_chain();
    let full_block = prepare_full_block(&certified_blocks[0], &batches[0]);
    block_store
        .persist(full_block)
        .expect("Successful DB write");
    assert!(block_store.has_persisted_block(certified_blocks[0].store_key()));
    batches[0].iter().for_each(|b| {
        assert!(block_store.has_persisted_batch(b.store_key()));
    });
    assert_eq!(block_store.last_committed_block_height, 0);
    assert!(block_store.pending_execution.is_empty());
}

#[test]
fn check_cache_api() {
    let block_cache_bucket_size = 2;
    let store = create_store_db();
    let mut block_store = BlockStore::new(store, block_cache_bucket_size);
    // Can create blocks with single batch and each batch will have single transaction.
    let test_data_provider = ExecutorTestDataProvider::default();
    block_store
        .initialize(test_data_provider.genesis_state_provider())
        .expect("Successful initialization");
    assert_eq!(block_store.last_committed_block_height, 0);
    assert!(block_store.pending_execution.is_empty());

    let (certified_blocks, _batches) = test_data_provider.generate_block_chain();
    certified_blocks.iter().for_each(|b| {
        block_store.cache(b.clone()).expect("Successful caching");
        match block_store.pending_execution.get(&b.height()) {
            None => panic!("Entry for {} should exists", b.height()),
            Some(None) => panic!("Valid entry value should be available"),
            Some(Some(data)) => match data {
                BlockReference::Block(data) => {
                    assert_eq!(data, b);
                    assert!(
                        b.height()
                            <= block_store.last_committed_block_height + block_cache_bucket_size
                    );
                }
                BlockReference::Memo(info) => {
                    assert_eq!(info.header(), b.header());
                    assert!(
                        b.height()
                            > block_store.last_committed_block_height + block_cache_bucket_size
                    );
                }
                BlockReference::Scheduled(_) => panic!("Block or Memo is expected."),
            },
        }
    });

    // increase local last_committed_block_height and try to cache blocks which have lower or equal height.
    block_store.last_committed_block_height = 2;
    assert!(matches!(
        block_store.cache(certified_blocks[1].clone()),
        Err(BlockStoreError::InvalidOperation(_))
    ));
}
#[test]
fn check_cache_take_api() {
    let block_cache_bucket_size = 2;
    let store = create_store_db();
    let mut block_store = BlockStore::new(store, block_cache_bucket_size);
    // Can create blocks with single batch and each batch will have single transaction.
    let test_data_provider = ExecutorTestDataProvider::default();
    block_store
        .initialize(test_data_provider.genesis_state_provider())
        .expect("Successful initialization");
    assert_eq!(block_store.last_committed_block_height, 0);
    assert!(block_store.pending_execution.is_empty());

    // Persist and cache all blocks except the last one.
    let (certified_blocks, _batches) = test_data_provider.generate_block_chain();
    assert!(!certified_blocks.is_empty());
    certified_blocks
        .iter()
        .take(certified_blocks.len() - 1)
        .for_each(|b| {
            let full_block = FullBlock {
                base: b.clone(),
                // here we do not store batches, as they are irrelevant for this test-case.
                batches: Default::default(),
            };
            block_store
                .persist(full_block)
                .expect("Successful DB write");
            block_store.cache(b.clone()).expect("Successful caching");
        });
    assert_eq!(
        block_store.pending_execution.len(),
        certified_blocks.len() - 1
    );

    // Request non-existent entry.
    let last_block_height = certified_blocks.last().unwrap().height();
    assert!(matches!(
        block_store.take_next_ready_block_by_height(last_block_height),
        Ok(None)
    ));

    // take block at height 1.
    let block_1 = block_store
        .take_next_ready_block_by_height(1)
        .expect("Successful retrieval")
        .expect("Block data is available");
    assert_eq!(&certified_blocks[0], &block_1);
    let block_ref = block_store
        .pending_execution
        .get(&1)
        .expect("Entry exists")
        .as_ref()
        .expect("Entry has valid value");
    assert!(matches!(block_ref, BlockReference::Scheduled(_)));

    // take block at height 3 which reference in cache is a memo.
    assert!(matches!(
        block_store.pending_execution.get(&3).expect("Entry exists"),
        Some(BlockReference::Memo(_))
    ));
    let block_3 = block_store
        .take_next_ready_block_by_height(3)
        .expect("Successful retrieval")
        .expect("Block data is available");
    assert_eq!(&certified_blocks[2], &block_3);
    let block_ref = block_store
        .pending_execution
        .get(&3)
        .expect("Entry exists")
        .as_ref()
        .expect("Entry has valid value");
    assert!(matches!(block_ref, BlockReference::Scheduled(_)));

    // Try to cache the value that is already scheduled
    let result = block_store.cache(block_3);
    assert!(matches!(result, Err(BlockStoreError::InvalidOperation(_))));

    // Try to cache the value that has not scheduled entry
    let result = block_store.cache(certified_blocks[3].clone());
    assert!(result.is_ok());
    let block_ref = block_store
        .pending_execution
        .get(&4)
        .expect("Entry exists")
        .as_ref()
        .expect("Entry has valid value");
    assert!(matches!(block_ref, BlockReference::Memo(_)));

    // Corrupt block_4 entry and try to cache one more time
    block_store.pending_execution.insert(4, None);
    let result = block_store.cache(certified_blocks[3].clone());
    assert!(result.is_ok());
    let block_ref = block_store
        .pending_execution
        .get(&4)
        .expect("Entry exists")
        .as_ref()
        .expect("Entry has valid value");
    assert!(matches!(block_ref, BlockReference::Memo(_)));

    // Corrupt block_2 entry and try to take it, check that error is returned and corrupted state is cleared.
    block_store.pending_execution.insert(2, None);
    let block_2 = block_store.take_next_ready_block_by_height(2);
    assert!(matches!(block_2, Err(BlockStoreError::InvalidState(_))));
    assert!(!block_store.pending_execution.contains_key(&2));

    // Update the cache with the last block memo, but do not persist it. `take...` will return error
    // and block-store invalid state will be cleared.
    block_store
        .cache(certified_blocks.last().unwrap().clone())
        .expect("Successful caching");
    assert!(matches!(
        block_store
            .pending_execution
            .get(&last_block_height)
            .expect("Entry exists"),
        Some(BlockReference::Memo(_))
    ));
    let block_last = block_store.take_next_ready_block_by_height(last_block_height);
    assert!(!block_store
        .pending_execution
        .contains_key(&last_block_height));
    assert!(matches!(block_last, Err(BlockStoreError::InvalidState(_))));
}

#[test]
fn check_retain_api() {
    let block_cache_bucket_size = 2;
    let store = create_store_db();
    let mut block_store = BlockStore::new(store, block_cache_bucket_size);
    // Can create blocks with single batch and each batch will have single transaction.
    let test_data_provider = ExecutorTestDataProvider::default();
    block_store
        .initialize(test_data_provider.genesis_state_provider())
        .expect("Successful initialization");
    assert_eq!(block_store.last_committed_block_height, 0);
    assert!(block_store.pending_execution.is_empty());

    let (certified_blocks, batches) = test_data_provider.generate_block_chain();
    assert!(!certified_blocks.is_empty());
    certified_blocks.iter().enumerate().for_each(|(idx, b)| {
        let full_block = prepare_full_block(b, &batches[idx]);
        block_store
            .persist(full_block)
            .expect("Successful DB write");
        block_store.cache(b.clone()).expect("Successful caching");
    });
    assert_eq!(block_store.pending_execution.len(), certified_blocks.len());
    assert_eq!(block_store.last_committed_block_height, 0);

    // In cache retain only blocks higher than 3
    block_store.retain(&3);
    assert_eq!(block_store.last_committed_block_height, 0);
    assert_eq!(
        block_store.pending_execution.len(),
        certified_blocks.len() - 3
    );
    certified_blocks.iter().enumerate().for_each(|(idx, b)| {
        assert_eq!(
            block_store.pending_execution.contains_key(&b.height()),
            b.height() > 3
        );
        // Check that persistent storage is intact.
        assert!(block_store.has_persisted_block(b.store_key()));
        batches[idx].iter().for_each(|bt| {
            assert!(block_store.has_persisted_batch(bt.store_key()));
        });
    });
}

#[test]
fn check_update_apis() {
    let block_cache_bucket_size = 2;
    let store = create_store_db();
    let mut block_store = BlockStore::new(store, block_cache_bucket_size);
    // Can create blocks with single batch and each batch will have single transaction.
    let test_data_provider = ExecutorTestDataProvider::default();
    block_store
        .initialize(test_data_provider.genesis_state_provider())
        .expect("Successful initialization");
    assert_eq!(block_store.last_committed_block_height, 0);
    assert!(block_store.pending_execution.is_empty());

    let (certified_blocks, _batches) = test_data_provider.generate_block_chain();
    assert!(!certified_blocks.is_empty());
    // check that update-apis do not update block-tables
    certified_blocks.iter().for_each(|b| {
        block_store
            .update_last_committed_block_info(b)
            .expect("Successful DB write");
        let last_committed_block = block_store
            .last_committed_block()
            .expect("Successful DB read");
        assert_eq!(last_committed_block.as_ref(), Some(b));
        assert_eq!(block_store.last_committed_block_height, b.height());
        assert!(!block_store.has_persisted_block(b.store_key()));
    });

    block_store.check_epoch_boundary_info(None);
    let last_block = certified_blocks.last().unwrap();
    block_store
        .update_epoch_boundary_block_info(last_block)
        .expect("Successful DB write");
    block_store.check_epoch_boundary_info(Some(last_block.qc()));
}

#[test]
fn check_has_entry_api() {
    let block_cache_bucket_size = 2;
    let store = create_store_db();
    let mut block_store = BlockStore::new(store, block_cache_bucket_size);
    // Can create blocks with single batch and each batch will have single transaction.
    let test_data_provider = ExecutorTestDataProvider::default();
    block_store
        .initialize(test_data_provider.genesis_state_provider())
        .expect("Successful initialization");
    assert_eq!(block_store.last_committed_block_height, 0);
    assert!(block_store.pending_execution.is_empty());

    let (certified_blocks, _batches) = test_data_provider.generate_block_chain();
    assert!(!certified_blocks.is_empty());
    certified_blocks.iter().for_each(|b| {
        block_store.cache(b.clone()).expect("Successful caching");
    });

    // Check non existent entry.
    assert!(!block_store.has_entry(42));

    // Check existent entry in cache.
    assert!(block_store.has_entry(5));

    // Increase last_committed_block_height, and prune cache data lower 6 height.
    block_store.last_committed_block_height = 6;
    block_store.retain(&5);
    assert!(block_store.has_entry(5));
    assert!(block_store.has_entry(6));
}
