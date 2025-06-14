#![allow(deprecated)] // To support API v1.

use crate::rest::block;
use crate::rest::block::get_block_transactions_v3;
use crate::rest::helpers::mocked_transaction_consumer;
use crate::rest::helpers::{
    setup_executor_and_web_resources, setup_executor_with_web_service, setup_web_service,
    ExecutorWebServiceResources,
};
use aptos_types::transaction::{ExecutionStatus, Transaction, TransactionStatus};
use committees::BlockHeader;
use futures::{Future, StreamExt};
use lifecycle::View;
use ntex::http::body::Body;
use ntex::http::Request;
use ntex::web::WebResponse;
use ntex::{
    http,
    web::{self, test},
    Pipeline, Service,
};
use rocksstore::schema::SchemaBatchWriter;
use rpc_node::error::ErrResp;
use rpc_node::rest::router::default_route;
use serde::de::DeserializeOwned;
use smr_timestamp::SmrTimestamp;
use socrypto::{Digest, Identity};
use socrypto::{Hash, HASH_LENGTH};
use soserde::SmrSerialize;
use std::{str::from_utf8, sync::Arc};
use supra_logger::LevelFilter;
use test_utils::execution_utils::{create_mint_transaction, execute_move_vm};
use tokio::sync::{mpsc, Notify};
use tracing::debug;
use transactions::{TTransactionHeader, TTransactionHeaderProperties, TxExecutionStatus};
use types::settings::backlog;
use types::settings::constants::LOCALNET_CHAIN_ID;
use types::{
    api::v1::{
        self, MoveTransactionOutput, TransactionAuthenticator, TransactionBlockInfo,
        TransactionOutput, TransactionParameters,
    },
    api::v2::{self, BlockHeaderInfo},
    api::v3,
    transactions::{
        smr_transaction::SmrMoveTransaction, transaction::SupraTransaction,
        transaction_output::TransactionOutput as SerializedOutput,
        MoveTransactionOutput as SerializedMoveOutput, MoveVmExecutionStatus,
    },
};

