#![allow(clippy::unwrap_used)]

use crate::consensus::synchronizer::SynchronizationContext;
use async_trait::async_trait;
use rpc::clients::RpcClientError;
use rpc::clients::RpcWsClientError;
use rpc::clients::SyncRequestApi;
use rpc::messages::RpcServerMessage;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::timeout;

pub(crate) struct MockSyncApi {
    rx: Receiver<RpcServerMessage>,
    tx: Sender<RpcServerMessage>,
}

impl MockSyncApi {
    pub fn new(size: usize) -> Self {
        let (tx, rx) = channel(size);
        Self { rx, tx }
    }

    pub(crate) async fn expect_sync_request(&mut self) -> RpcServerMessage {
        let res = timeout(Duration::from_secs(2), self.rx.recv()).await;
        assert!(res.is_ok());
        res.unwrap().expect("Channel had available data")
    }

    pub(crate) async fn expect_no_sync_request(&mut self) {
        let res = timeout(Duration::from_secs(1), self.rx.recv()).await;
        assert!(res.is_err())
    }
}

#[async_trait]
impl SyncRequestApi for MockSyncApi {
    async fn send(&self, req: RpcServerMessage) -> Result<(), RpcClientError> {
        self.tx.send(req).await.map_err(|_| {
            RpcClientError::RpcWsClientError(RpcWsClientError::ChannelClosedError(
                "Failed to send message".to_string(),
            ))
        })
    }

    fn is_closed(&self) -> bool {
        false
    }
}

#[cfg(test)]
impl<SyncApi: SyncRequestApi> SynchronizationContext<SyncApi> {
    pub(crate) fn sync_api(&mut self) -> &mut SyncApi {
        &mut self.request_sender
    }
}

#[tokio::test]
async fn check_synchronizer_api() {
    let sync_api = MockSyncApi::new(10);
    let sync_retry_in_secs = 5;
    let mut synchronizer = SynchronizationContext::new(sync_api, sync_retry_in_secs);

    // skip the first window, and check the next
    synchronizer.next_window().await;
    let result = timeout(
        Duration::from_secs(sync_retry_in_secs + 1),
        synchronizer.next_window(),
    )
    .await;
    assert!(result.is_ok());

    // check the post sync request when no sync request on going.
    synchronizer
        .try_post_sync_request(1)
        .await
        .expect("Successful send");
    assert_eq!(synchronizer.pending_sync_request, Some(1));
    let res = synchronizer.request_sender.expect_sync_request().await;
    assert_eq!(res, RpcServerMessage::Block(1));

    // check the post sync request for the same height as an ongoing sync request ,
    // the call is success but no new request is posted
    synchronizer
        .try_post_sync_request(1)
        .await
        .expect("Successful call.");
    assert_eq!(synchronizer.pending_sync_request, Some(1));
    synchronizer.request_sender.expect_no_sync_request().await;

    // check the post sync request for other block height than an ongoing sync request
    // the call fails with error and no sync request is posted.
    let result = synchronizer.try_post_sync_request(2).await;
    assert!(result.is_err());
    assert_eq!(synchronizer.pending_sync_request, Some(1));
    synchronizer.request_sender.expect_no_sync_request().await;

    // retry to send the pending sync request
    synchronizer.retry().await.expect("Successful send");
    assert_eq!(synchronizer.pending_sync_request, Some(1));
    let res = synchronizer.request_sender.expect_sync_request().await;
    assert_eq!(res, RpcServerMessage::Block(1));

    // Try to clear sync request with not-matching pending-request.
    synchronizer.clear(2);
    assert_eq!(synchronizer.pending_sync_request, Some(1));

    // Try to clear sync request with matching pending-request.
    synchronizer.clear(1);
    assert!(synchronizer.pending_sync_request.is_none());

    // retry without pending sync request
    synchronizer.retry().await.expect("Successful call");
    assert!(synchronizer.pending_sync_request.is_none());
    synchronizer.request_sender.expect_no_sync_request().await;

    // check the post sync request one more time
    synchronizer
        .try_post_sync_request(2)
        .await
        .expect("Successful call.");
    assert_eq!(synchronizer.pending_sync_request, Some(2));
    let res = synchronizer.request_sender.expect_sync_request().await;
    assert_eq!(res, RpcServerMessage::Block(2));
}
