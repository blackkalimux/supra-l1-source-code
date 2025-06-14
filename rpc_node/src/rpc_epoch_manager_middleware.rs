use async_trait::async_trait;
use bytes::Bytes;
use certifier::AsynchronousCertifierNetworkMessage;
use committees::CommitteeAuthorization;
use epoch_manager::{CommitteeCertifierMessage, EpochManagerMessage};
use lifecycle::TEpochId;
use rpc::clients::{RpcClient, SyncRequestApi};
use rpc::messages::RpcServerMessage;
use serde::Serialize;
use socrypto::Identity;
use sop2p::{NetworkSender, NetworkSenderError};
use soserde::SmrDeserialize;
use std::sync::Arc;
use task_manager::{async_shutdown_token, async_task_tracker};
use tokio::{
    sync::mpsc::{error::TrySendError, Receiver, Sender},
    task::Builder,
};
use tracing::{debug, info, warn};

/// An interface between the [EpochManager] run by RPC nodes and the network. Intercepts
/// network messages sent by the [CommitteeInfo] [AsynchronousCertifier] and forwards them
/// to the attached consensus provider via the [RpcClient]. This allows us to use the
/// [AsynchronousCertifier] as a synchronizer for [CommitteeAuthorization]s without having to
/// rework its architecture. We should consider modifying this design to be less ad-hoc in the
/// future as it breaks the contracts implicit in the names of the [NetworkSender] methods.
pub struct RpcEpochManagerMiddleware;

impl RpcEpochManagerMiddleware {
    pub async fn init(
        client: RpcClient,
        rx_committee_auth: Receiver<CommitteeAuthorization>,
        tx_certifier_message: Sender<CommitteeCertifierMessage>,
    ) -> RpcEpochManagerMiddlewareSender {
        let receiver =
            RpcEpochManagerMiddlewareReceiver::new(rx_committee_auth, tx_certifier_message);
        receiver.spawn().await;
        RpcEpochManagerMiddlewareSender { client }
    }
}

struct RpcEpochManagerMiddlewareReceiver {
    rx_committee_auth: Receiver<CommitteeAuthorization>,
    tx_certifier_message: Sender<CommitteeCertifierMessage>,
}

impl RpcEpochManagerMiddlewareReceiver {
    pub fn new(
        rx_committee_auth: Receiver<CommitteeAuthorization>,
        tx_certifier_message: Sender<CommitteeCertifierMessage>,
    ) -> Self {
        Self {
            rx_committee_auth,
            tx_certifier_message,
        }
    }

    pub async fn spawn(mut self) {
        Builder::new()
            .name("rpc_epoch_manager_middleware")
            .spawn(async_task_tracker().track_future(async move {
                self.run().await;
            }))
            .expect("Must be able to spawn the RpcEpochManagerMiddleware");
    }

    async fn run(&mut self) {
        // Notifier to enable graceful shutdown.
        let shutdown_token = async_shutdown_token();

        loop {
            let result = tokio::select! {
                Some(c) = self.rx_committee_auth.recv() => self.handle_committee_authorization(c).await,
                _ = shutdown_token.cancelled() => {
                    debug!("EpochManager received a shutdown notification");
                    break;
                }
            };
            match result {
                Ok(_) => (),
                Err(e) => warn!("{e:?}"),
            }
        }
        info!("RpcEpochManagerMiddleware shutting down.");
    }

    async fn handle_committee_authorization(
        &self,
        authorization: CommitteeAuthorization,
    ) -> Result<(), TrySendError<CommitteeCertifierMessage>> {
        debug!(
            "Received the CommitteeAuthorization for Committee {:?}",
            authorization.committee().hash()
        );
        // Pull the [Certificate<CommitteeInfo>] out of the [CommitteeAuthorization] so that
        // it can be sent to the [AsynchronousCertifier] to allow it to clean up its request log.
        // The [EpochManager] will then reconstruct the [CommitteeAuthorization] when the certifier
        // forwards it to it.
        let c = authorization.certificate().clone();
        let n = AsynchronousCertifierNetworkMessage::Certificate(c);
        let m = CommitteeCertifierMessage::Network(Box::new(n));
        match self.tx_certifier_message.try_send(m) {
            Ok(_) => Ok(()),
            // TODO: Graceful shutdown.
            Err(TrySendError::Closed(_)) => {
                panic!("The CommitteeCertifier terminated unexpectedly.")
            }
            Err(TrySendError::Full(m)) => {
                // We only log a warning here because the system can recover if the channel is
                // eventually drained because of the sync loop in the [AsynchronousCertifier].
                debug!(
                    "Warn failed to deliver a message to the CommitteeCertifier due to \
                    a channel full error: {m:?}"
                );
                Ok(())
            }
        }
    }
}

pub struct RpcEpochManagerMiddlewareSender {
    client: RpcClient,
}

#[async_trait]
impl NetworkSender<Identity> for RpcEpochManagerMiddlewareSender {
    /// Converts the given message, which is expected to be a serialized [EpochManagerMessage],
    /// into an [RpcServerMessage] and sends it via the [RpcClient].
    async fn multicast<Services>(
        &self,
        service_id: Services,
        _receivers: Arc<Vec<Identity>>,
        message: Bytes,
    ) -> Result<(), NetworkSenderError<Identity>>
    where
        Services: Serialize + Send,
    {
        self.broadcast(service_id, message).await
    }

    /// Converts the given message, which is expected to be a serialized [EpochManagerMessage],
    /// into an [RpcServerMessage] and sends it via the [RpcClient].
    async fn broadcast<Services>(
        &self,
        _service_id: Services,
        message: Bytes,
    ) -> Result<(), NetworkSenderError<Identity>>
    where
        Services: Serialize + Send,
    {
        let Ok(m) = EpochManagerMessage::try_from_bytes(message.to_vec().as_slice()) else {
            // TODO: Proper error?
            warn!("Only EpochManagerMessages should be sent via middleware");
            return Ok(());
        };

        let EpochManagerMessage::CommitteeCertifierMessage(m) = m else {
            // TODO: Proper error?
            warn!("RpcEpochManagerMiddleware received an unexpected message: {m:?}");
            return Ok(());
        };

        let epoch_to_sync = match m {
            AsynchronousCertifierNetworkMessage::Certificate(_) => return Ok(()),
            AsynchronousCertifierNetworkMessage::Proposal(p) => p.vote().data().epoch(),
            AsynchronousCertifierNetworkMessage::Vote(v) => v.data().epoch(),
        };

        let sync_request = RpcServerMessage::Authorization(epoch_to_sync);
        debug!("Post request {sync_request:?}");
        match self.client.send(sync_request).await {
            Ok(_) => Ok(()),
            Err(e) => {
                // TODO: Proper error?
                warn!(
                    "Failed to send a sync request for the CommitteeAuthorization for epoch \
                    {epoch_to_sync} due to: {e}"
                );
                Ok(())
            }
        }
    }
}
