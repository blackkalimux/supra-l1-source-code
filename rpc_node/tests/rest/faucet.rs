use crate::rest::common::insert_magic_block;
use crate::rest::helpers::{setup_executor_and_web_resources, ExecutorWebServiceResources};
use aptos_types::account_config;
use aptos_types::account_config::CoinStoreResource;
use aptos_types::transaction::Transaction;
use execution::{MoveExecutor, MoveStore};
use mempool::MoveSequenceNumberProvider;
use move_core_types::account_address::AccountAddress;
use ntex::http::StatusCode;
use ntex::web;
use ntex::web::test;
use rocksstore::schema::SchemaBatchWriter;
use rpc_node::rest::api_server_state_builder::ApiServerStateBuilder;
use rpc_node::rest::faucet::Faucet;
use rpc_node::rest::faucet_call_limiter::FaucetCallLimiter;
use rpc_node::rest::router::{default_route, rest_root_route};
use rpc_node::transactions::dispatcher::TransactionDispatcher;
use rpc_node::RpcStorage;
use socrypto::Digest;
use std::time::Duration;
use supra_logger::LevelFilter;
use test_utils::execution_utils::execute_move_vm;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;
use tokio::time;
use tracing::{debug, trace};
use transactions::{TTransactionHeaderProperties, TxExecutionStatus};
use types::api::v1::{FaucetStatus, TransactionInfoV1};
use types::settings::constants::{
    DEFAULT_COOL_DOWN_PERIOD_FOR_FAUCET_IN_SECONDS, QUANTS_GRANTED_PER_REQUEST,
};
use types::transactions::smr_transaction::SmrTransaction;