/// Ensures that the transaction-execution-status-by-transaction-hash index is not updated with
/// the results of attempted re-executions after a transaction has been finalized. This index
/// must always store the status emitted when the transaction was finalized. We will have a
/// separate index for storing the results of re-executions (all of which should return
/// [TxExecutionStatus::Invalid]).
#[ntex::test]
async fn test_transactions_api() {
    let _ = supra_logger::init_default_logger(LevelFilter::OFF);

    let (executor_with_resources, web_service) =
        setup_executor_with_web_service(false, &[], &Default::default()).await;
    let ExecutorWebServiceResources {
        executor: test_executor,
        mut rx_tx,
        rpc_storage,
        ..
    } = executor_with_resources;

    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    // The transaction handler for receiving transactions sent from RPC endpoint.
    let (ctrl_leader, mut ctrl_follower) = StepControlLeader::new();

    // Make some fake block data to associate with the transaction when simulating it being
    // stored by the archive.
    let expected_block_hash = Hash([7; HASH_LENGTH]);
    let timestamp = SmrTimestamp::now();
    let expected_block_info = TransactionBlockInfo {
        hash: expected_block_hash,
        height: 7,
        timestamp,
    };
    let expected_block_header_info = BlockHeaderInfo::new(
        expected_block_hash,
        BlockHeader::new(
            Identity::default(),
            7,
            expected_block_hash,
            timestamp,
            View::new(LOCALNET_CHAIN_ID, 7, 7),
        ),
    );

    // Insert block header info
    let mut writer = SchemaBatchWriter::default();
    rpc_storage
        .archive()
        .insert_block_header_info(&mut writer, expected_block_header_info.clone())
        .unwrap();
    writer.write_all().unwrap();

    // The output generated during execution.
    let output = SerializedOutput::Move(SerializedMoveOutput {
        gas_used: 88,
        events: Vec::new(),
        // We give the transaction the failure status when first inserting it, so the
        // set the executor status accordingly.
        vm_status: MoveVmExecutionStatus {
            transaction_status: TransactionStatus::Keep(ExecutionStatus::OutOfGas),
            auxiliary_data: Default::default(),
        },
    });

    // The output presented via the API.
    let expected_output = TransactionOutput::Move(MoveTransactionOutput {
        gas_used: 88,
        events: Vec::new(),
        // The message corresponding to the above status.
        vm_status: "Out of gas".to_string(),
    });

    let _txn_handler = {
        let move_store = test_executor.move_store.clone();
        let move_executor = test_executor.move_executor.clone();
        let rpc_storage = rpc_storage.clone();

        tokio::spawn(async move {
            while let Some(smr_tx) = rx_tx.recv().await {
                debug!("Tx handler received a tx; hash: {}", smr_tx.digest());

                let tx = smr_tx.move_transaction().expect("Must be move transaction");
                let tx = Transaction::UserTransaction(tx.clone());
                execute_move_vm(&move_executor, &move_store, vec![tx]);

                // Tx is now in Pending state in the RpcStorage.
                ctrl_leader.notify_and_wait().await;

                // Insert a transaction with a status that indicates that every subsequent
                // execution should return [TxExecutionStatus::Invalid]. Subsequent attempts
                // to update the status in the transaction-hash-to-status index should fail.
                let mut writer = SchemaBatchWriter::default();
                rpc_storage
                    .archive()
                    .insert_transaction(&mut writer, &smr_tx, TxExecutionStatus::Fail)
                    .unwrap();

                // Insert some dummy block data to ensure that the related indexes are also
                // updated correctly. We only do this once because the archive should only
                // update these fields on the initial finalization. Other tests cover this.
                rpc_storage
                    .archive()
                    .insert_executed_transaction(
                        expected_block_hash,
                        expected_block_header_info.clone(),
                        0,
                        &smr_tx,
                        &output,
                        &mut writer,
                    )
                    .unwrap();
                // Insert txn hash <-> block hash map
                rpc_storage
                    .archive()
                    .insert_transaction_hash_to_block_hash_mapping(
                        smr_tx.digest(),
                        expected_block_hash,
                        &mut writer,
                    )
                    .unwrap();
                writer.write_all().unwrap();

                // Once we inserted a tx manually, this tx must be also *manually* removed from the backlog.
                rpc_storage.expire(smr_tx.expiration_timestamp());

                ctrl_leader.notify_and_wait().await;

                // Try updating the status to [TxExecutionStatus::Success]. This should fail
                // and trigger an error log (if the logs are active).
                let mut writer = SchemaBatchWriter::default();
                rpc_storage
                    .archive()
                    .insert_transaction(&mut writer, &smr_tx, TxExecutionStatus::Success)
                    .unwrap();
                writer.write_all().unwrap();

                ctrl_leader.notify_and_wait().await;

                // Try updating the status to [TxExecutionStatus::Invalid]. This should fail.
                let mut writer = SchemaBatchWriter::default();
                rpc_storage
                    .archive()
                    .insert_transaction(&mut writer, &smr_tx, TxExecutionStatus::Invalid)
                    .unwrap();
                writer.write_all().unwrap();
                ctrl_leader.notify_and_wait().await;

                // Try updating the status to [TxExecutionStatus::Pending]. This should fail
                // and trigger an error log (if the logs are active).
                let mut writer = SchemaBatchWriter::default();
                rpc_storage
                    .archive()
                    .insert_transaction(&mut writer, &smr_tx, TxExecutionStatus::Pending)
                    .unwrap();
                writer.write_all().unwrap();
                ctrl_leader.notify_and_wait().await;
            }
        })
    };

    let pipeline = test::init_service(rpc_app).await;
    let submission_path = "/rpc/v1/transactions/submit".to_string();

    // Test tx submission and its progress from a pending to a finalized state.

    let signed_txn = match create_mint_transaction(
        &test_executor.move_store,
        *test_executor.signer_profile_helper.account_address(),
        backlog::Parameters::percentage_of_default_tx_ttl(0.5),
    ) {
        Transaction::UserTransaction(txn) => txn,
        _ => panic!("Expect UserTransaction type"),
    };

    // Calculate the expected values for the components of the API responses that
    // we can know in advance.
    let smr_move_transaction = SmrMoveTransaction::from(signed_txn.clone());
    let expected_header = smr_move_transaction.header();

    let req = test::TestRequest::post()
        .uri(submission_path.as_str())
        .set_json(&SupraTransaction::Move(signed_txn))
        .to_request();
    let mut resp = pipeline.call(req).await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let body = resp.take_body().collect::<Vec<_>>().await;
    let txn_hashes = body
        .iter()
        .map(|b| from_utf8(b.as_ref().unwrap()).unwrap())
        .collect::<Vec<_>>();

    // Calculate the expected hash too. Should be able to do this before submitting the
    // transaction. Need to refactor after the SmrTransaction refactor PR has been merged.
    // TODO.
    let expected_hash = serde_json::from_str::<Hash>(txn_hashes[0]).unwrap();

    // Wait for the pending transaction to be inserted into the archive db and check the response
    // data. No block or output data should be present yet.
    ctrl_follower
        .wait_and_ack(async {
            let transaction = query_existing_tx_details(&pipeline, expected_hash).await;

            debug!("Inspecting {transaction:?}");

            assert!(transaction.block_header().is_none());
            assert_eq!(transaction.header().to_bytes(), expected_header.to_bytes(),);
            assert_eq!(transaction.hash(), &expected_hash);
            // Note: Not checking the payload because it is serialized in the request and
            // deserialized in the response.
            assert!(transaction.output().is_none());
            assert_eq!(transaction.status(), &TxExecutionStatus::Pending);
        })
        .await;

    // Checks the correctness of the response after an insertion attempt.
    let check_response_data = |transaction: v1::TransactionInfoV1| {
        assert_eq!(
            transaction.block_header().clone().unwrap().to_bytes(),
            expected_block_info.to_bytes()
        );
        assert_eq!(transaction.header().to_bytes(), expected_header.to_bytes(),);
        assert_eq!(transaction.hash(), &expected_hash);
        // Note: Not checking the payload because it is serialized in the request and
        // deserialized in the response.
        assert_eq!(
            transaction.output().clone().unwrap().to_bytes(),
            expected_output.to_bytes()
        );
        assert_eq!(transaction.status(), &TxExecutionStatus::Fail);
    };

    // A failed execution is simulated, so the status should be updated because this is the first
    // time the transaction has produced a status indicating that its has been finalized. We also
    // insert some dummy block data here, so that should also be present in the response.
    ctrl_follower
        .wait_and_ack(async {
            let transaction = query_existing_tx_details(&pipeline, expected_hash).await;
            check_response_data(transaction);
        })
        .await;

    // Same transaction executed again and succeed. This should never happen since transactions
    // should only output the `Fail` status if the sequence number of the sending account has
    // been incremented. Consequently, the status in the archive should remain unchanged, as should
    // the rest of the response data.
    ctrl_follower
        .wait_and_ack(async {
            let transaction = query_existing_tx_details(&pipeline, expected_hash).await;
            check_response_data(transaction);
        })
        .await;

    // Ensure that the attempt to update the status to [TxExecutionStatus::Invalid] also fails.
    ctrl_follower
        .wait_and_ack(async {
            let transaction = query_existing_tx_details(&pipeline, expected_hash).await;
            check_response_data(transaction);
        })
        .await;

    // And that the attempt to update the status to [TxExecutionStatus::Pending] fails too.
    ctrl_follower
        .wait_and_ack(async {
            let transaction = query_existing_tx_details(&pipeline, expected_hash).await;
            check_response_data(transaction);
        })
        .await;
}

