use crate::consensus::block_verifier::TBlockVerifier;
use crate::consensus::errors::{AssemblerError, ChainStateAssemblerResult};
use crate::consensus::synchronizer::SynchronizationContext;
use crate::consensus::{TBlockStore, VerificationResult};
use async_trait::async_trait;
use committees::{BlockInfo, CertifiedBlock, Height, SmrBlock, TBlockHeader, TSmrBlock};
use consistency_warranter::{StateConsistencyWarranterError, TStateConsistencyWarranter};
use epoch_manager::{EpochNotificationAndAckSender, Notification};
use execution::traits::TBlockExecutor;
use lifecycle::TEpochInfo;
use lifecycle_types::{EpochStates, TEpochStates};
use notifier::Ack;
use rpc::clients::SyncRequestApi;
use rpc::messages::RpcBlockData;
use rpc::FullBlock;
use std::fmt::{Debug, Formatter};
use task_manager::{async_shutdown_token, async_task_tracker};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn, Instrument};
use types::executed_block::ExecutedBlock;
use types::impl_struct_create_and_read_methods;

const INTERNAL_FEEDBACK_CHANNEL_SIZE: usize = 1;
/// Encloses the logic to consume certified blocks and reconstruct chain state from them.
/// Upon each block arrival it is
///     - verified
///     - executed if the next expected one
///     - otherwise cached to be executed later
/// When async state is identified, then sync request is posted to the certified block provider
/// for the missing blocks one by one from bottom(the lowest next block that should be executed)
/// to top (the highest block that is missing to form a chain).
/// Only a single sync request is processed at the moment of the time.
///
/// Currently only a single epoch data is handled, which is the data of the current active epoch.
/// Any data from future epochs is ignored and only memo of the latest highest observed block
/// is kept for successful synchronization.
///
/// Also pending blocks for the execution which form a chain sent/scheduled to be executed one by
/// one in sequential order.
pub struct ChainStateAssembler<Storage, BlockVerifier, Executor, SyncApi> {
    /// Communication channel to receive block message from block-provider.
    blocks_rx: Receiver<RpcBlockData>,
    /// Communication channel to receive epoch change notifications.
    epoch_notifications_rx: Receiver<EpochNotificationAndAckSender>,
    /// Executor context to execute valid blocks.
    executor: Executor,
    /// The header and digest of the most recently executed block.
    last_executed_block_info: BlockInfo,
    /// Holds info related to the latest highest observed block from provider.
    last_observed_highest_block: Option<BlockInfo>,
    /// Synchronizer context
    synchronizer: SynchronizationContext<SyncApi>,
    /// Storage for verified and executed blocks as well as for the cached blocks pending to be executed.
    store: Storage,
    /// Block verifier to validate blocks before having them part of the store.
    block_verifier: BlockVerifier,
}

struct InternalProcessData<'a> {
    shutdown_token: &'a CancellationToken,
    ready_to_execute_tx: &'a Sender<CertifiedBlock>,
    ready_to_execute_rx: &'a mut Receiver<CertifiedBlock>,
}

/// Set of input channels that [ChainStateAssembler] accepts external input data/
pub struct DataInputs {
    /// Communication channel to receive block message from block-provider.
    blocks_rx: Receiver<RpcBlockData>,
    /// Communication channel to receive epoch change notifications.
    epoch_notifications_rx: Receiver<EpochNotificationAndAckSender>,
}

impl_struct_create_and_read_methods!(
    DataInputs {
        blocks_rx: Receiver<RpcBlockData>,
        epoch_notifications_rx: Receiver<EpochNotificationAndAckSender>,
    }

);

impl<Storage, BlockVerifier: TBlockVerifier, Executor, SyncApi> TEpochStates
    for ChainStateAssembler<Storage, BlockVerifier, Executor, SyncApi>
{
    fn epoch_states(&self) -> &EpochStates {
        self.block_verifier.epoch_states()
    }
}

