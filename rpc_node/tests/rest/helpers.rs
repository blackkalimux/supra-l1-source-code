#![cfg(test)]

use archive::db::ArchiveDb;
use archive::tests::utilities::new_archive_db_with_cache_size;
use committees::TBlockHeader;
use configurations::rpc::kyc_token::KycToken;
use execution::test_utils::executor::ExecutorTestDataProvider;
use index_storage_ifc::ArchiveWrite;
use mempool::MoveSequenceNumberProvider;
use mocked_transaction_consumer::MockedTransactionConsumer;
use ntex::web::{DefaultError, WebServiceFactory};
use rpc_node::gas::{gas_monitor, AggregatedGasPriceReader};
use rpc_node::rest::api_server_state_builder::ApiServerStateBuilder;
use rpc_node::rest::router::rest_root_route;
use rpc_node::transactions::dispatcher::TransactionDispatcher;
use rpc_node::RpcStorage;
use test_utils::execution_utils;
use test_utils::execution_utils::{ExecutedDataGenerator, TestExecutor};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use types::limiting_strategy::IntervalStrategy;
use types::settings::backlog;
use types::transactions::smr_transaction::SmrTransaction;

/// Set up a test executor and a web service in regular mode.
pub async fn setup_executor_with_web_service(
    with_archive_state: bool,
    consensus_access_tokens: &[KycToken],
    backlog_parameters: &backlog::Parameters,
) -> (
    ExecutorWebServiceResources,
    impl WebServiceFactory<DefaultError> + Sized,
) {
    let executor_with_resources =
        setup_executor_and_web_resources(with_archive_state, backlog_parameters).await;
    setup_web_service(executor_with_resources, consensus_access_tokens).await
}

/// Set  a web service in regular mode utilizing provided web-service resources
pub async fn setup_web_service(
    executor_with_resources: ExecutorWebServiceResources,
    consensus_access_tokens: &[KycToken],
) -> (
    ExecutorWebServiceResources,
    impl WebServiceFactory<DefaultError> + Sized,
) {
    let api_server_state = ApiServerStateBuilder::new()
        .storage(executor_with_resources.rpc_storage.clone())
        .tx_sender(executor_with_resources.txn_dispatcher())
        .move_store(executor_with_resources.executor.move_store.clone())
        .store(executor_with_resources.executor.chain_store.clone())
        .gas_price_reader(executor_with_resources.gas_price_reader.clone())
        .rest_call_limiter(
            IntervalStrategy::from_secs(5)
                .expect("5s is not an empty Duration")
                .into(),
        )
        .build();

    let web_service = rest_root_route(api_server_state, consensus_access_tokens);

    (executor_with_resources, web_service)
}

pub fn setup_test_archive_storage(
    cache_size: usize,
    with_state: bool,
    test_executor: &TestExecutor,
) -> ArchiveDb {
    let archive = new_archive_db_with_cache_size(cache_size, &test_executor.chain_store);

    if with_state {
        // Add some data to the Archive.
        let chain = ExecutorTestDataProvider::default().executed_chain_with_empty_outputs();
        let mut executed_data_generator = ExecutedDataGenerator::new(test_executor, 5);
        let automation_task_qty = 3;
        // Mock 3 automation tasks for account at index 0, starting from 0 index of automation-task
        let automation_tasks =
            executed_data_generator.generate_automation_task_metadata(0, 0, automation_task_qty);
        archive
            .insert_automation_task_data(&automation_tasks)
            .expect("Successful insertion of automation task info");
        // Setup enough fund for account 0 to simulate automated transactions
        executed_data_generator.faucet_account(0, 10_000_000_000);

        for mut block in chain {
            let (automated_txn_meta, txn_outputs) = executed_data_generator
                .generate_executed_automation_data(block.height(), 0, automation_task_qty);
            block.set_automated_transaction_data(automated_txn_meta);
            block.set_automation_outputs(txn_outputs);
            let b = block.finalize().expect("Should have finalized state");
            archive
                .internal_insert_executed_block(b)
                .expect("Must be able to insert data into the test archive");
        }
    }
    archive
}

/// Set up a test executor and resources required to start a web service.
pub async fn setup_executor_and_web_resources(
    with_archive_state: bool,
    backlog_parameters: &backlog::Parameters,
) -> ExecutorWebServiceResources {
    // TODO: Could make this a param if any tests need it.
    let test_cache_size = 100;
    let (tx_txn, rx_tx) = mpsc::channel(1);
    let (_, gas_price_reader) = gas_monitor();
    let executor = execution_utils::setup_executor();
    let tx_sender = MockedTransactionConsumer::new(tx_txn);
    let archive_db = setup_test_archive_storage(test_cache_size, with_archive_state, &executor);
    let rpc_storage = RpcStorage::new(
        archive_db,
        MoveSequenceNumberProvider::new(executor.move_store.clone()),
        backlog_parameters,
    );

    ExecutorWebServiceResources {
        executor,
        tx_consumer: tx_sender,
        rx_tx,
        rpc_storage,
        gas_price_reader,
    }
}

/// A utility type creating all primitives required to execute txs
/// and set up web services.
pub struct ExecutorWebServiceResources {
    pub executor: TestExecutor,
    pub tx_consumer: MockedTransactionConsumer,
    pub rx_tx: Receiver<SmrTransaction>,
    pub rpc_storage: RpcStorage<MoveSequenceNumberProvider>,
    pub gas_price_reader: AggregatedGasPriceReader,
}

impl ExecutorWebServiceResources {
    pub(crate) fn txn_dispatcher(&self) -> TransactionDispatcher {
        TransactionDispatcher::default()
            .register_consumer(self.tx_consumer.clone())
            .register_consumer(self.rpc_storage.clone())
    }
}

pub mod mocked_transaction_consumer {
    use async_trait::async_trait;
    use rpc_node::transactions::dispatcher::{TTransactionConsumer, TransactionDispatchError};
    use socrypto::Digest;
    use std::sync::Arc;
    use tokio::sync::mpsc::Sender;
    use types::transactions::smr_transaction::SmrTransaction;

    pub type PassPredicateType = Arc<Box<dyn Fn(&SmrTransaction) -> bool + Send + Sync + 'static>>;
    #[derive(Clone)]
    pub struct MockedTransactionConsumer {
        sender: Sender<SmrTransaction>,
        pass_predicate: PassPredicateType,
    }

    impl MockedTransactionConsumer {
        pub fn new(sender: Sender<SmrTransaction>) -> Self {
            Self {
                sender,
                pass_predicate: Arc::new(Box::new(pass)),
            }
        }

        pub fn set_pass_predicate<T>(&mut self, predicate: T)
        where
            T: Fn(&SmrTransaction) -> bool + Send + Sync + 'static,
        {
            self.pass_predicate = Arc::new(Box::new(predicate))
        }
    }
    #[async_trait]
    impl TTransactionConsumer for MockedTransactionConsumer {
        async fn consume(&self, txn: SmrTransaction) -> Result<(), TransactionDispatchError> {
            let txn_hash = txn.digest();
            if (self.pass_predicate)(&txn) {
                self.sender.send(txn).await.map_err(|_| {
                    TransactionDispatchError::Fatal(format!(
                        "Failed to send transaction: {txn_hash}"
                    ))
                })
            } else {
                Err(TransactionDispatchError::ConsumptionError(format!(
                    "Configured to fail transaction: {txn_hash}"
                )))
            }
        }
    }

    pub fn pass(_txn: &SmrTransaction) -> bool {
        true
    }

    pub fn fail(_txn: &SmrTransaction) -> bool {
        false
    }
}