/// Test
/// - requesting non existent tx,
/// - tx submission and consequent expiry without execution.
#[ntex::test]
async fn test_transactions_api_with_expired_tx() {
    let _ = supra_logger::init_default_logger(LevelFilter::OFF);

    let (executor_with_resources, web_service) =
        setup_executor_with_web_service(false, &[], &Default::default()).await;
    let ExecutorWebServiceResources {
        executor: test_executor,
        mut rx_tx,
        rpc_storage,
        ..
    } = executor_with_resources;

    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    // The transaction handler for receiving transactions sent from RPC endpoint.
    let (ctrl_leader, mut ctrl_follower) = StepControlLeader::new();

    let _txn_handler = {
        let rpc_storage = rpc_storage.clone();

        tokio::spawn(async move {
            while let Some(smr_tx) = rx_tx.recv().await {
                debug!("Tx handler received a tx; hash: {}", smr_tx.digest());

                // Tx is now in Pending state in the RpcStorage.
                ctrl_leader.notify_and_wait().await;

                // Retire this tx without adding to the archive.
                rpc_storage.expire(smr_tx.expiration_timestamp());

                ctrl_leader.notify_and_wait().await;
            }
        })
    };

    let pipeline = test::init_service(rpc_app).await;
    let submission_path = "/rpc/v1/transactions/submit".to_string();

    // Request non existent tx by v2.
    let path = format!("/rpc/v2/transactions/{}", Hash::dummy());
    debug!("Requesting {path}");

    let req = test::TestRequest::get().uri(path.as_str()).to_request();
    let resp = pipeline.call(req).await.unwrap();

    assert_eq!(resp.status(), http::StatusCode::NOT_FOUND);

    // Request non existent tx by v1.
    let path = format!("/rpc/v1/transactions/{}", Hash::dummy());
    debug!("Requesting {path}");

    let req = test::TestRequest::get().uri(path.as_str()).to_request();
    let mut resp = pipeline.call(req).await.unwrap();

    assert_eq!(resp.status(), http::StatusCode::OK);
    assert_eq!(
        resp.take_body().as_ref(),
        Some(&Body::Bytes(ntex_bytes::Bytes::from("null")))
    );

    // Test submitting a tx that later will be expired.

    let signed_txn = match create_mint_transaction(
        &test_executor.move_store,
        *test_executor.signer_profile_helper.account_address(),
        backlog::Parameters::percentage_of_default_tx_ttl(0.5),
    ) {
        Transaction::UserTransaction(txn) => txn,
        _ => panic!("Expect UserTransaction type"),
    };

    let req = test::TestRequest::post()
        .uri(submission_path.as_str())
        .set_json(&SupraTransaction::Move(signed_txn))
        .to_request();
    let mut resp = pipeline.call(req).await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let body = resp.take_body().collect::<Vec<_>>().await;
    let txn_hashes = body
        .iter()
        .map(|b| from_utf8(b.as_ref().unwrap()).unwrap())
        .collect::<Vec<_>>();

    // Calculate the expected hash too. Should be able to do this before submitting the
    // transaction. Need to refactor after the SmrTransaction refactor PR has been merged.
    // TODO.
    let expected_hash = serde_json::from_str::<Hash>(txn_hashes[0]).unwrap();

    ctrl_follower
        .wait_and_ack(async {
            let transaction = query_existing_tx_details(&pipeline, expected_hash).await;
            debug!("Inspecting {transaction:?}");

            assert_eq!(transaction.status(), &TxExecutionStatus::Pending);
        })
        .await;

    ctrl_follower
        .wait_and_ack(async {
            let path = format!("/rpc/v2/transactions/{expected_hash}");

            debug!("Requesting {path}");

            let req = test::TestRequest::get().uri(path.as_str()).to_request();
            let resp = pipeline.call(req).await.unwrap();

            assert_eq!(resp.status(), http::StatusCode::NOT_FOUND);
        })
        .await;
}