#[async_trait]
impl<
        Storage: TBlockStore + Send + Sync + 'static,
        BlockVerifier: TBlockVerifier + Send + Sync + 'static,
        Executor: TBlockExecutor + Send + Sync + 'static,
        SyncApi: SyncRequestApi + Send + Sync + 'static,
    > TStateConsistencyWarranter
    for ChainStateAssembler<Storage, BlockVerifier, Executor, SyncApi>
{
    async fn execute(
        &mut self,
        block: &CertifiedBlock,
    ) -> Result<(), StateConsistencyWarranterError> {
        match self.execute_block(block).await {
            Ok(_) => {
                debug!("Successfully restored consistency between blockchain and Move stores.");
                Ok(())
            }
            Err(AssemblerError::ExecutionFailed(err)) => {
                debug!("Failed to execute block to restore consistency between blockchain and Move stores: {err:?}");
                // ExecutionError can be handled by StateConsistencyWarranter, so we can return it as is.
                Err(StateConsistencyWarranterError::ExecutionError(err))
            }
            Err(err) => {
                debug!("Failed to restore consistency between blockchain and Move stores: {err:?}");
                Err(StateConsistencyWarranterError::FailedToRestoreConsistency(
                    err.to_string(),
                ))
            }
        }
    }

    fn last_committed_block(&self) -> Result<CertifiedBlock, StateConsistencyWarranterError> {
        self.store
            .last_committed_block()
            .map_err(|e| StateConsistencyWarranterError::NoCommittedBlock(Some(format!("{e:?}"))))?
            .ok_or(StateConsistencyWarranterError::NoCommittedBlock(None))
    }

    fn last_executed_block(&self) -> Result<ExecutedBlock, StateConsistencyWarranterError> {
        self.store
            .last_executed_block()
            .map_err(|e| StateConsistencyWarranterError::NoExecutedBlock(Some(format!("{e:?}"))))?
            .ok_or(StateConsistencyWarranterError::NoExecutedBlock(None))
    }
}
impl<Storage, BlockVerifier: TBlockVerifier, Executor, SyncAPI> Debug
    for ChainStateAssembler<Storage, BlockVerifier, Executor, SyncAPI>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainStateAssembler")
            .field("last_executed_block_info", &self.last_executed_block_info)
            .field(
                "last_observed_highest_block",
                &self.last_observed_highest_block,
            )
            .field("epoch_active_state", &self.is_current_epoch_over())
            .finish()
    }
}

