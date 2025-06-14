use async_trait::async_trait;
use core::fmt::Debug;
use rpc::clients::{RpcClient, RpcClientError, RpcWsClientError, SyncRequestApi};
use rpc::messages::RpcServerMessage;
use socrypto::{Digest, Hash};
use std::sync::Arc;
use thiserror::Error;
use types::transactions::smr_transaction::SmrTransaction;

/// Generic interface to consume the incoming transaction.
#[async_trait]
pub trait TTransactionConsumer {
    async fn consume(&self, txn: SmrTransaction) -> Result<(), TransactionDispatchError>;
}

/// Generic type supporting transaction consumption.
pub type ConsumerType = Arc<Box<dyn TTransactionConsumer + Send + Sync + 'static>>;

/// TransactionDispatcher providing means to forward incoming transactions to registered consumers.
/// Transaction will be forwarded to the registered consumers in the same order as they were registered.
/// If any of the consumers fails forwarding will be interrupted and error will be reported.
#[derive(Clone, Default)]
pub struct TransactionDispatcher {
    delegates: Vec<ConsumerType>,
}

impl TransactionDispatcher {
    /// Registers an [SmrTransaction] consumer.
    /// Consumers are _ordered_ and will be called in order they were added.
    pub fn register_consumer<T>(mut self, delegate: T) -> Self
    where
        T: TTransactionConsumer + Send + Sync + 'static,
    {
        self.delegates.push(Arc::new(Box::new(delegate)));
        self
    }

    pub async fn send(&self, txn: SmrTransaction) -> Result<Hash, TransactionDispatchError> {
        let hash = txn.digest();
        for delegate in &self.delegates {
            delegate.consume(txn.clone()).await?
        }
        Ok(hash)
    }
}

#[async_trait]
impl TTransactionConsumer for RpcClient {
    async fn consume(&self, txn: SmrTransaction) -> Result<(), TransactionDispatchError> {
        self.send(RpcServerMessage::SendTxn(txn.clone()))
            .await
            .map_err(|e| {
                match &e {
                    RpcClientError::RpcWsClientError(RpcWsClientError::NotConnected) => {
                        TransactionDispatchError::Fatal(format!("{e:?}"))
                    }
                    RpcClientError::RpcWsClientError(_) |
                    // TODO: When rpc-rest-errors are properly forwarded to caller, revisit this
                    //       logic and see if any of them can be reported as fatal or not
                    RpcClientError::RpcRestClientError(_) => {
                        TransactionDispatchError::ConsumptionError(format!("{e:?}"))
                    }
                }
            })
    }
}

#[derive(Debug, Error)]
pub enum TransactionDispatchError {
    #[error("Failed to consume: {0}")]
    ConsumptionError(String),
    #[error("Fatal: {0}")]
    Fatal(String),
}

#[cfg(test)]
mod tests {
    use crate::transactions::dispatcher::{
        TTransactionConsumer, TransactionDispatchError, TransactionDispatcher,
    };
    use async_trait::async_trait;
    use socrypto::Digest;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use transactions::SmrTransactionProtocol;
    use types::tests::utils::transaction_maker::{
        generate_smr_transaction, EXPIRATION_DURATION_IN_SECS,
    };
    use types::transactions::smr_transaction::SmrTransaction;

    #[derive(Clone)]
    struct Consumer {
        called: Arc<AtomicBool>,
        fail: bool,
    }

    impl Consumer {
        fn with_failire() -> Self {
            Self {
                called: Default::default(),
                fail: true,
            }
        }
        fn valid() -> Self {
            Self {
                called: Default::default(),
                fail: false,
            }
        }

        fn assert_called(&self) {
            assert!(self.called.load(Ordering::Relaxed));
        }
        fn assert_not_called(&self) {
            assert!(!self.called.load(Ordering::Relaxed));
        }
    }

    #[async_trait]
    impl TTransactionConsumer for Consumer {
        async fn consume(&self, _txn: SmrTransaction) -> Result<(), TransactionDispatchError> {
            if self.fail {
                return Err(TransactionDispatchError::ConsumptionError(
                    "Configured to fail".to_string(),
                ));
            }
            self.called.store(true, Ordering::Relaxed);
            Ok(())
        }
    }
    #[tokio::test]
    async fn check_dispatcher_send() {
        // Check the case when all are successful
        let consumer1 = Consumer::valid();
        let consumer2 = Consumer::valid();
        let dispatcher = TransactionDispatcher::default()
            .register_consumer(consumer1.clone())
            .register_consumer(consumer2.clone());
        let txn =
            generate_smr_transaction(SmrTransactionProtocol::Move, 1, EXPIRATION_DURATION_IN_SECS);
        let hash_feedback = dispatcher.send(txn.clone()).await.expect("Successful send");
        assert_eq!(hash_feedback, txn.digest());
        consumer1.assert_called();
        consumer2.assert_called();

        // Check the case when the very fist one fails.
        let consumer1 = Consumer::with_failire();
        let consumer2 = Consumer::valid();
        let dispatcher = TransactionDispatcher::default()
            .register_consumer(consumer1.clone())
            .register_consumer(consumer2.clone());
        let res = dispatcher.send(txn.clone()).await;
        assert!(res.is_err());
        consumer1.assert_not_called();
        consumer2.assert_not_called();

        // Check the case when the second one.
        let consumer1 = Consumer::valid();
        let consumer2 = Consumer::with_failire();
        let dispatcher = TransactionDispatcher::default()
            .register_consumer(consumer1.clone())
            .register_consumer(consumer2.clone());
        let res = dispatcher.send(txn.clone()).await;
        assert!(res.is_err());
        consumer1.assert_called();
        consumer2.assert_not_called();
    }
}