/// Convenience function to fetch [TransactionInfoV2] by [Hash].
async fn query_existing_tx_details(
    pipeline: &Pipeline<impl Service<Request, Response = WebResponse, Error = ntex::web::Error>>,
    hash: Hash,
) -> v1::TransactionInfoV1 {
    let path = format!("/rpc/v1/transactions/{hash}");

    debug!("Requesting {path}");

    let req = test::TestRequest::get().uri(path.as_str()).to_request();
    let mut resp = pipeline.call(req).await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let body = resp.take_body().collect::<Vec<_>>().await;
    let body_strings = body
        .iter()
        .map(|b| from_utf8(b.as_ref().unwrap()).unwrap())
        .collect::<Vec<_>>();
    serde_json::from_str::<v1::TransactionInfoV1>(body_strings[0]).unwrap()
}

#[ntex::test]
/// Test the tx execution API fails if the tx cannot be passed to the validator.
async fn test_transaction_submission_failure_0() {
    let mut executor_with_resources =
        setup_executor_and_web_resources(false, &Default::default()).await;
    let ExecutorWebServiceResources {
        tx_consumer: tx_sender,
        ..
    } = &mut executor_with_resources;
    tx_sender.set_pass_predicate(mocked_transaction_consumer::fail);
    let (executor_with_resources, web_service) =
        setup_web_service(executor_with_resources, &[]).await;
    let ExecutorWebServiceResources {
        executor: test_executor,
        mut rx_tx,
        ..
    } = executor_with_resources;

    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    let _tx_handler = tokio::spawn(async move {
        while let Some(smr_tx) = rx_tx.recv().await {
            let tx_hash = smr_tx.digest();

            debug!("[Tx {tx_hash}] Received. Ignore it intentionally.");
        }
    });

    let pipeline = test::init_service(rpc_app).await;

    // Submit a transaction
    let uri = "/rpc/v1/transactions/submit".to_string();
    let signed_txn = match create_mint_transaction(
        &test_executor.move_store,
        *test_executor.signer_profile_helper.account_address(),
        backlog::Parameters::percentage_of_default_tx_ttl(0.5),
    ) {
        Transaction::UserTransaction(txn) => txn,
        _ => panic!("Expect UserTransaction type"),
    };

    let request = test::TestRequest::post()
        .uri(&uri)
        .set_json(&SupraTransaction::Move(signed_txn))
        .to_request();
    debug!("Submitting a funding request to {uri}");

    // Response must be an error response.
    let response: ErrResp = test::read_response_json(&pipeline, request).await;
    debug!("Response {response:?}");

    let expected_response_message = "failed";
    assert_eq!(response.message(), expected_response_message);
}