impl<
        Storage: TBlockStore + Send + Sync + 'static,
        BlockVerifier: TBlockVerifier + Send + Sync + 'static,
        Executor: TBlockExecutor + Send + Sync + 'static,
        SyncApi: SyncRequestApi + Send + Sync + 'static,
    > ChainStateAssembler<Storage, BlockVerifier, Executor, SyncApi>
{
    pub async fn new(
        inputs: DataInputs,
        store: Storage,
        block_verifier: BlockVerifier,
        executor: Executor,
        sync_api: SyncApi,
        sync_interval_in_secs: u64,
    ) -> ChainStateAssemblerResult<Self> {
        let sync_state = SynchronizationContext::new(sync_api, sync_interval_in_secs);
        let last_executed_block = store
            .last_executed_block()?
            .ok_or_else(|| {
                AssemblerError::InvalidBlockStoreState(
                    "Storage is not properly initialized. Missing ExecutedBlock info.".to_string(),
                )
            })
            .map(|eb| BlockInfo::from(eb.block()))?;

        let DataInputs {
            blocks_rx,
            epoch_notifications_rx,
        } = inputs;
        let mut assembler = Self {
            blocks_rx,
            epoch_notifications_rx,
            executor,
            last_executed_block_info: last_executed_block,
            last_observed_highest_block: None,
            synchronizer: sync_state,
            store,
            block_verifier,
        };

        assembler
            .ensure_blockchain_and_move_stores_are_consistent()
            .await?;

        Ok(assembler)
    }

    pub fn spawn(self) -> JoinHandle<()> {
        task::Builder::new()
            .name("chain_state_assembler")
            .spawn(
                async_task_tracker()
                    .track_future(async move {
                        debug!("Starting ChainStateAssembler.");
                        let result = self.run().await;
                        if result.is_err() {
                            error!("Exit ChainStateAssembler with status {result:?}");
                        } else {
                            info!("Exit ChainStateAssembler with status {result:?}");
                        }
                    })
                    .instrument(tracing::trace_span!("chain_state_assembler")),
            )
            .expect("chain_state_assembler successfully scheduled.")
    }

    /// Starts processing loop of incoming messages.
    pub async fn run(mut self) -> ChainStateAssemblerResult<()> {
        let shutdown_token = async_shutdown_token();
        // Skip the first window.
        self.synchronizer.next_window().await;
        let (ready_to_execute_tx, mut ready_to_execute_rx) =
            channel::<CertifiedBlock>(INTERNAL_FEEDBACK_CHANNEL_SIZE);
        let mut can_do_a_step = true;
        while can_do_a_step {
            let process_data = InternalProcessData {
                shutdown_token: &shutdown_token,
                ready_to_execute_tx: &ready_to_execute_tx,
                ready_to_execute_rx: &mut ready_to_execute_rx,
            };
            let result = self.do_step(process_data).await;
            can_do_a_step = match &result {
                Ok(can_proceed) => *can_proceed,
                Err(AssemblerError::Fatal(_)) => return result.map(drop),
                Err(AssemblerError::ExecutionFailed(_)) => return result.map(drop),
                Err(e) => {
                    error!("{e:?}");
                    true
                }
            }
        }
        Ok(())
    }

    /// Runs a single step of the execution loop according to the next trigger.
    /// Errors are propagated to the main loop.
    /// On successful trigger consumption it returns whether the next steps can be continued.
    async fn do_step(
        &mut self,
        process_data: InternalProcessData<'_>,
    ) -> ChainStateAssemblerResult<bool> {
        let InternalProcessData {
            shutdown_token,
            ready_to_execute_tx,
            ready_to_execute_rx,
        } = process_data;

        tokio::select! {
            // Bias the selection logic to ensure that epoch manager notifications are handled
            // as soon as possible.
            biased;

            maybe_epoch_notification = self.epoch_notifications_rx.recv() => {
                if let Some(epoch_notification) = maybe_epoch_notification {
                    self.handle_epoch_manager_notification(epoch_notification).await?
                } else {
                    info!("Epoch manager is not available.");
                    return Ok(false);
                }
            }
            _ = shutdown_token.cancelled() => {
                debug!("Received shutdown notification");
                return Ok(false);
            }
            msg = self.blocks_rx.recv() => {
                debug!("Received a new block: {msg:?}");
                if let Some(rpc_msg) = msg {
                    self.handle_incoming_block(rpc_msg, ready_to_execute_tx).await?;
                } else {
                    info!("Input block channel channel has been closed.");
                    return Ok(false);
                }
            }
            certified_block = ready_to_execute_rx.recv() => {
                if let Some(ready_block_for_execution) = certified_block {
                    self.handle_scheduled_block(&ready_block_for_execution, ready_to_execute_tx).await?;
                } else {
                    error!("Regression: internal feedback channel has been closed.");
                    return Ok(false);
                }
            }
            _ = self.synchronizer.next_window() => self.synchronizer.retry().await?,
        }
        Ok(true)
    }

    /// Processes incoming messages from epoch manager.
    ///     - EndEpoch notification does not cause any internal state update, as end-epoch should
    ///       have been already identified when epoch boundary block has been executed.
    ///       If internal state does not already reflect epoch-end, then regression error is reported as fatal.
    ///     - NewEpoch notification triggers
    ///         - internal state update of store and block-verifier with new epoch data,
    ///         - current epoch end identification flag is cleared, to resume incoming blocks processing
    ///         - tries to sync skipped blocks if any
    /// For both cases acknowledgement is returned to epoch manager.
    async fn handle_epoch_manager_notification(
        &mut self,
        n: EpochNotificationAndAckSender,
    ) -> ChainStateAssemblerResult<()> {
        let (notification, ack_sender) = n;
        if !self.is_current_epoch_over() {
            return Err(AssemblerError::Fatal(
                "Regression: current epoch end should have been \
                       identified internally when epoch-end block was executed."
                    .to_string(),
            ));
        }
        let result = match notification {
            // Nothing to do.
            Notification::EndEpoch => {
                debug!("ChainStateAssembler received EndEpoch notification.");
                Ok(())
            }
            Notification::NewEpoch(new_epoch_state) => {
                if new_epoch_state.is_epoch_over()
                    || !new_epoch_state.extends(self.current_epoch_state())
                {
                    return Err(AssemblerError::Fatal(
                        format!("Regression: Invalid epoch state, either the new epoch has ended or does not extend \
                                 current epoch. New Epoch State: {new_epoch_state:?}, Current Epoch State: {:?}",
                                self.current_epoch_state())));
                }
                // as long as we are clearing the cache after each block execution, and we are not caching blocks from future epochs,
                // so this clean-up seems redundant here.
                // maybe it makes more sense to check that cache is empty and report an error otherwise,
                // unless we start to support new epoch data caching as well.
                self.store
                    .retain(&new_epoch_state.epoch_info().start_height());
                self.block_verifier.update(*new_epoch_state);
                self.try_to_sync_skipped_blocks().await
            }
        };
        ack_sender
            .send(Ack)
            .expect("Ack to EpochManager must succeed.");
        result
    }

    /// Handles incoming message containing block info.
    /// There is no particular difference in handling of [RpcBlockData::NewBlock] or [RpcBlockData::SyncBlock].
    async fn handle_incoming_block(
        &mut self,
        msg: RpcBlockData,
        ready_to_execute_tx: &Sender<CertifiedBlock>,
    ) -> ChainStateAssemblerResult<()> {
        let full_block: FullBlock = msg.into();
        // Keep a memo of the block.
        self.observe_block(&full_block.base);
        // Check whether the assembler state and block allow the block to be processed further.
        if self.should_be_skipped(full_block.block()) {
            return Ok(());
        }
        match self.block_verifier.verify(&full_block) {
            VerificationResult::Success => {}
            VerificationResult::OldEpochBlock => {
                warn!("Regression: Old blocks should have been filtered out before verification.");
                return Ok(());
            }
            VerificationResult::FutureEpochBlock => {
                debug!("Maybe detected out of sync state! Try to sync");
                return self.try_to_sync_skipped_blocks().await;
            }
            VerificationResult::InvalidBlock(e) => return Err(AssemblerError::Fatal(e)),
        }
        self.handle_full_block(full_block, ready_to_execute_tx)
            .await
    }

    /// Keep the memo of the block as last highest observed block if it has higher height
    /// than the previous observed one.
    fn observe_block(&mut self, certified_block: &CertifiedBlock) {
        if certified_block.height()
            > self
                .last_observed_highest_block
                .as_ref()
                .map(|bi| bi.height())
                .unwrap_or(Height::MIN)
        {
            debug!("Observed a new block: {certified_block:?}");
            self.last_observed_highest_block = Some(BlockInfo::from(certified_block.block()));
        }
    }

    /// Checks whether the input block should be skipped.
    /// The block is skipped if:
    ///     - epoch change is in progress, and assembler is halted.
    ///     - the new block height is lower than the last executed block height.
    ///     - there is already identified block with the same height pending execution.
    fn should_be_skipped(&mut self, block: &SmrBlock) -> bool {
        if self.is_current_epoch_over() {
            info!("ChainStateAssembler is halted. Most probably due to epoch-change-phase. Dropping the block-data: {block:?}.");
            return true;
        }
        if block.height() <= self.last_executed_block_info.height() {
            // Already have a block for this height. This can happen if this RPC node started
            // from a snapshot that was further ahead that its block server.
            debug!(
                "Server is behind. Last executed block: {:?}. New block: {:?}",
                self.last_executed_block_info,
                block.header(),
            );
            return true;
        }
        // Skip block if there is any entry in the store for it.
        self.store.has_entry(block.height())
    }

    /// Process a new block,
    ///  - either store in the cache and post sync request for the possible missing block
    ///  - or execute it immediately and schedule any direct successor for execution.
    async fn handle_full_block(
        &mut self,
        new_block: FullBlock,
        ready_to_execute_tx: &Sender<CertifiedBlock>,
    ) -> ChainStateAssemblerResult<()> {
        debug!(
            "Received a new block: {} - {}",
            new_block.base.hash(),
            new_block.base.header()
        );
        if new_block.base.height() <= self.last_executed_block_info.height() {
            return Err(AssemblerError::Regression(format!(
                "Tried to process block with less or equal height({}) than last executed block({})",
                new_block.base.height(),
                self.last_executed_block_info.height()
            )));
        }
        let block = new_block.base.clone();
        self.store.persist(new_block)?;
        let does_follow_chain = self.does_follow_chain(&block)?;
        // Clear the sync state if we were waiting for this block
        self.synchronizer.clear(block.height());
        if does_follow_chain {
            // Try to execute the chain of blocks starting this block.
            // If there are any pending block which was waiting for this block to be executed,
            // then the direct follower will also be scheduled for execution.
            self.try_execute_block_chain(&block, ready_to_execute_tx)
                .await
        } else {
            // Cache this block to be executed later, and try to post sync request for the block following the last-executed block.
            self.store.cache(block)?;
            self.try_to_sync_skipped_blocks().await
        }
    }

    async fn handle_scheduled_block(
        &mut self,
        certified_block: &CertifiedBlock,
        ready_to_execute_tx: &Sender<CertifiedBlock>,
    ) -> ChainStateAssemblerResult<()> {
        self.synchronizer.clear(certified_block.height());
        if self.is_current_epoch_over() {
            Err(AssemblerError::Regression(
                format!("Trying to execute block when epoch is marked over. Block: {certified_block:?}. {self:?} ")))
        } else if self.does_follow_chain(certified_block)? {
            self.try_execute_block_chain(certified_block, ready_to_execute_tx)
                .await
        } else {
            // Clear the entry from the store.
            self.store.retain(&certified_block.height());
            Err(AssemblerError::Regression(format!(
                "Scheduled cached block which does not follow the chain. \
                   Last executed block height: {}, Height of the block scheduled for execution {}",
                self.last_executed_block_info.height(),
                certified_block.height()
            )))
        }
    }

    /// Checks whether the input block is the direct successor of the last executed block in terms
    /// of height and parent-child relationship.
    /// If the input block is the direct successor of the last executed block by height and later is
    /// not the parent of it, then fatal error is reported.
    fn does_follow_chain(&self, block: &CertifiedBlock) -> ChainStateAssemblerResult<bool> {
        if block.height() == self.next_block_height() {
            (block.parent() == self.last_executed_block_info.hash())
                .then_some(true)
                .ok_or_else(|| AssemblerError::Fatal(
                    format!("The block does not continue the chain as it does not follow parent chain: Expected Parent Info: {}. {block:?}",
                            self.last_executed_block_info)))
        } else {
            Ok(false)
        }
    }

    /// Sends the input block for execution.
    /// If block is not a trigger for epoch change and if the direct successor of it is in pending state
    /// then that block is scheduled for the execution having it sent via feedback channel back as input.
    /// If epoch change is triggered then the epoch boundary block info in store is updated with
    /// the input block data.
    async fn try_execute_block_chain(
        &mut self,
        certified_block: &CertifiedBlock,
        ready_to_execute_tx: &Sender<CertifiedBlock>,
    ) -> ChainStateAssemblerResult<()> {
        self.execute_block(certified_block).await?;
        if self.is_current_epoch_over() {
            debug!("Epoch ended: {self:?}");
            // TODO: this can be part of the execution component as well.
            self.store
                .update_epoch_boundary_block_info(certified_block)?;
            return Ok(());
        }
        let maybe_next_block_ready_to_execute = self
            .store
            .take_next_ready_block_by_height(self.next_block_height())?;
        // Try to schedule any pending block that is ready to be executed.
        if let Some(next_ready_block) = maybe_next_block_ready_to_execute {
            // Current implementation schedules pending for execution one by one.
            // So the next possible pending block will be scheduled/pushed into this channel
            // only after the current one is successfully processed.
            // In case if the flow is updated causing any regression in the mentioned flow,
            // fatal error will be reported to terminate the flow to report any potential deadlock.
            // Note: the size of `the ready_to_execute_tx` channel is 1.
            ready_to_execute_tx.try_send(next_ready_block).map_err(|e| {
                AssemblerError::Fatal(format!(
                    "Failed to send pending block for execution through feedback channel: {e}"
                ))
            })
        } else {
            // If there is no pending ready block to be executed, check whether
            // there are any blocks that have been skipped and try to sync them.
            self.try_to_sync_skipped_blocks().await
        }
    }

    /// Try to post sync request if the last observed block height is greater
    /// than the last executed block height and there is no info available
    /// for the next block to be executed in the store.
    async fn try_to_sync_skipped_blocks(&mut self) -> ChainStateAssemblerResult<()> {
        if let Some(last_observed_block) = self.last_observed_highest_block.as_ref() {
            let next_block_height = self.next_block_height();

            // Post sync request unless there is already block data available in the store for
            // the next block to be executed.
            // This scenario may happen when we are in the middle of the executing already cached
            // chain of the blocks, i.e., N, N+1, N+3, and N+4 has be scheduled for the execution.
            // Meantime [N+m](m > 4) new block has been received and selected to be processed.
            // Then when flow notices that it can not be executed it as it does not follow the latest
            // executed block which is [N+3], and will try to send sync request for [N+4].
            // But if there is already cached info for [N+4] in the store there is no need to send sync request.
            if last_observed_block.height() >= next_block_height
                && !self.store.has_entry(next_block_height)
            {
                debug!("Sync block at height: {next_block_height}. {self:?}");
                self.synchronizer
                    .try_post_sync_request(next_block_height)
                    .await?;
            }
        }
        Ok(())
    }

    /// Executes the block and updates internal state.
    /// The last committed block state in store is updated with the input block information.
    /// After successful execution local last executed block info is updated.
    /// Store in-memory state is updated having any info related to it exists.
    /// The internal flag of epoch end is updated according to the epoch change flag returned by execution component.
    async fn execute_block(
        &mut self,
        certified_block: &CertifiedBlock,
    ) -> ChainStateAssemblerResult<()> {
        self.store
            .update_last_committed_block_info(certified_block)?;
        let (epoch_changed, _) = self
            .executor
            .execute_block(
                certified_block,
                self.block_verifier.current_validator_committee(),
            )
            .await?;
        self.handle_epoch_change(epoch_changed);
        self.last_executed_block_info = BlockInfo::from(certified_block.block());
        self.store.retain(&certified_block.height());
        Ok(())
    }

    /// Returns the next block height expected to be executed.
    fn next_block_height(&self) -> Height {
        self.last_executed_block_info.height() + 1
    }

    /// Handles epoch-change feedback from executor.
    /// If epoch change is reported, current epoch is marked and ended, otherwise nothing is done.
    fn handle_epoch_change(&mut self, epoch_changed: bool) {
        if epoch_changed {
            self.block_verifier.end_current_epoch()
        }
    }

    /// Checks whether the current epoch is over.
    fn is_current_epoch_over(&self) -> bool {
        self.current_epoch_state().is_epoch_over()
    }
}

#[cfg(test)]
#[path = "tests/chain_state_assembler.rs"]
mod chain_state_assembler_tests;
