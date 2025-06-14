#![allow(clippy::unwrap_used)]

use crate::consensus::block_store::block_store_tests::{
    persist_executed_block, prepare_full_block,
};
use crate::consensus::block_store::{block_store_tests, BlockReference};
use crate::consensus::block_verifier::block_verifier_tests::TestDataProvider;
use crate::consensus::chain_state_assembler::InternalProcessData;
use crate::consensus::synchronizer::synchronizer_tests::MockSyncApi;
use crate::consensus::{
    AssemblerError, BlockStore, BlockVerifier, ChainStateAssembler, ChainStateAssemblerDataInputs,
    TBlockStore, TBlockVerifier, VerificationResult,
};
use crate::RpcEpochChangeNotificationSchedule;
use aptos_types::block_info::GENESIS_EPOCH;
use aptos_types::transaction::Transaction;
use aptos_types::vm_status::{StatusCode, VMStatus};
use committees::{
    BatchAvailabilityCertificate, BlockInfo, CertifiedBlock, Height, TBlockHeader, TSmrBlock,
};
use configurations::rpc::ChainStateAssemblerConfig;
use consistency_warranter::DUPLICATE_TRANSACTION_EXECUTION_ERROR_MESSAGE;
use epoch_manager::{EpochNotificationAndAckSender, EpochStatesProvider, Notification};
use execution::error::ExecutionError;
use execution::test_utils::executor::ExecutorTestDataProvider;
use execution::traits::{TBlockExecutor, TExecutorStore};
use execution::{DkgExecutionEvents, DkgExecutionResult, MoveExecutionResult};
use lifecycle::{EpochInfo, TEpochId, TView};
use lifecycle_types::{EpochState, EpochStates, TEpochStates, TEpochStatesWrite};
use notifier::Ack;
use rocksstore::chain_storage::block_store::ChainStore;
use rocksstore::chain_storage::consensus_last_state::LastStateItemValue;
use rpc::messages::{RpcBlockData, RpcServerMessage};
use rpc::FullBlock;
use smrcommon::get_random_port;
use socrypto::Hash;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use supra_logger::init_runtime_logger;
use task_manager::async_shutdown_token;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use traits::Storable;
use transactions::SignedSmrTransaction;
use types::batches::batch::SmrBatch;
use types::executed_block::ExecutedBlock;
use types::transactions::block_metadata::BlockMetadata;
use types::transactions::smr_transaction::SmrTransaction;
use types::validator_committee::ValidatorCommittee;

#[derive(Clone)]
struct MockedExecutorTweaks {
    trigger_epoch_change: Arc<AtomicBool>,
    fail_execution: Arc<AtomicBool>,
    duplicated_execution: Arc<AtomicBool>,
}

impl MockedExecutorTweaks {
    fn fail_execution(&mut self) {
        self.fail_execution.store(true, Ordering::Relaxed);
    }
    fn should_fail(&mut self) -> bool {
        self.fail_execution.load(Ordering::Relaxed)
    }
    fn pass_execution(&mut self) {
        self.fail_execution.store(false, Ordering::Relaxed);
    }
    fn enable_duplicated_execution(&mut self) {
        self.duplicated_execution.store(true, Ordering::Relaxed);
    }
    fn disable_duplicated_execution(&mut self) {
        self.duplicated_execution.store(false, Ordering::Relaxed);
    }
    fn should_fail_duplicated_execution(&self) -> bool {
        self.duplicated_execution.load(Ordering::Relaxed)
    }
    fn enable_epoch_change(&mut self) {
        self.trigger_epoch_change.store(true, Ordering::Relaxed);
    }
    fn disable_epoch_change(&mut self) {
        self.trigger_epoch_change.store(false, Ordering::Relaxed);
    }

    fn get_epoch_change(&self) -> bool {
        self.trigger_epoch_change.load(Ordering::Relaxed)
    }
}
#[derive(Clone)]
struct MockedExecutor {
    store: ChainStore,
    tweaks: MockedExecutorTweaks,
}

impl MockedExecutor {
    fn create(store: ChainStore) -> (MockedExecutor, MockedExecutorTweaks) {
        let tweaks = MockedExecutorTweaks {
            trigger_epoch_change: Arc::new(AtomicBool::new(false)),
            fail_execution: Arc::new(AtomicBool::new(false)),
            duplicated_execution: Arc::new(AtomicBool::new(false)),
        };
        let executor = MockedExecutor {
            store,
            tweaks: tweaks.clone(),
        };
        (executor, tweaks)
    }
}

#[async_trait::async_trait]
impl TBlockExecutor for MockedExecutor {
    fn execution_store(&self) -> &impl TExecutorStore {
        &self.store
    }

    fn finalize_batch(&mut self, _bac: &BatchAvailabilityCertificate) -> bool {
        true
    }

    fn retrieve_batch(&self, _key: Hash, _ctx: String) -> Result<SmrBatch, ExecutionError> {
        unimplemented!("Should not be called in test context!")
    }

    async fn execute_move_transactions(
        &self,
        _move_transactions: Vec<Transaction>,
        _info_context: &CertifiedBlock,
    ) -> Result<MoveExecutionResult, ExecutionError> {
        unimplemented!("Should not be called in test context!")
    }

    fn execute_dkg_transactions(
        &mut self,
        _dkg_transactions: Vec<SignedSmrTransaction>,
        _info_context: &CertifiedBlock,
    ) -> Result<DkgExecutionResult, ExecutionError> {
        unimplemented!("should not be called in test context!")
    }

    fn output_executed_block(
        &mut self,
        _finalized_block: ExecutedBlock,
    ) -> Result<(), ExecutionError> {
        unimplemented!("should not be called in test context!")
    }

    async fn output_dkg_events(
        &self,
        _events: DkgExecutionEvents,
        _committed_block: &CertifiedBlock,
    ) -> Result<(), ExecutionError> {
        unimplemented!("should not be called in test context!")
    }

    fn propagate_mvm_gas_prices(&mut self, _gas_prices: Vec<u64>) {
        unimplemented!("should not be called in test context!")
    }

    async fn on_new_epoch(
        &mut self,
        _new_epoch_info: Option<EpochInfo>,
    ) -> Result<(), ExecutionError> {
        unimplemented!("should not be called in test context!")
    }