#[ntex::test]
/// Test the tx execution API fails if the tx TTL exceeds the configured value.
async fn test_transaction_submission_failure_1() {
    let _ = supra_logger::init_default_logger(LevelFilter::DEBUG);

    let mut executor_with_resources =
        setup_executor_and_web_resources(false, &Default::default()).await;
    let ExecutorWebServiceResources {
        tx_consumer: tx_sender,
        ..
    } = &mut executor_with_resources;
    tx_sender.set_pass_predicate(mocked_transaction_consumer::fail);
    let (executor_with_resources, web_service) =
        setup_web_service(executor_with_resources, &[]).await;
    let ExecutorWebServiceResources {
        executor: test_executor,
        mut rx_tx,
        ..
    } = executor_with_resources;

    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    let _tx_handler = tokio::spawn(async move {
        while let Some(smr_tx) = rx_tx.recv().await {
            let tx_hash = smr_tx.digest();

            debug!("[Tx {tx_hash}] Received. Ignore it intentionally.");
        }
    });

    let pipeline = test::init_service(rpc_app).await;

    // Submit a transaction with TTL exceeding the max backlog tx TTL.
    let uri = "/rpc/v1/transactions/submit".to_string();
    let signed_txn = match create_mint_transaction(
        &test_executor.move_store,
        *test_executor.signer_profile_helper.account_address(),
        backlog::Parameters::percentage_of_default_tx_ttl(2.0),
    ) {
        Transaction::UserTransaction(txn) => txn,
        _ => panic!("Expect UserTransaction type"),
    };

    let request = test::TestRequest::post()
        .uri(&uri)
        .set_json(&SupraTransaction::Move(signed_txn))
        .to_request();
    debug!("Submitting a funding request to {uri}");

    // Response must be an error response.
    let response: ErrResp = test::read_response_json(&pipeline, request).await;
    debug!("Response {response:?}");

    let expected_response_message = "failed";
    assert_eq!(response.message(), expected_response_message);
}