#[ntex::test]
/// Test faucet API:
/// - test creating a bunch of delegate faucets,
/// - test funding account with that bunch of faucets.
async fn test_faucet_api() -> anyhow::Result<()> {
    let _ = supra_logger::init_default_logger(LevelFilter::OFF);

    debug!("Setting up resources");
    let resources = setup_executor_and_web_resources(false, &Default::default()).await;
    let tx_sender = resources.txn_dispatcher();
    let ExecutorWebServiceResources {
        executor,
        rx_tx,
        rpc_storage,
        gas_price_reader,
        ..
    } = resources;

    let _smr_tx_processor = spawn_smr_tx_processor(
        rpc_storage.clone(),
        executor.move_store.clone(),
        executor.move_executor.clone(),
        rx_tx,
    );

    let num_minters = 1;
    let minter_accounts = Faucet::prepare(
        num_minters,
        tx_sender.clone(),
        rpc_storage.archive().reader(),
        executor.move_store.clone(),
    )
    .await;

    debug!("Minter accounts {minter_accounts:#?}");

    // Check that the required number of delegate faucets has been created.
    assert_eq!(minter_accounts.len(), num_minters);

    let faucet = Faucet::new(
        &minter_accounts,
        tx_sender.clone(),
        rpc_storage.archive().reader(),
        executor.move_store.clone(),
        QUANTS_GRANTED_PER_REQUEST,
    );

    let max_faucet_requests_per_account = 100;

    let api_server_state = ApiServerStateBuilder::new()
        .storage(rpc_storage.clone())
        .tx_sender(tx_sender)
        .move_store(executor.move_store.clone())
        .store(executor.chain_store.clone())
        .gas_price_reader(gas_price_reader)
        .faucet(
            faucet,
            FaucetCallLimiter::new(
                max_faucet_requests_per_account,
                DEFAULT_COOL_DOWN_PERIOD_FOR_FAUCET_IN_SECONDS,
            ),
        )
        .build();

    let web_service = rest_root_route(api_server_state, &[]);

    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    let pipeline = test::init_service(rpc_app).await;

    // Submit a funding request.
    let funded_account = AccountAddress::random();
    let uri = format!(
        "/rpc/v1/wallet/faucet/{}",
        funded_account.to_canonical_string()
    );
    debug!("Submitting a funding request to {uri}");
    let req = test::TestRequest::get().uri(&uri).to_request();
    let response = test::read_response_json(&pipeline, req).await;
    let FaucetStatus::Accepted(tx_hash) = response else {
        anyhow::bail!("Response is expected to be `Accepted`. Got {response:?}");
    };
    debug!("Funding tx hash: {tx_hash}");

    let uri = format!("/rpc/v1/wallet/faucet/transactions/{tx_hash}",);

    // insert a block for testing
    let mut writer = SchemaBatchWriter::default();
    let block_header_info = insert_magic_block(rpc_storage.archive(), &mut writer);
    rpc_storage
        .archive()
        .insert_transaction_hash_to_block_hash_mapping(
            tx_hash,
            block_header_info.hash,
            &mut writer,
        )?;
    writer.write_all()?;

    // The associated tx must be accounted in the local store.
    // Do it in the loop to account for processing time.
    time::timeout(Duration::from_secs(10), async move {
        loop {
            let Some(_tx_data) = rpc_storage.archive().reader().get_transaction(tx_hash)? else {
                // Tx has not yet arrived in the store.
                trace!("Tx `{tx_hash}` has not found. Waiting...");
                time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            debug!("Funding tx `tx_hash` has been found");
            break;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;

    debug!("Submitting a request to {uri}");
    let req = test::TestRequest::get().uri(&uri).to_request();
    let response: Option<TransactionInfoV1> = test::read_response_json(&pipeline, req).await;
    debug!("Faucet tx status {response:?}");
    assert!(matches!(response, Some(tx_info) if *tx_info.hash()==tx_hash));

    // Funded account has to receive funds.
    // Do it in the loop to account for processing time.
    time::timeout(Duration::from_secs(10), async move {
        loop {
            let account_balance = executor
                .move_store
                .read_resource::<CoinStoreResource>(&funded_account).map(|coin_store| coin_store.coin());

            if let Some(account_balance) = account_balance {
                // Funded account is always a new account.
                // It is created and funded by a single tx,
                // therefore, its balance must be non-zero at creation time.
                debug!("Account {funded_account} holds {account_balance}");
                assert_eq!(account_balance, QUANTS_GRANTED_PER_REQUEST, "Account {funded_account} is expected to be funded");
                break;
            } else {
                trace!("Account resource for `{funded_account}` has been read but not found. Waiting...");
                time::sleep(Duration::from_secs(1)).await;
            }
        }
    })
        .await?;

    Ok(())
}

#[ntex::test]
/// Test faucet rate limiting.
async fn test_faucet_rate_limiting_with_1_minter() -> anyhow::Result<()> {
    let _ = supra_logger::init_default_logger(LevelFilter::OFF);

    debug!("Setting up resources");
    let resources = setup_executor_and_web_resources(false, &Default::default()).await;
    let tx_sender = resources.txn_dispatcher();
    let ExecutorWebServiceResources {
        executor,
        rpc_storage,
        rx_tx,
        gas_price_reader,
        ..
    } = resources;

    let _smr_tx_processor = spawn_smr_tx_processor(
        rpc_storage.clone(),
        executor.move_store.clone(),
        executor.move_executor.clone(),
        rx_tx,
    );

    let num_minters = 1;
    let minter_accounts = Faucet::prepare(
        num_minters,
        tx_sender.clone(),
        rpc_storage.archive().reader(),
        executor.move_store.clone(),
    )
    .await;

    debug!("Minter accounts {minter_accounts:#?}");

    // Check that the required number of delegate faucets has been created.
    assert_eq!(minter_accounts.len(), num_minters);

    let faucet = Faucet::new(
        &minter_accounts,
        tx_sender.clone(),
        rpc_storage.archive().reader(),
        executor.move_store.clone(),
        QUANTS_GRANTED_PER_REQUEST,
    );

    let max_faucet_requests_per_account = 2;
    let cooldown_period_secs = 2;

    let api_server_state = ApiServerStateBuilder::new()
        .storage(rpc_storage)
        .tx_sender(tx_sender)
        .move_store(executor.move_store)
        .store(executor.chain_store)
        .gas_price_reader(gas_price_reader)
        .faucet(
            faucet,
            FaucetCallLimiter::new(max_faucet_requests_per_account, cooldown_period_secs),
        )
        .build();

    let web_service = rest_root_route(api_server_state, &[]);

    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    let pipeline = test::init_service(rpc_app).await;

    // Submit a funding request.
    let funded_account = AccountAddress::random();
    let uri = format!(
        "/rpc/v1/wallet/faucet/{}",
        funded_account.to_canonical_string()
    );
    debug!("Submitting a funding request to {uri}");
    let req = test::TestRequest::get().uri(&uri).to_request();
    let resp: FaucetStatus = test::read_response_json(&pipeline, req).await;
    assert!(
        matches!(resp, FaucetStatus::Accepted(_)),
        "Accepted expected"
    );

    debug!("Re-submitting a funding request to {uri} BEFORE the cooldown period has elapsed");
    let req = test::TestRequest::get().uri(&uri).to_request();
    let resp = pipeline.call(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Request must be rejected with TOO_MANY_REQUESTS status"
    );

    trace!("Waiting `cooldown_period_secs` = {cooldown_period_secs}s to re-submit the request",);
    time::sleep(Duration::from_secs(cooldown_period_secs)).await;

    // Submit again.
    debug!("Re-submitting a funding request to {uri} AFTER the cooldown period has elapsed");
    let req = test::TestRequest::get().uri(&uri).to_request();
    let resp: FaucetStatus = test::read_response_json(&pipeline, req).await;
    assert!(
        matches!(resp, FaucetStatus::Accepted(_)),
        "Request must succeed: this is a second request submitted within rate limitations"
    );

    trace!("Waiting `cooldown_period_secs` = {cooldown_period_secs}s to re-submit the request",);
    time::sleep(Duration::from_secs(cooldown_period_secs)).await;

    // Submit again.
    debug!("Submitting a funding request to {uri} more times than allowed");
    let req = test::TestRequest::get().uri(&uri).to_request();
    let resp = pipeline.call(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Request must be rejected with TOO_MANY_REQUESTS status"
    );

    Ok(())
}

#[ntex::test]
/// Test the faucet API fails if the faucet tx cannot be passed to the validator.
/// The test creates the minter accounts, but simulates a disconnection with the validator,
/// when a faucet request comes in.
async fn test_faucet_failure_api() {
    debug!("Setting up resources");
    let ExecutorWebServiceResources {
        executor,
        mut rx_tx,
        rpc_storage,
        mut tx_consumer,
        gas_price_reader,
        ..
    } = setup_executor_and_web_resources(false, &Default::default()).await;
    tx_consumer.set_pass_predicate(
        move |txn: &SmrTransaction| {
            let tx_hash = txn.digest();
            let tx = txn.move_transaction().expect("Must be move transaction");

            let sender_account = tx.sender();
            let root_account_address = account_config::aptos_test_root_address();

            trace!("Received signed tx `{tx_hash:?}` from {sender_account}");

            if sender_account != root_account_address && tx.sequence_number() != 0 {
                // Skip processing tx from non-root accounts except from the very first one.
                // This includes the minter accounts.
                debug!("Tx IS NOT submitted by the root account. Sequence number = {}. Ignore it intentionally.", tx.sequence_number());
                false
            } else {
                true
            }
        },
    );
    let tx_sender = TransactionDispatcher::default().register_consumer(tx_consumer);

    let _smr_tx_processor = {
        let move_store = executor.move_store.clone();
        let executor = executor.move_executor.clone();
        let archive_db = rpc_storage.archive().clone();

        tokio::spawn(async move {
            while let Some(smr_tx) = rx_tx.recv().await {
                let tx = smr_tx.move_transaction().expect("Must be move transaction");

                let tx = Transaction::UserTransaction(tx.clone());
                execute_move_vm(&executor, &move_store, vec![tx]);

                // Give some time for the tx to be registered as [TxExecutionStatus::Unexecuted].
                // It may happen that the tx will be marked
                time::sleep(Duration::from_millis(100)).await;

                // There is no real validator, so just update the database.
                let mut batch = SchemaBatchWriter::default();
                archive_db
                    .insert_transaction(&mut batch, &smr_tx, TxExecutionStatus::Success)
                    .expect("Must insert");
                batch.write_all().unwrap();
            }
        })
    };

    let num_minters = 1;
    let minter_accounts = Faucet::prepare(
        num_minters,
        tx_sender.clone(),
        rpc_storage.archive().reader(),
        executor.move_store.clone(),
    )
    .await;

    debug!("Minter accounts {minter_accounts:#?}");

    // Check that the required number of delegate faucets has been created.
    assert_eq!(minter_accounts.len(), num_minters);

    let faucet = Faucet::new(
        &minter_accounts,
        tx_sender.clone(),
        rpc_storage.archive().reader(),
        executor.move_store.clone(),
        QUANTS_GRANTED_PER_REQUEST,
    );

    let max_faucet_requests_per_account = 100;

    let api_server_state = ApiServerStateBuilder::new()
        .storage(rpc_storage)
        .tx_sender(tx_sender.clone())
        .move_store(executor.move_store)
        .store(executor.chain_store)
        .gas_price_reader(gas_price_reader)
        .faucet(
            faucet,
            FaucetCallLimiter::new(
                max_faucet_requests_per_account,
                DEFAULT_COOL_DOWN_PERIOD_FOR_FAUCET_IN_SECONDS,
            ),
        )
        .build();

    let web_service = rest_root_route(api_server_state, &[]);

    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    let pipeline = test::init_service(rpc_app).await;

    // Submit a funding request.
    let funded_account = AccountAddress::random();
    let uri = format!(
        "/rpc/v1/wallet/faucet/{}",
        funded_account.to_canonical_string()
    );
    debug!("Submitting a funding request to {uri}");
    let req = test::TestRequest::get().uri(&uri).to_request();

    let response: FaucetStatus = test::read_response_json(&pipeline, req).await;

    debug!("Response {response:?}");

    assert!(matches!(response, FaucetStatus::TryLater));
}

fn spawn_smr_tx_processor(
    rpc_storage: RpcStorage<MoveSequenceNumberProvider>,
    move_store: MoveStore,
    move_executor: MoveExecutor,
    mut transaction_rx: Receiver<SmrTransaction>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(smr_tx) = transaction_rx.recv().await {
            let tx = smr_tx.move_transaction().expect("Must be move transaction");

            let sender_account = tx.sender();
            let tx_hash = smr_tx.digest();

            trace!("Received signed tx `{tx_hash:?}` from {sender_account}",);

            let tx = Transaction::UserTransaction(tx.clone());
            execute_move_vm(&move_executor, &move_store, vec![tx]);

            // Give some time for the tx to be registered as [TxExecutionStatus::Pending].
            time::sleep(Duration::from_millis(100)).await;

            // There is no real validator, thus **manually**
            // - update the database.
            // - remove the tx from the backlog

            let mut batch = SchemaBatchWriter::default();
            rpc_storage
                .archive()
                .insert_transaction(&mut batch, &smr_tx, TxExecutionStatus::Success)
                .expect("Must insert");

            batch.write_all().unwrap();

            rpc_storage.expire(smr_tx.expiration_timestamp());
        }
    })
}