    async fn execute_block(
        &mut self,
        committed_block: &CertifiedBlock,
        _validator_committee: &ValidatorCommittee,
    ) -> Result<(bool, Vec<SmrTransaction>), ExecutionError>
    where
        Self: Sync,
    {
        if self.tweaks.should_fail() {
            return Err(ExecutionError::Internal(
                "Test is configured to fail execution".to_string(),
            ));
        } else if self.tweaks.should_fail_duplicated_execution() {
            return Err(ExecutionError::MoveVmError(VMStatus::error(
                StatusCode::UNEXPECTED_ERROR_FROM_KNOWN_MOVE_FUNCTION,
                Some(DUPLICATE_TRANSACTION_EXECUTION_ERROR_MESSAGE.into()),
            )));
        }

        let block_metadata = BlockMetadata::dummy(committed_block);
        let executed_block =
            ExecutedBlock::new(committed_block.clone(), vec![], vec![], block_metadata)
                .finalize()
                .expect("Successful finalization");
        let last_exec_block = LastStateItemValue::ExecutedBlock(Cow::Borrowed(&executed_block));
        self.store
            .last_state
            .write_bytes(last_exec_block.as_key(), last_exec_block.encoded())
            .expect("Successful DB write");
        // return configure trigger and reset it
        let epoch_changed = self.tweaks.get_epoch_change();
        self.tweaks.disable_epoch_change();
        Ok((epoch_changed, vec![]))
    }
}

#[derive(Clone)]
struct MockedBlockVerifier {
    verification_result: VerificationResult,
    epoch_states: EpochStates,
}

impl MockedBlockVerifier {
    async fn create(
        epoch_state_provider: &impl EpochStatesProvider<RpcEpochChangeNotificationSchedule>,
    ) -> Self {
        Self {
            verification_result: VerificationResult::Success,
            epoch_states: epoch_state_provider.get_epoch_states(1).await,
        }
    }
}

impl TEpochStates for MockedBlockVerifier {
    fn epoch_states(&self) -> &EpochStates {
        &self.epoch_states
    }
}

impl TEpochStatesWrite for MockedBlockVerifier {
    fn epoch_states_mut(&mut self) -> &mut EpochStates {
        &mut self.epoch_states
    }
}

impl TBlockVerifier for MockedBlockVerifier {
    fn verify(&self, _block: &FullBlock) -> VerificationResult {
        self.verification_result.clone()
    }
}

struct InputProviderIfc {
    block_data_tx: Sender<RpcBlockData>,
    epoch_data_tx: Sender<EpochNotificationAndAckSender>,
}

impl InputProviderIfc {
    fn send_end_epoch(&self) -> oneshot::Receiver<Ack> {
        let notification = Notification::EndEpoch;
        let (tx, rx) = oneshot::channel::<Ack>();
        let end_epoch = (notification, tx);
        self.epoch_data_tx
            .try_send(end_epoch)
            .expect("Successful delivery");
        rx
    }
    async fn check_ack(rx: oneshot::Receiver<Ack>, expect_ack: bool) {
        match timeout(Duration::from_secs(3), rx)
            .await
            .expect("Successful wait")
        {
            Ok(_) => assert!(expect_ack, "Acknowledge was not expected but got one"),
            Err(_) => assert!(!expect_ack, "Acknowledge was expected but did not get one"),
        };
    }

    fn send_new_epoch(&self, epoch_state: EpochState) -> oneshot::Receiver<Ack> {
        let notification = Notification::NewEpoch(Box::new(epoch_state));
        let (tx, rx) = oneshot::channel::<Ack>();
        let end_epoch = (notification, tx);
        self.epoch_data_tx
            .try_send(end_epoch)
            .expect("Successful delivery");
        rx
    }

    fn send_sync_block(&self, certified_block: &CertifiedBlock, batches: &[SmrBatch]) {
        let full_block = prepare_full_block(certified_block, batches);
        self.block_data_tx
            .try_send(RpcBlockData::SyncBlock(full_block))
            .expect("Successful delivery");
    }

    fn send_new_block(&self, certified_block: &CertifiedBlock, batches: &[SmrBatch]) {
        let full_block = prepare_full_block(certified_block, batches);
        self.block_data_tx
            .try_send(RpcBlockData::NewBlock(full_block))
            .expect("Successful delivery");
    }
}

fn create_external_inout(size: usize) -> (InputProviderIfc, ChainStateAssemblerDataInputs) {
    let (block_data_tx, block_data_rx) = channel::<RpcBlockData>(size);
    let (epoch_data_tx, epoch_data_rx) = channel::<EpochNotificationAndAckSender>(size);
    let input = InputProviderIfc {
        block_data_tx,
        epoch_data_tx,
    };
    let output = ChainStateAssemblerDataInputs::new(block_data_rx, epoch_data_rx);
    (input, output)
}

struct InternalProcessDataRef {
    shutdown_token: CancellationToken,
    feedback_rx: Receiver<CertifiedBlock>,
    feedback_tx: Sender<CertifiedBlock>,
}

impl InternalProcessDataRef {
    fn create() -> Self {
        let shutdown_token = async_shutdown_token();
        let (feedback_tx, feedback_rx) = channel::<CertifiedBlock>(1);
        Self {
            shutdown_token,
            feedback_rx,
            feedback_tx,
        }
    }

    fn get(&mut self) -> InternalProcessData {
        InternalProcessData {
            shutdown_token: &self.shutdown_token,
            ready_to_execute_tx: &self.feedback_tx,
            ready_to_execute_rx: &mut self.feedback_rx,
        }
    }

    pub(crate) async fn expect_new_scheduled_block(&mut self) -> CertifiedBlock {
        let res = timeout(Duration::from_secs(2), self.feedback_rx.recv()).await;
        assert!(res.is_ok());
        res.unwrap().expect("Channel had available data")
    }

    pub(crate) async fn expect_no_scheduled_block(&mut self) {
        let res = timeout(Duration::from_secs(1), self.feedback_rx.recv()).await;
        assert!(res.is_err())
    }
}

struct TestAssets {
    test_data_provider: TestDataProvider,
    input_providers: InputProviderIfc,
    executor_tweaks: MockedExecutorTweaks,
    assembler: ChainStateAssembler<BlockStore, BlockVerifier, MockedExecutor, MockSyncApi>,
    process_data_ref: InternalProcessDataRef,
}