#[ntex::test]
async fn test_transactions_parameters_endpoint() {
    let _ = supra_logger::init_default_logger(LevelFilter::OFF);

    let (executor_with_resources, web_service) =
        setup_executor_with_web_service(false, &[], &Default::default()).await;
    let ExecutorWebServiceResources { mut rx_tx, .. } = executor_with_resources;

    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    let _txn_handler = {
        tokio::spawn(async move {
            while let Some(smr_tx) = rx_tx.recv().await {
                debug!("Tx handler received a tx; hash: {}", smr_tx.digest());
            }
        })
    };

    let pipeline = test::init_service(rpc_app).await;
    let uri = "/rpc/v1/transactions/parameters";
    debug!("Requesting {uri}");

    let req = test::TestRequest::get().uri(uri).to_request();
    let response: TransactionParameters = test::read_response_json(&pipeline, req).await;

    assert_eq!(
        response.max_transaction_time_to_live_seconds,
        backlog::Parameters::default().max_backlog_transaction_time_to_live_seconds,
        "Parameters must match"
    );
}

/// Convenience function to fetch [TransactionInfoV2] by [Hash] based on the passed API version
/// and transaction type
pub(crate) async fn query_existing_tx_details_by_version<T: DeserializeOwned>(
    pipeline: &Pipeline<impl Service<Request, Response = WebResponse, Error = ntex::web::Error>>,
    hash: Hash,
    txn_type: Option<&str>,
    version: u8,
) -> Option<T> {
    let txn_type_param = match txn_type {
        None => String::new(),
        Some(value) => format!("?type={value}"),
    };
    let path = format!("/rpc/v{version}/transactions/{hash}{txn_type_param}");

    debug!("Requesting {path}");

    let req = test::TestRequest::get().uri(path.as_str()).to_request();
    let mut resp = pipeline.call(req).await.unwrap();
    if resp.status() != http::StatusCode::OK {
        return None;
    }
    assert_eq!(resp.status(), http::StatusCode::OK);
    let body = resp.take_body().collect::<Vec<_>>().await;
    let body_strings = body
        .iter()
        .map(|b| from_utf8(b.as_ref().unwrap()).unwrap())
        .collect::<Vec<_>>();
    serde_json::from_str::<T>(body_strings[0]).ok()
}
#[ntex::test]
async fn test_transaction_by_hash_api() {
    let _ = supra_logger::init_default_logger(LevelFilter::OFF);
    let (_, web_service) = setup_executor_with_web_service(true, &[], &Default::default()).await;
    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    let pipeline = test::init_service(rpc_app).await;

    // Get the latest block header.
    let latest_header_bytes = block::get("/rpc/v1/block", &pipeline).await;
    let resp_block_header_info =
        serde_json::from_slice::<BlockHeaderInfo>(latest_header_bytes.as_ref()).unwrap();
    let bhash = resp_block_header_info.hash;

    let block_user_txn_hashes = get_block_transactions_v3(Some("user"), bhash, &pipeline).await;
    let block_auto_txn_hashes = get_block_transactions_v3(Some("auto"), bhash, &pipeline).await;
    let block_meta_txn_hashes = get_block_transactions_v3(Some("meta"), bhash, &pipeline).await;
    assert!(!block_user_txn_hashes.is_empty());
    assert!(!block_auto_txn_hashes.is_empty());
    assert_eq!(block_meta_txn_hashes.len(), 1);
    let user_txn_hash = block_user_txn_hashes[0];
    let auto_txn_hash = block_auto_txn_hashes[0];
    let meta_txn_hash = block_meta_txn_hashes[0];

    // Get user transaction with v1 and v3
    let v2_user_txn_info = query_existing_tx_details_by_version::<v2::TransactionInfoV2>(
        &pipeline,
        user_txn_hash,
        None,
        2,
    )
    .await
    .expect("Valid txn info");
    let v3_user_txn_info = query_existing_tx_details_by_version::<v3::Transaction>(
        &pipeline,
        user_txn_hash,
        Some("user"),
        3,
    )
    .await
    .expect("Valid txn info")
    .into_transaction_info()
    .unwrap();
    let v3_user_txn_info_no_type =
        query_existing_tx_details_by_version::<v3::Transaction>(&pipeline, user_txn_hash, None, 3)
            .await
            .expect("Valid txn info")
            .into_transaction_info()
            .unwrap();
    assert_eq!(v2_user_txn_info.header(), v3_user_txn_info.header());
    assert_eq!(v2_user_txn_info.header(), v3_user_txn_info_no_type.header());

    // Check no Automated transaction info can be get via V2
    assert!(
        query_existing_tx_details_by_version::<v2::TransactionInfoV2>(
            &pipeline,
            auto_txn_hash,
            None,
            2
        )
        .await
        .is_none()
    );
    assert!(
        query_existing_tx_details_by_version::<v2::TransactionInfoV2>(
            &pipeline,
            auto_txn_hash,
            Some("auto"),
            2
        )
        .await
        .is_none()
    );

    let v3_auto_txn_info = query_existing_tx_details_by_version::<v3::Transaction>(
        &pipeline,
        auto_txn_hash,
        Some("auto"),
        3,
    )
    .await
    .expect("Valid txn info")
    .into_transaction_info()
    .unwrap();
    let v3_auto_txn_info_no_type =
        query_existing_tx_details_by_version::<v3::Transaction>(&pipeline, auto_txn_hash, None, 3)
            .await
            .expect("Valid txn info")
            .into_transaction_info()
            .unwrap();
    assert_eq!(v3_auto_txn_info.header(), v3_auto_txn_info_no_type.header());
    assert!(matches!(
        v3_auto_txn_info.authenticator(),
        TransactionAuthenticator::Automation(_)
    ));

    // Check no block metadata  transaction info can be get via V2
    assert!(
        query_existing_tx_details_by_version::<v2::TransactionInfoV2>(
            &pipeline,
            meta_txn_hash,
            None,
            2
        )
        .await
        .is_none()
    );
    assert!(
        query_existing_tx_details_by_version::<v2::TransactionInfoV2>(
            &pipeline,
            meta_txn_hash,
            Some("meta"),
            2
        )
        .await
        .is_none()
    );

    let v3_meta_txn_info = query_existing_tx_details_by_version::<v3::Transaction>(
        &pipeline,
        meta_txn_hash,
        Some("meta"),
        3,
    )
    .await
    .expect("Valid txn info")
    .into_block_metadata_info()
    .unwrap();
    println!("meta without type");
    let v3_meta_txn_info_no_type =
        query_existing_tx_details_by_version::<v3::Transaction>(&pipeline, meta_txn_hash, None, 3)
            .await
            .expect("Valid txn info")
            .into_block_metadata_info()
            .unwrap();
    assert_eq!(
        v3_meta_txn_info.block_header(),
        v3_meta_txn_info_no_type.block_header()
    );
    // Check that block-metadata hash is checked to be valid one.
    assert!(query_existing_tx_details_by_version::<v3::Transaction>(
        &pipeline,
        auto_txn_hash,
        Some("meta"),
        3,
    )
    .await
    .is_none());
}

struct StepControlLeader {
    ctrl_tx: mpsc::Sender<Arc<Notify>>,
}

struct StepControlFollower {
    ctrl_rx: mpsc::Receiver<Arc<Notify>>,
}

impl StepControlLeader {
    fn new() -> (Self, StepControlFollower) {
        let (ctrl_tx, ctrl_rx) = tokio::sync::mpsc::channel(1);

        (
            StepControlLeader { ctrl_tx },
            StepControlFollower { ctrl_rx },
        )
    }

    async fn notify_and_wait(&self) {
        let acker = Arc::new(Notify::new());
        self.ctrl_tx.send(acker.clone()).await.expect("Must send");
        acker.notified().await;
    }
}

impl StepControlFollower {
    async fn wait_and_ack<F>(&mut self, callback: F)
    where
        F: Future<Output = ()>,
    {
        let acker = self.ctrl_rx.recv().await.unwrap();
        callback.await;
        acker.notify_waiters();
    }
}
