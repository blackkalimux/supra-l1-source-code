use crate::consensus::errors::ChainStateAssemblerResult;
use crate::consensus::AssemblerError;
use committees::Height;
use rpc::clients::SyncRequestApi;
use rpc::messages::RpcServerMessage;
use std::time::Duration;
use tokio::time::{interval, Instant, Interval};
use tracing::debug;

/// Encloses synchronization context and provides API to schedule and retry sync requests based on
/// block height.
/// Supports a single sync request at a time.
/// Any sync request that differs from the ongoing one will be reported as error.
pub(crate) struct SynchronizationContext<SyncApi> {
    /// Communication channel to post sync-requests for missing blocks.
    request_sender: SyncApi,
    /// Recent sync request which is not yet resolved.
    pending_sync_request: Option<Height>,
    /// Interval to post the pending sync request.
    sync_timer: Interval,
}

impl<SyncApi: SyncRequestApi> SynchronizationContext<SyncApi> {
    pub(crate) fn new(request_sender: SyncApi, sync_interval_in_secs: u64) -> Self {
        Self {
            request_sender,
            pending_sync_request: None,
            sync_timer: interval(Duration::from_secs(sync_interval_in_secs)),
        }
    }

    pub(crate) async fn next_window(&mut self) -> Instant {
        let res = self.sync_timer.tick().await;
        debug!("Sync timer ticked");
        res
    }

    /// Posts sync request one more time if there is still one pending.
    pub(crate) async fn retry(&mut self) -> ChainStateAssemblerResult<()> {
        if let Some(block_height) = &self.pending_sync_request {
            return self.post_sync_request(*block_height).await;
        }
        Ok(())
    }

    /// Sends block sync request to block-provider for the block at the input height.
    async fn post_sync_request(&mut self, block_height: Height) -> ChainStateAssemblerResult<()> {
        debug!("Post sync request for block at height: {block_height}");
        self.request_sender
            .send(RpcServerMessage::Block(block_height))
            .await?;
        self.sync_timer.reset();
        Ok(())
    }

    /// Tries to post sync request for the block with the input height.
    /// If there is already pending sync request for the same block height a new request is not posted.
    /// If there is already pending sync request for different block height a regression error is reported.
    pub(crate) async fn try_post_sync_request(
        &mut self,
        height: Height,
    ) -> ChainStateAssemblerResult<()> {
        match &self.pending_sync_request {
            None => {
                self.pending_sync_request = Some(height);
                self.post_sync_request(height).await
            }
            Some(ongoing_sync_height) => {
                if ongoing_sync_height == &height {
                    debug!("There is already ongoing sync request for the block at height: {ongoing_sync_height}. Nothing to be done.");
                    Ok(())
                } else {
                    Err(AssemblerError::Fatal(
                        format!(
                            "There is ongoing sync request for block at height: {ongoing_sync_height} \
                             when request for block at height: {height} is posted. \
                             Either sync state is not properly cleared or ChainStateAssembler is in invalid state!")
                    ))
                }
            }
        }
    }

    /// Clears the cached pending sync request if it matches the input height.
    pub(crate) fn clear(&mut self, height: Height) {
        if Some(&height) == self.pending_sync_request.as_ref() {
            self.pending_sync_request = None;
            self.sync_timer.reset()
        }
    }
}

#[cfg(test)]
#[path = "tests/synchronizer.rs"]
pub(crate) mod synchronizer_tests;