impl TestAssets {
    async fn initialize() -> Self {
        let sync_interval: u64 = 3;
        let test_data_provider = TestDataProvider::from(ExecutorTestDataProvider::default());

        // Block-store properly initialized with genesis data
        let store = block_store_tests::create_store_db();
        let mut block_store = BlockStore::new(
            store.clone(),
            ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE,
        );
        block_store
            .initialize(test_data_provider.genesis_state_provider())
            .expect("Successful initialization");

        // Executor
        let (executor, executor_tweaks) = MockedExecutor::create(store);

        // Sync API
        let sync_api = MockSyncApi::new(10);

        // Block-Verifier
        let block_verifier = BlockVerifier::new(&test_data_provider, false).await;

        // Data Inputs
        let (input_providers, inputs) = create_external_inout(10);

        // Block store is not properly initialized so creation should fail;
        let assembler = ChainStateAssembler::new(
            inputs,
            block_store.clone(),
            block_verifier,
            executor.clone(),
            sync_api,
            sync_interval,
        )
        .await
        .expect("Successful Assembler instance creation");

        Self {
            test_data_provider,
            input_providers,
            executor_tweaks,
            assembler,
            process_data_ref: InternalProcessDataRef::create(),
        }
    }
}

#[tokio::test]
async fn check_creation() {
    let sync_interval: u64 = 3;
    let test_data_provider = TestDataProvider::from(ExecutorTestDataProvider::default());

    // Block-store
    let store = block_store_tests::create_store_db();
    let mut block_store = BlockStore::new(
        store.clone(),
        ChainStateAssemblerConfig::DEFAULT_BLOCK_CACHE_BUCKET_SIZE,
    );

    // Executor
    let (executor, mut executor_tweaks) = MockedExecutor::create(store.clone());

    // Sync API
    let sync_api = MockSyncApi::new(10);

    // Block-Verifier
    let block_verifier = MockedBlockVerifier::create(&test_data_provider).await;

    // Data Inputs
    let (_inputs_providers, inputs) = create_external_inout(10);

    // Block store is not properly initialized so creation should fail;
    let assembler = ChainStateAssembler::new(
        inputs,
        block_store.clone(),
        block_verifier.clone(),
        executor.clone(),
        sync_api,
        sync_interval,
    )
    .await;
    assert!(matches!(
        assembler,
        Err(AssemblerError::InvalidBlockStoreState(_))
    ));

    // When block store almost is properly initialized with some executed block let's check that
    // state-consistency warranter guards inappropriate state.
    let executed_blocks = test_data_provider.executed_chain_with_empty_outputs();
    assert!(!executed_blocks.is_empty());
    persist_executed_block(&block_store, &executed_blocks[0]);

    // Data Inputs
    let (_inputs_providers, inputs) = create_external_inout(10);
    // Sync API
    let sync_api = MockSyncApi::new(10);
    let assembler = ChainStateAssembler::new(
        inputs,
        block_store.clone(),
        block_verifier.clone(),
        executor.clone(),
        sync_api,
        sync_interval,
    )
    .await;
    assert!(matches!(assembler, Err(AssemblerError::Fatal(_))));

    // When state is properly initialized but there is a need to execute last-commit-block but it fails.
    // Check that for now execution error is suppressed.
    block_store
        .update_last_committed_block_info(executed_blocks[1].certified_block())
        .expect("Successful  DB write");
    executor_tweaks.enable_duplicated_execution();

    // Data Inputs
    let (_inputs_providers, inputs) = create_external_inout(10);
    // Sync API
    let sync_api = MockSyncApi::new(10);
    let assembler = ChainStateAssembler::new(
        inputs,
        block_store.clone(),
        block_verifier.clone(),
        executor.clone(),
        sync_api,
        sync_interval,
    )
    .await;

    // Duplicated execution should be handled.
    assert!(assembler.is_ok());

    executor_tweaks.disable_duplicated_execution();
    executor_tweaks.fail_execution();

    // Data Inputs
    let (_inputs_providers, inputs) = create_external_inout(10);
    // Sync API
    let sync_api = MockSyncApi::new(10);
    let assembler = ChainStateAssembler::new(
        inputs,
        block_store,
        block_verifier,
        executor,
        sync_api,
        sync_interval,
    )
    .await;

    // All errors except duplicated execution should be fatal.
    assert!(matches!(assembler, Err(AssemblerError::Fatal(_))));
}

#[tokio::test]
async fn check_epoch_notification_events_handling() {
    let mut test_assets = TestAssets::initialize().await;
    let ack_expected = true;

    // Current epoch of the assembler is 1
    assert_eq!(test_assets.assembler.current_epoch(), GENESIS_EPOCH + 1);
    // Current epoch of the test_provider is 1
    assert_eq!(
        test_assets.test_data_provider.validator_committee().epoch(),
        GENESIS_EPOCH + 1
    );

    // check that upon end/new -epoch notification arrival before internal acknowledgment,
    // handling result is Fatal error.
    assert!(!test_assets.assembler.is_current_epoch_over());
    let ack_rx = test_assets.input_providers.send_end_epoch();
    let result = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await;
    assert!(matches!(result, Err(AssemblerError::Fatal(_))));
    assert!(!test_assets.assembler.is_current_epoch_over());
    InputProviderIfc::check_ack(ack_rx, !ack_expected).await;

    // Epoch 2 state
    let epoch_2_state = test_assets.test_data_provider.get_next_epoch_state(10);
    let ack_rx = test_assets
        .input_providers
        .send_new_epoch(epoch_2_state.clone());
    let result = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await;
    assert!(matches!(result, Err(AssemblerError::Fatal(_))));
    assert!(!test_assets.assembler.is_current_epoch_over());
    InputProviderIfc::check_ack(ack_rx, !ack_expected).await;

    // check that upon end/new -epoch notification arrival and internal acknowledgment is also there
    // handling result is Ok(true).
    test_assets.assembler.handle_epoch_change(true);
    let ack_rx = test_assets.input_providers.send_end_epoch();
    let result = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("Successful step");
    assert!(test_assets.assembler.is_current_epoch_over());
    assert!(result);
    InputProviderIfc::check_ack(ack_rx, ack_expected).await;

    let ack_rx = test_assets
        .input_providers
        .send_new_epoch(epoch_2_state.clone());
    let result = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("Successful step");
    assert!(result);
    assert!(!test_assets.assembler.is_current_epoch_over());
    InputProviderIfc::check_ack(ack_rx, ack_expected).await;
    // Successfully updated to epoch 2
    assert_eq!(test_assets.assembler.current_epoch(), epoch_2_state.epoch());

    // End epoch 2 and check new epoch handling when the new epoch is invalid
    // New epoch is already ended
    test_assets.assembler.handle_epoch_change(true);
    // Move test-data provider to epoch 2
    test_assets.test_data_provider.move_to_next_epoch(10);
    let mut ended_epoch_3_state = test_assets.test_data_provider.get_next_epoch_state(20);
    ended_epoch_3_state.end_epoch();
    let ack_rx = test_assets
        .input_providers
        .send_new_epoch(ended_epoch_3_state);
    let result = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await;
    assert!(matches!(result, Err(AssemblerError::Fatal(_))));
    assert!(test_assets.assembler.is_current_epoch_over());
    assert_eq!(test_assets.assembler.current_epoch(), epoch_2_state.epoch());
    InputProviderIfc::check_ack(ack_rx, !ack_expected).await;

    // Provide epoch data that does not extend current epoch.
    let does_not_follow_current_epoch = test_assets.test_data_provider.current_epoch_state();
    let ack_rx = test_assets
        .input_providers
        .send_new_epoch(does_not_follow_current_epoch);
    let result = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await;
    assert!(matches!(result, Err(AssemblerError::Fatal(_))));
    assert!(test_assets.assembler.is_current_epoch_over());
    assert_eq!(test_assets.assembler.current_epoch(), epoch_2_state.epoch());
    InputProviderIfc::check_ack(ack_rx, !ack_expected).await;

    // Update internal store with some pending blocks in cache, and with block that was observed but not processed.
    // Update to epoch 3 and check that store-cache is cleared from data from epoch 2 and for the
    // next block which is missing sync request is sent
    let (epoch_2_certified_blocks, _) = test_assets
        .test_data_provider
        .generate_block_chain_with_start_height_round_parent(11, 11, Hash::default());
    // Block at height 11
    test_assets
        .assembler
        .store
        .cache(epoch_2_certified_blocks[0].clone())
        .expect("Successfully cached");
    assert!(test_assets
        .assembler
        .store
        .has_entry(epoch_2_certified_blocks[0].height()));
    // Block at height 12
    test_assets
        .assembler
        .store
        .cache(epoch_2_certified_blocks[1].clone())
        .expect("Successfully cached");
    assert!(test_assets
        .assembler
        .store
        .has_entry(epoch_2_certified_blocks[1].height()));
    let epoch_2_last_block = epoch_2_certified_blocks
        .last()
        .expect("Non empty block set");
    // Move test-data provider to epoch 3
    test_assets
        .test_data_provider
        .move_to_next_epoch(epoch_2_last_block.height());
    let (epoch_3_certified_blocks, _) = test_assets
        .test_data_provider
        .generate_block_chain_with_start_height_round_parent(
            epoch_2_last_block.height() + 1,
            epoch_2_last_block.round() + 1,
            epoch_2_last_block.hash(),
        );
    // Set one of the epoch 3 blocks as last observed block
    test_assets.assembler.last_observed_highest_block =
        Some(BlockInfo::from(epoch_3_certified_blocks[2].block()));
    // Store epoch-2-last-block as last executed block.
    test_assets.assembler.last_executed_block_info = BlockInfo::from(epoch_2_last_block.block());
    // Send new epoch 3 notification, epoch 2 was ended above
    let epoch_3_state = test_assets.test_data_provider.current_epoch_state();
    let ack_rx = test_assets.input_providers.send_new_epoch(epoch_3_state);
    let result = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("Successful step");
    assert!(result);
    assert!(!test_assets.assembler.is_current_epoch_over());
    assert_eq!(test_assets.assembler.current_epoch(), 3);
    InputProviderIfc::check_ack(ack_rx, ack_expected).await;

    // Check store cache does not have entry with 1 and 2 heights
    assert!(!test_assets
        .assembler
        .store
        .has_entry(epoch_3_certified_blocks[0].height()));
    assert!(!test_assets
        .assembler
        .store
        .has_entry(epoch_3_certified_blocks[1].height()));
    // Check that synchronizer context has pending request.
    let sync_request = test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_sync_request()
        .await;
    assert_eq!(
        sync_request,
        RpcServerMessage::Block(test_assets.assembler.next_block_height())
    );
}

#[tokio::test]
async fn check_try_to_sync_skipped_blocks() {
    let mut test_assets = TestAssets::initialize().await;

    // Assembler has genesis block as last executed block,
    // There are no observed blocks yet.
    assert!(test_assets.assembler.last_observed_highest_block.is_none());
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 0);
    // Nothing is posted.
    test_assets
        .assembler
        .try_to_sync_skipped_blocks()
        .await
        .expect("Successful function call");
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;

    // Check that if executed and observed blocks are the same no sync request is posted
    let (certified_blocks, _) = test_assets.test_data_provider.generate_block_chain();
    let block_2 = &certified_blocks[1];
    test_assets.assembler.last_executed_block_info = BlockInfo::from(block_2.block());
    test_assets.assembler.observe_block(block_2);
    // Nothing is posted.
    test_assets
        .assembler
        .try_to_sync_skipped_blocks()
        .await
        .expect("Successful function call");
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;

    // Check that if executed block has lower height than observed block sync request is posted.
    // Observed block height is 5
    test_assets.assembler.observe_block(&certified_blocks[4]);
    // Sync request for block 3 is posted as last_executed_block is 2.
    test_assets
        .assembler
        .try_to_sync_skipped_blocks()
        .await
        .expect("Successful function call");
    let result = test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_sync_request()
        .await;
    assert_eq!(result, RpcServerMessage::Block(3));

    // Check that as there is pending sync request for block 3 no new sync request can be posted via this API
    test_assets.assembler.last_executed_block_info = BlockInfo::from(certified_blocks[2].block());
    // Sync request for block 4 will be tried to be posted as last_executed_block is 3.
    // But as long as sync-context still have block 3 as pending request, the call will fail and nothing will be posted.
    let result = test_assets.assembler.try_to_sync_skipped_blocks().await;
    assert!(result.is_err());
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;
    // Check that if sync-timer triggers, then request for block 3 is still sent out.
    test_assets
        .assembler
        .synchronizer
        .retry()
        .await
        .expect("Successful function call");
    let result = test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_sync_request()
        .await;
    assert_eq!(result, RpcServerMessage::Block(3));

    // Let also check that any incoming block which has lower height than last observed one,
    // does not update the state
    test_assets.assembler.observe_block(block_2);
    assert_eq!(
        test_assets
            .assembler
            .last_observed_highest_block
            .expect("Valid data")
            .height(),
        5
    );
}

#[tokio::test]
async fn check_execute_block() {
    let mut test_assets = TestAssets::initialize().await;
    let (certified_blocks, _) = test_assets.test_data_provider.generate_block_chain();

    // Check that upon successful execution last committed and local last-executed-block-info is updated.
    // Check that also store is cleared from block-reference if any.
    let c_block_1 = &certified_blocks[0];
    test_assets
        .assembler
        .store
        .cache(c_block_1.clone())
        .expect("Block caching is successful");
    assert!(test_assets
        .assembler
        .store
        .get_cache_entry(c_block_1.height())
        .is_some());
    test_assets
        .assembler
        .execute_block(c_block_1)
        .await
        .expect("Successful execution");
    assert_eq!(
        test_assets
            .assembler
            .store
            .last_committed_block()
            .expect("Successful DB read")
            .as_ref(),
        Some(c_block_1)
    );
    assert_eq!(
        test_assets.assembler.last_executed_block_info.header(),
        c_block_1.header()
    );
    assert!(test_assets
        .assembler
        .store
        .get_cache_entry(c_block_1.height())
        .is_none());

    // Let's assume that execution of the block fails.
    // Last committed block is updated, but local states are not updated.
    test_assets.executor_tweaks.fail_execution();
    let c_block_2 = &certified_blocks[1];
    test_assets
        .assembler
        .store
        .cache(c_block_2.clone())
        .expect("Block caching is successful");
    assert!(test_assets.assembler.store.has_entry(c_block_2.height()));
    let exec_result = test_assets.assembler.execute_block(c_block_2).await;
    assert!(matches!(
        exec_result,
        Err(AssemblerError::ExecutionFailed(_))
    ));
    // Although execution fails, but as before we store the block as last committed block, the state is updated
    assert_eq!(
        test_assets
            .assembler
            .store
            .last_committed_block()
            .expect("Successful DB read")
            .as_ref(),
        Some(c_block_2)
    );
    assert_eq!(
        test_assets.assembler.last_executed_block_info.header(),
        c_block_1.header()
    );
    assert!(test_assets.assembler.store.has_entry(c_block_2.height()));

    // let's assume the block triggers epoch-change, check that epoch is marked as done.
    test_assets.executor_tweaks.pass_execution();
    test_assets.executor_tweaks.enable_epoch_change();
    assert!(!test_assets.assembler.is_current_epoch_over());

    test_assets
        .assembler
        .execute_block(c_block_2)
        .await
        .expect("Successful execution");
    assert_eq!(
        test_assets
            .assembler
            .store
            .last_committed_block()
            .expect("Successful DB read")
            .as_ref(),
        Some(c_block_2)
    );
    assert_eq!(
        test_assets.assembler.last_executed_block_info.header(),
        c_block_2.header()
    );
    assert!(test_assets.assembler.is_current_epoch_over());
}
#[tokio::test]
async fn check_handle_scheduled_block() {
    let mut test_assets = TestAssets::initialize().await;
    let (certified_blocks, _) = test_assets.test_data_provider.generate_block_chain();

    // Last executed block is genesis one.
    // Check that if it does not follow the last executed block
    //    - cache entry is cleared from store, and error is reported.
    let c_block_2 = &certified_blocks[1];
    test_assets
        .assembler
        .store
        .cache(c_block_2.clone())
        .expect("Block caching is successful");
    assert!(test_assets.assembler.store.has_entry(c_block_2.height()));
    let exec_result = test_assets
        .assembler
        .handle_scheduled_block(c_block_2, &test_assets.process_data_ref.feedback_tx)
        .await;
    assert!(matches!(exec_result, Err(AssemblerError::Regression(_))));
    assert_eq!(
        test_assets
            .assembler
            .store
            .last_committed_block()
            .expect("Successful DB read")
            .as_ref()
            .unwrap()
            .height(),
        0
    );
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 0);
    assert!(test_assets
        .assembler
        .store
        .get_cache_entry(c_block_2.height())
        .is_none());

    // Check that upon successful execution last committed and local last-executed-block-info is updated.
    // As there is no direct descendant of the executed block cached, no block will be scheduled
    let c_block_1 = &certified_blocks[0];
    test_assets
        .assembler
        .handle_scheduled_block(c_block_1, &test_assets.process_data_ref.feedback_tx)
        .await
        .expect("Successful execution");
    assert_eq!(
        test_assets
            .assembler
            .store
            .last_committed_block()
            .expect("Successful DB read")
            .as_ref(),
        Some(c_block_1)
    );
    assert_eq!(
        test_assets.assembler.last_executed_block_info.header(),
        c_block_1.header()
    );
    test_assets
        .process_data_ref
        .expect_no_scheduled_block()
        .await;

    // Now cache block 3 and schedule block2 for execution. We should expect that block 3 is the next scheduled block for execution.
    let c_block_2 = &certified_blocks[1];
    let c_block_3 = &certified_blocks[2];
    test_assets
        .assembler
        .store
        .cache(c_block_2.clone())
        .expect("Block 2 successfully cached");
    test_assets
        .assembler
        .store
        .cache(c_block_3.clone())
        .expect("Block 3 successfully cached");
    test_assets
        .assembler
        .handle_scheduled_block(c_block_2, &test_assets.process_data_ref.feedback_tx)
        .await
        .expect("Successful execution");
    assert_eq!(
        test_assets.assembler.last_executed_block_info.header(),
        c_block_2.header()
    );
    let scheduled_block = test_assets
        .process_data_ref
        .expect_new_scheduled_block()
        .await;
    assert_eq!(c_block_3, &scheduled_block);
    assert!(test_assets
        .assembler
        .store
        .get_cache_entry(c_block_2.height())
        .is_none());
    let block_3_store_cache_entry = test_assets
        .assembler
        .store
        .get_cache_entry(c_block_3.height())
        .expect("Entry from block 3 exits")
        .as_ref()
        .expect("Valid entry");
    assert!(matches!(
        block_3_store_cache_entry,
        BlockReference::Scheduled(_)
    ));

    // Now cache and observe block_5, and handle block 3 as scheduled one, expect that sync request for block 4 is posted.
    let c_block_5 = &certified_blocks[4];
    test_assets
        .assembler
        .store
        .cache(c_block_5.clone())
        .expect("Block 5 successfully cached");
    test_assets.assembler.observe_block(c_block_5);
    test_assets
        .assembler
        .handle_scheduled_block(c_block_3, &test_assets.process_data_ref.feedback_tx)
        .await
        .expect("Successful execution");
    assert_eq!(
        test_assets.assembler.last_executed_block_info.header(),
        c_block_3.header()
    );
    test_assets
        .process_data_ref
        .expect_no_scheduled_block()
        .await;
    let request = test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_sync_request()
        .await;
    assert_eq!(request, RpcServerMessage::Block(4));

    // Check that if epoch ends then no block is scheduled even there direct descendant cached
    // Send block 4 as scheduled block, meanwhile having block 5 in the store cache.
    let c_block_4 = &certified_blocks[3];
    assert!(test_assets.assembler.store.has_entry(5));
    test_assets.executor_tweaks.enable_epoch_change();
    test_assets
        .assembler
        .handle_scheduled_block(c_block_4, &test_assets.process_data_ref.feedback_tx)
        .await
        .expect("Successful execution");
    assert_eq!(
        test_assets.assembler.last_executed_block_info.header(),
        c_block_4.header()
    );
    test_assets
        .process_data_ref
        .expect_no_scheduled_block()
        .await;
    assert!(test_assets.assembler.store.has_entry(5));
}

#[tokio::test]
async fn check_handle_full_block() {
    let mut test_assets = TestAssets::initialize().await;
    let (certified_blocks, batches) = test_assets.test_data_provider.generate_block_chain();

    // Last executed block is genesis one.
    // Check that if it does not follow the last executed block
    //    - entry is persisted and simply cached
    //    - internal state is not updated
    //    - and sync request for block 1 is posted
    println!("Case1: full-block which is valid but not ready for the execution yet");
    let c_block_2 = &certified_blocks[1];
    // Precondition: If we are already handling the block that means block has been observed.
    test_assets.assembler.observe_block(c_block_2);
    let full_block_2 = prepare_full_block(c_block_2, &batches[1]);
    assert!(!test_assets.assembler.store.has_entry(c_block_2.height()));
    test_assets
        .assembler
        .handle_full_block(full_block_2, &test_assets.process_data_ref.feedback_tx)
        .await
        .expect("Full block is handled without errors");
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 0);
    assert!(test_assets
        .assembler
        .store
        .get_cache_entry(c_block_2.height())
        .is_some());
    assert!(test_assets
        .assembler
        .store
        .has_persisted_block(c_block_2.store_key()));
    batches[1].iter().for_each(|bt| {
        assert!(test_assets
            .assembler
            .store
            .has_persisted_batch(bt.store_key()));
    });
    assert_eq!(
        test_assets
            .assembler
            .store
            .last_committed_block()
            .expect("Successful DB read")
            .expect("Valid entry")
            .height(),
        0
    );
    let sync_request = test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_sync_request()
        .await;
    assert_eq!(sync_request, RpcServerMessage::Block(1));
    println!("Case1: Pass");

    // Now provide block 1 as input,
    //   - block 1 will be successfully executed and
    //   - block 2 will be scheduled for execution. and no sync request will be posted.
    //   - sync context will be cleared as well
    println!("Case2: full-block which is valid but and ready for the execution, and there is also direct descendant available");
    let c_block_1 = &certified_blocks[0];
    let full_block_1 = prepare_full_block(c_block_1, &batches[0]);
    assert!(!test_assets.assembler.store.has_entry(c_block_1.height()));
    test_assets
        .assembler
        .handle_full_block(full_block_1, &test_assets.process_data_ref.feedback_tx)
        .await
        .expect("Full block is handled without errors");
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 1);
    assert!(test_assets
        .assembler
        .store
        .has_persisted_block(c_block_1.store_key()));
    batches[0].iter().for_each(|bt| {
        assert!(test_assets
            .assembler
            .store
            .has_persisted_batch(bt.store_key()));
    });
    assert_eq!(
        test_assets
            .assembler
            .store
            .last_committed_block()
            .expect("Successful DB read")
            .as_ref()
            .expect("Valid entry")
            .height(),
        1
    );
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;
    // Check that sync state is cleared, i.e. no sync request will be posted during retry.
    test_assets
        .assembler
        .synchronizer
        .retry()
        .await
        .expect("Successful sync retry attempt");
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;
    let scheduled_block = test_assets
        .process_data_ref
        .expect_new_scheduled_block()
        .await;
    assert_eq!(&scheduled_block, c_block_2);
    println!("Case2: Pass");

    // Try to send block 1 one more time for execution, it should fail with regression error.
    println!("Case3: full-block which is already executed");
    let full_block_2 = prepare_full_block(c_block_1, &batches[0]);
    let result = test_assets
        .assembler
        .handle_full_block(full_block_2, &test_assets.process_data_ref.feedback_tx)
        .await;
    assert!(matches!(result, Err(AssemblerError::Regression(_))));
    println!("Case3: Pass");
}

#[tokio::test]
async fn check_handle_incoming_block() {
    let port = get_random_port().expect("Failed to get random port");
    let _ = init_runtime_logger(|| Box::new(std::io::sink()), port);
    let mut test_assets = TestAssets::initialize().await;
    let (epoch1_certified_blocks, epoch1_batches) =
        test_assets.test_data_provider.generate_block_chain();

    // Last executed block is genesis one.
    // Check that if input block from current epoch does not follow the last executed block
    //    - entry is persisted and simply cached
    //    - internal state is not updated
    //    - and sync request for block 1 is posted
    // Block with height 2 is posted
    println!("Case1: new-block which is valid but not ready for the execution yet");
    let c_block_2 = &epoch1_certified_blocks[1];
    assert!(!test_assets.assembler.store.has_entry(c_block_2.height()));
    test_assets
        .input_providers
        .send_new_block(c_block_2, &epoch1_batches[1]);
    let main_loop_can_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("New block is handled without errors");
    assert!(main_loop_can_continue);
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 0);
    assert!(test_assets
        .assembler
        .store
        .get_cache_entry(c_block_2.height())
        .is_some());
    assert!(test_assets
        .assembler
        .store
        .has_persisted_block(c_block_2.store_key()));
    epoch1_batches[1].iter().for_each(|bt| {
        assert!(test_assets
            .assembler
            .store
            .has_persisted_batch(bt.store_key()));
    });
    assert_eq!(
        test_assets
            .assembler
            .store
            .last_committed_block()
            .expect("Successful DB read")
            .expect("Valid entry")
            .height(),
        0
    );
    let sync_request = test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_sync_request()
        .await;
    assert_eq!(sync_request, RpcServerMessage::Block(1));
    // check that observed state is updated
    assert_eq!(
        test_assets
            .assembler
            .last_observed_highest_block
            .as_ref()
            .expect("Valid data")
            .height(),
        2
    );
    println!("Case1: Pass");

    // Now provide block 1 as input,
    //   - block 1 will be successfully executed and
    //   - block 2 will be scheduled for execution. and no sync request will be posted.
    //   - sync context will be cleared as well
    println!("Case2: sync-block which is valid  and ready for the execution, and there is direct descendant available");
    let c_block_1 = &epoch1_certified_blocks[0];
    test_assets
        .input_providers
        .send_sync_block(c_block_1, &epoch1_batches[0]);
    let main_loop_can_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("New block is handled without errors");
    assert!(main_loop_can_continue);
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 1);
    assert!(test_assets
        .assembler
        .store
        .has_persisted_block(c_block_1.store_key()));
    epoch1_batches[0].iter().for_each(|bt| {
        assert!(test_assets
            .assembler
            .store
            .has_persisted_batch(bt.store_key()));
    });
    assert_eq!(
        test_assets
            .assembler
            .store
            .last_committed_block()
            .expect("Successful DB read")
            .expect("Valid entry")
            .height(),
        1
    );
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;
    // Check that sync state is cleared, i.e. no sync request will be posted during retry.
    test_assets
        .assembler
        .synchronizer
        .retry()
        .await
        .expect("Successful sync retry attempt");
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;
    let height2_cache_state = test_assets
        .assembler
        .store
        .get_cache_entry(2)
        .expect("Entry is available")
        .as_ref()
        .expect("Valid entry");
    assert!(matches!(height2_cache_state, BlockReference::Scheduled(_)));
    println!("Case2: Pass");

    println!("Case2.5: A new block is from current epoch but does not directly follow the last executed block");
    // the new block will be cached, but no sync request for block 2 will be posted as there is already entry for it in the cache.
    let c_block_3 = &epoch1_certified_blocks[2];
    test_assets
        .input_providers
        .send_new_block(c_block_3, &epoch1_batches[2]);
    let main_loop_can_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("New block is handled without errors");
    assert!(main_loop_can_continue);
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 1);
    // No sync request is sent as there is already entry in the store-cache for block-2
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;
    // check that observed state is updated
    assert_eq!(
        test_assets
            .assembler
            .last_observed_highest_block
            .as_ref()
            .expect("Valid data")
            .height(),
        3
    );
    println!("Case2.5: Pass");

    // Post block 2 which is already scheduled for execution. It will not be executed as it is planned
    // to be executed via feedback channel.
    println!("Case3: new-block which is valid and ready for the execution, but it is internally already scheduled for execution");
    test_assets
        .input_providers
        .send_new_block(c_block_2, &epoch1_batches[1]);
    let main_loop_can_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("New block is handled without errors");
    assert!(main_loop_can_continue);
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 1);
    println!("Case3: Pass");

    // Run step one more time and block-2 will be executed as it was scheduled for execution.
    println!(
        "Case4: Run assembler step to check scheduled block execution which is in feedback-channel"
    );
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 1);
    let main_loop_will_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("New block is handled without errors");
    assert!(main_loop_will_continue);
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 2);
    assert_eq!(
        test_assets
            .assembler
            .last_observed_highest_block
            .as_ref()
            .expect("Valid entry")
            .height(),
        3
    );
    // Run one step to execute block-3 which will be in scheduled state.
    let main_loop_will_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("Scheduled block is handled without errors");
    assert!(main_loop_will_continue);
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 3);
    println!("Case4: Pass");

    // Try to send block 1 one more time for execution, no error is reported, but block is not executed as well
    println!("Case5: A new block is already executed block");
    let c_block_1 = &epoch1_certified_blocks[0];
    test_assets
        .input_providers
        .send_new_block(c_block_1, &epoch1_batches[0]);
    let main_loop_will_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("New block is handled without errors");
    assert!(main_loop_will_continue);
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 3);
    assert_eq!(
        test_assets
            .assembler
            .last_observed_highest_block
            .as_ref()
            .expect("Valid entry")
            .height(),
        3
    );
    println!("Case5: Pass");

    // Provide invalid block, verification fails and fatal error is returned.
    println!("Case6: Invalid block that fails the verification");
    let invalid_c_block_4 = CertifiedBlock::new(
        epoch1_certified_blocks[3].block().clone(),
        epoch1_certified_blocks[0].qc().clone(),
    );
    test_assets
        .input_providers
        .send_sync_block(&invalid_c_block_4, &epoch1_batches[3]);
    let result = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await;
    assert!(matches!(result, Err(AssemblerError::Fatal(_))));
    println!("Case6: Pass");

    // Move test data provider to next epoch and get epoch block data
    let epoch_1_last_block = epoch1_certified_blocks.last().unwrap();
    test_assets
        .test_data_provider
        .move_to_next_epoch(epoch_1_last_block.height());
    let (epoch2_certified_blocks, epoch2_batches) = test_assets
        .test_data_provider
        .generate_block_chain_with_start_height_round_parent(
            epoch_1_last_block.height() + 1,
            epoch_1_last_block.round() + 1,
            epoch_1_last_block.hash(),
        );

    // Provide future epoch block, it will be observed but not executed, and sync request will be
    // posted for block 3 as block 2 is the last executed block.
    // we are still having block-2 as last executed block, but observed block is updated.
    // future epoch block data is not persisted.
    println!("Case7: Future epoch data as a new block");
    let epoch2_c_block_1 = &epoch2_certified_blocks[0];
    test_assets
        .input_providers
        .send_new_block(epoch2_c_block_1, &epoch2_batches[0]);
    let main_loop_will_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("New block is handled without errors");
    assert!(main_loop_will_continue);
    assert_eq!(
        test_assets
            .assembler
            .last_observed_highest_block
            .as_ref()
            .unwrap()
            .height(),
        epoch2_c_block_1.height()
    );
    assert_eq!(test_assets.assembler.last_executed_block_info.height(), 3);
    assert!(!test_assets
        .assembler
        .store
        .has_entry(epoch2_c_block_1.height()));
    assert!(!test_assets
        .assembler
        .store
        .has_persisted_block(epoch2_c_block_1.store_key()));
    epoch2_batches[0].iter().for_each(|bt| {
        assert!(!test_assets
            .assembler
            .store
            .has_persisted_batch(bt.store_key()));
    });
    let sync_request = test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_sync_request()
        .await;
    assert_eq!(sync_request, RpcServerMessage::Block(4));
    // Observed block will be updated
    assert_eq!(
        test_assets
            .assembler
            .last_observed_highest_block
            .as_ref()
            .unwrap()
            .height(),
        epoch2_c_block_1.height()
    );
    println!("Case7: Pass");

    // Feed remaining blocks of epoch 1 and end epoch with the last one.
    // Although we submit also already executed blocks, no error will be reported.
    println!("Case8: Feed with epoch data and finalize the epoch");
    let last_block_height = epoch1_certified_blocks.len() as Height;
    for (idx, c_block) in epoch1_certified_blocks.iter().enumerate().skip(3) {
        if c_block.height() == last_block_height {
            test_assets.executor_tweaks.enable_epoch_change();
        }
        // Alternate between sync block and new block, processing should be the same and not fail
        if idx % 2 == 0 {
            test_assets
                .input_providers
                .send_new_block(c_block, &epoch1_batches[idx]);
        } else {
            test_assets
                .input_providers
                .send_sync_block(c_block, &epoch1_batches[idx]);
        }
        let main_loop_will_continue = test_assets
            .assembler
            .do_step(test_assets.process_data_ref.get())
            .await
            .expect("New block is handled without errors");
        // As long as with previous use-case we have observed block higher than current blocks being submitted,
        // upon each block-processing a sync request for the next expected block will be posted.
        // so here we are consuming sync requests to keep receiver channel clean except for the block that ends-epoch
        // because end-epoch block will terminate thr flow early and no sync block should be expected.
        let c_block_height = c_block.height();
        if c_block_height >= 4 && c_block_height != last_block_height {
            let sync_request = test_assets
                .assembler
                .synchronizer
                .sync_api()
                .expect_sync_request()
                .await;
            assert_eq!(sync_request, RpcServerMessage::Block(c_block.height() + 1));
        }
        assert!(main_loop_will_continue);
    }
    assert!(test_assets.assembler.is_current_epoch_over());
    assert_eq!(
        test_assets.assembler.last_executed_block_info.height(),
        epoch_1_last_block.height()
    );
    assert_eq!(
        test_assets
            .assembler
            .last_observed_highest_block
            .as_ref()
            .unwrap()
            .height(),
        epoch2_c_block_1.height()
    );
    test_assets
        .process_data_ref
        .expect_no_scheduled_block()
        .await;
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;
    test_assets
        .assembler
        .store
        .check_epoch_boundary_info(Some(epoch_1_last_block.qc()));
    println!("Case8: Pass");

    // Check that epoch is marked as over, no scheduled block is available and no block will be accepted at all.
    println!("Case9: a new block in ended epoch state");
    test_assets
        .input_providers
        .send_new_block(epoch2_c_block_1, &epoch2_batches[0]);
    let main_loop_will_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("New block is handled without errors");
    assert!(main_loop_will_continue);
    assert_eq!(
        test_assets.assembler.last_executed_block_info.height(),
        epoch_1_last_block.height()
    );
    // No entry for epoch 2 blocks
    assert!(!test_assets
        .assembler
        .store
        .has_entry(epoch2_c_block_1.height()));
    assert!(!test_assets
        .assembler
        .store
        .has_persisted_block(epoch2_c_block_1.store_key()));
    epoch2_batches[0].iter().for_each(|bt| {
        assert!(!test_assets
            .assembler
            .store
            .has_persisted_batch(bt.store_key()));
    });
    // No Sync request is also sent.
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;
    // No scheduled block
    test_assets
        .process_data_ref
        .expect_no_scheduled_block()
        .await;
    println!("Case9: Pass");

    // Post new epoch data and expect sync request for the epoch2-block1 as it is the last observed block.
    println!("Case10: New epoch notification has received");
    assert_eq!(
        test_assets
            .assembler
            .last_observed_highest_block
            .as_ref()
            .unwrap()
            .height(),
        epoch2_c_block_1.height()
    );
    let new_epoch_state = test_assets.test_data_provider.current_epoch_state();
    let ack_rx = test_assets.input_providers.send_new_epoch(new_epoch_state);
    let main_loop_can_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("Successful new epoch handling");
    assert!(main_loop_can_continue);
    InputProviderIfc::check_ack(ack_rx, true).await;
    assert!(!test_assets.assembler.is_current_epoch_over());
    assert_eq!(test_assets.assembler.current_epoch(), 2);
    let sync_request = test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_sync_request()
        .await;
    assert_eq!(
        sync_request,
        RpcServerMessage::Block(epoch2_c_block_1.height())
    );
    println!("Case10: Pass");

    // Check that new block data will be processes already
    println!("Case11: A new epoch and a new epoch data as input.");
    test_assets
        .input_providers
        .send_new_block(epoch2_c_block_1, &epoch2_batches[0]);
    let main_loop_will_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("New block is handled without errors");
    assert!(main_loop_will_continue);
    assert_eq!(
        test_assets
            .assembler
            .last_observed_highest_block
            .as_ref()
            .unwrap()
            .height(),
        epoch2_c_block_1.height()
    );
    assert_eq!(
        test_assets.assembler.last_executed_block_info.height(),
        epoch2_c_block_1.height()
    );
    assert!(test_assets
        .assembler
        .store
        .has_persisted_block(epoch2_c_block_1.store_key()));
    epoch2_batches[0].iter().for_each(|bt| {
        assert!(test_assets
            .assembler
            .store
            .has_persisted_batch(bt.store_key()));
    });
    println!("Case11: Pass");

    // Check that old epoch data will be ignored and internal state will not be updated.
    // Sync state is clear
    println!("Case12: Old epoch data as input.");
    test_assets
        .assembler
        .synchronizer
        .retry()
        .await
        .expect("Successful retry");
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;
    test_assets
        .input_providers
        .send_sync_block(c_block_1, &epoch1_batches[0]);
    let main_loop_can_continue = test_assets
        .assembler
        .do_step(test_assets.process_data_ref.get())
        .await
        .expect("New block is handled without errors");
    assert!(main_loop_can_continue);
    // Last executed block is intact.
    assert_eq!(
        test_assets.assembler.last_executed_block_info.height(),
        epoch2_c_block_1.height()
    );
    // No sync request is sent
    test_assets
        .assembler
        .synchronizer
        .sync_api()
        .expect_no_sync_request()
        .await;
    // No cache entry is available for epoch1-block1
    assert!(test_assets.assembler.store.get_cache_entry(1).is_none());
    println!("Case12: Pass");
}
