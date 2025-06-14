use crate::rest::helpers::setup_executor_with_web_service;
use ntex::web;
use ntex::web::test;
use rpc_node::rest::router::default_route;
use supra_logger::LevelFilter;
use types::tests::utils::common::coin_type;

use crate::rest::block;
use crate::rest::block::get_block_transactions_v3;
use crate::rest::common::{
    api_response_snapshot_filters, call_endpoint, find_api_endpoints_by_tag,
    setup_test_data_fixture, RECEIVER_ACCOUNT_KEY_PAIR, SENDER_ACCOUNT_KEY_PAIR,
};
use crate::rest::transactions::query_existing_tx_details_by_version;
use transactions::{AccountAddress, TTransactionHeaderProperties};
use types::api::v1::TransactionAuthenticator;
use types::api::v2::{AccountStatementV2, BlockHeaderInfo};
use types::api::v3::{AccountStatementV3, Transaction};

/// Use snapshot testing for detect API changes.
/// If this test fails, inspect if the change is backward compatible, if not
/// deprecate the existing API and create a new one.
#[ntex::test]
async fn test_account_api() {
    let _ = supra_logger::init_default_logger(LevelFilter::OFF);

    let (executor_with_resources, web_service) =
        setup_executor_with_web_service(false, &[], &Default::default()).await;
    let test_executor = executor_with_resources.executor;
    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    // Database fixture setup in order for test API.
    setup_test_data_fixture(
        &test_executor.move_executor,
        &test_executor.move_store,
        executor_with_resources.rpc_storage.archive(),
    );
    let sender_account = SENDER_ACCOUNT_KEY_PAIR.0.to_canonical_string();
    let receiver_account = RECEIVER_ACCOUNT_KEY_PAIR.0.to_canonical_string();

    let pipeline = test::init_service(rpc_app).await;

    // Assert snapshot for account APIs, if it fails, check the list of endpoints.
    // This is to remind us to add new test cases for new endpoints.
    let account_apis = find_api_endpoints_by_tag("/accounts");
    insta::assert_debug_snapshot!(account_apis, @r#"
    [
        "/rpc/v1/accounts/{address}",
        "/rpc/v1/accounts/{address}/coin_transactions",
        "/rpc/v1/accounts/{address}/modules",
        "/rpc/v1/accounts/{address}/modules/{module_name}",
        "/rpc/v1/accounts/{address}/resources",
        "/rpc/v1/accounts/{address}/resources/{resource_type}",
        "/rpc/v1/accounts/{address}/transactions",
        "/rpc/v2/accounts/{address}",
        "/rpc/v2/accounts/{address}/coin_transactions",
        "/rpc/v2/accounts/{address}/modules",
        "/rpc/v2/accounts/{address}/modules/{module_name}",
        "/rpc/v2/accounts/{address}/resources",
        "/rpc/v2/accounts/{address}/resources/{resource_type}",
        "/rpc/v2/accounts/{address}/transactions",
        "/rpc/v3/accounts/{address}/automated_transactions",
        "/rpc/v3/accounts/{address}/coin_transactions",
        "/rpc/v3/accounts/{address}/modules",
        "/rpc/v3/accounts/{address}/resources",
        "/rpc/v3/accounts/{address}/transactions",
    ]
    "#);

    // Account info
    let path = format!("/rpc/v1/accounts/{}", sender_account);
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::assert_snapshot!(&resp_body, @r###"
    sequence_number: 1
    authentication_key: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
    "###);

    // Account coin transactions v1
    let path = format!("/rpc/v1/accounts/{}/coin_transactions", receiver_account);
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;

    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        record:
        - authenticator:
            Move:
              Ed25519:
                public_key: 0xf66bf0ce5ceb582b93d6780820c2025b9967aedaa259bdbb9f3d0297eced0e18
                signature: ***
          block_header:
            hash: ***
            height: 24
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: '0x000000000000000000000000000000000000000000000000000000000a550c18'
            sequence_number: 3
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_coin::mint
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '1000'
          output:
            Move:
              gas_used: 8
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '1000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '0'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '1000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '4'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '8'
              vm_status: Executed successfully
          status: Success
        - authenticator:
            Move:
              Ed25519:
                public_key: 0xe882f5be65879997cace0e5f5cbd527ab02febb96b8b40e090903f143d139d5e
                signature: ***
          block_header:
            hash: ***
            height: 24
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
            sequence_number: 0
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_account::transfer
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '2000'
          output:
            Move:
              gas_used: 9
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinWithdraw
                data:
                  account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '3'
                  account_address: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                sequence_number: '0'
                type: 0x1::coin::WithdrawEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '1'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '5'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '9'
              vm_status: Executed successfully
          status: Success
        cursor: 28
        "###
        );
    });

    // Only the first coin transaction
    let path = format!(
        "/rpc/v1/accounts/{}/coin_transactions?count=1",
        receiver_account
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        record:
        - authenticator:
            Move:
              Ed25519:
                public_key: 0xf66bf0ce5ceb582b93d6780820c2025b9967aedaa259bdbb9f3d0297eced0e18
                signature: ***
          block_header:
            hash: ***
            height: 24
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: '0x000000000000000000000000000000000000000000000000000000000a550c18'
            sequence_number: 3
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_coin::mint
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '1000'
          output:
            Move:
              gas_used: 8
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '1000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '0'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '1000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '4'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '8'
              vm_status: Executed successfully
          status: Success
        cursor: 27
        "###
        );
    });

    // Only the second coin transaction
    let path = format!(
        "/rpc/v1/accounts/{}/coin_transactions?count=1&start=27",
        receiver_account
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        record:
        - authenticator:
            Move:
              Ed25519:
                public_key: 0xe882f5be65879997cace0e5f5cbd527ab02febb96b8b40e090903f143d139d5e
                signature: ***
          block_header:
            hash: ***
            height: 24
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
            sequence_number: 0
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_account::transfer
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '2000'
          output:
            Move:
              gas_used: 9
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinWithdraw
                data:
                  account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '3'
                  account_address: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                sequence_number: '0'
                type: 0x1::coin::WithdrawEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '1'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '5'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '9'
              vm_status: Executed successfully
          status: Success
        cursor: 28
        "###
        );
    });

    // No additional coin transactions
    let path = format!(
        "/rpc/v1/accounts/{}/coin_transactions?count=1&start=28",
        receiver_account
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::assert_snapshot!(
        &resp_body,
        @r###"
    record: []
    cursor: 0
    "###
    );

    // Account coin transactions v2
    let path = format!("/rpc/v2/accounts/{}/coin_transactions", receiver_account);
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;

    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        record:
        - authenticator:
            Move:
              Ed25519:
                public_key: 0xf66bf0ce5ceb582b93d6780820c2025b9967aedaa259bdbb9f3d0297eced0e18
                signature: ***
          block_header:
            author: 0x1818181818181818181818181818181818181818181818181818181818181818
            hash: ***
            height: 24
            parent: '0x0000000000000000000000000000000000000000000000000000000000000000'
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            view:
              epoch_id:
                chain_id: 255
                epoch: 1
              round: 1
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: '0x000000000000000000000000000000000000000000000000000000000a550c18'
            sequence_number: 3
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_coin::mint
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '1000'
          output:
            Move:
              gas_used: 8
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '1000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '0'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '1000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '4'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '8'
              vm_status: Executed successfully
          status: Success
        - authenticator:
            Move:
              Ed25519:
                public_key: 0xe882f5be65879997cace0e5f5cbd527ab02febb96b8b40e090903f143d139d5e
                signature: ***
          block_header:
            author: 0x1818181818181818181818181818181818181818181818181818181818181818
            hash: ***
            height: 24
            parent: '0x0000000000000000000000000000000000000000000000000000000000000000'
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            view:
              epoch_id:
                chain_id: 255
                epoch: 1
              round: 1
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
            sequence_number: 0
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_account::transfer
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '2000'
          output:
            Move:
              gas_used: 9
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinWithdraw
                data:
                  account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '3'
                  account_address: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                sequence_number: '0'
                type: 0x1::coin::WithdrawEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '1'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '5'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '9'
              vm_status: Executed successfully
          status: Success
        cursor: 28
        "###
        );
    });

    // Only the first
    let path = format!(
        "/rpc/v2/accounts/{}/coin_transactions?count=1",
        receiver_account
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;

    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,@r###"
        record:
        - authenticator:
            Move:
              Ed25519:
                public_key: 0xf66bf0ce5ceb582b93d6780820c2025b9967aedaa259bdbb9f3d0297eced0e18
                signature: ***
          block_header:
            author: 0x1818181818181818181818181818181818181818181818181818181818181818
            hash: ***
            height: 24
            parent: '0x0000000000000000000000000000000000000000000000000000000000000000'
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            view:
              epoch_id:
                chain_id: 255
                epoch: 1
              round: 1
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: '0x000000000000000000000000000000000000000000000000000000000a550c18'
            sequence_number: 3
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_coin::mint
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '1000'
          output:
            Move:
              gas_used: 8
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '1000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '0'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '1000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '4'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '8'
              vm_status: Executed successfully
          status: Success
        cursor: 27
        "###
        )}
    );

    // Only the second
    let path = format!(
        "/rpc/v2/accounts/{}/coin_transactions?count=1&start=27",
        receiver_account
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,@r###"
        record:
        - authenticator:
            Move:
              Ed25519:
                public_key: 0xe882f5be65879997cace0e5f5cbd527ab02febb96b8b40e090903f143d139d5e
                signature: ***
          block_header:
            author: 0x1818181818181818181818181818181818181818181818181818181818181818
            hash: ***
            height: 24
            parent: '0x0000000000000000000000000000000000000000000000000000000000000000'
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            view:
              epoch_id:
                chain_id: 255
                epoch: 1
              round: 1
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
            sequence_number: 0
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_account::transfer
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '2000'
          output:
            Move:
              gas_used: 9
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinWithdraw
                data:
                  account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '3'
                  account_address: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                sequence_number: '0'
                type: 0x1::coin::WithdrawEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '1'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '5'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '9'
              vm_status: Executed successfully
          status: Success
        cursor: 28
        "###
        )}
    );

    // No additional coin transactions
    // Only the second
    let path = format!(
        "/rpc/v2/accounts/{}/coin_transactions?count=1&start=28",
        receiver_account
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,@r###"
        record: []
        cursor: 0
        "###
        )}
    );

    // Account coin transactions v3
    let path = format!("/rpc/v3/accounts/{}/coin_transactions", receiver_account);
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;

    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r"
        - txn_type: user
          authenticator:
            Move:
              Ed25519:
                public_key: 0xf66bf0ce5ceb582b93d6780820c2025b9967aedaa259bdbb9f3d0297eced0e18
                signature: ***
          block_header:
            author: 0x1818181818181818181818181818181818181818181818181818181818181818
            hash: ***
            height: 24
            parent: '0x0000000000000000000000000000000000000000000000000000000000000000'
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            view:
              epoch_id:
                chain_id: 255
                epoch: 1
              round: 1
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: '0x000000000000000000000000000000000000000000000000000000000a550c18'
            sequence_number: 3
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_coin::mint
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '1000'
          output:
            Move:
              gas_used: 8
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '1000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '0'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '1000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '4'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '8'
              vm_status: Executed successfully
          status: Success
        - txn_type: user
          authenticator:
            Move:
              Ed25519:
                public_key: 0xe882f5be65879997cace0e5f5cbd527ab02febb96b8b40e090903f143d139d5e
                signature: ***
          block_header:
            author: 0x1818181818181818181818181818181818181818181818181818181818181818
            hash: ***
            height: 24
            parent: '0x0000000000000000000000000000000000000000000000000000000000000000'
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            view:
              epoch_id:
                chain_id: 255
                epoch: 1
              round: 1
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
            sequence_number: 0
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_account::transfer
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '2000'
          output:
            Move:
              gas_used: 9
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinWithdraw
                data:
                  account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '3'
                  account_address: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                sequence_number: '0'
                type: 0x1::coin::WithdrawEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '1'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '5'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '9'
              vm_status: Executed successfully
          status: Success
        "
        );
    });

    // Only the first
    let path = format!(
        "/rpc/v3/accounts/{}/coin_transactions?count=1",
        receiver_account
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;

    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,@r"
        - txn_type: user
          authenticator:
            Move:
              Ed25519:
                public_key: 0xf66bf0ce5ceb582b93d6780820c2025b9967aedaa259bdbb9f3d0297eced0e18
                signature: ***
          block_header:
            author: 0x1818181818181818181818181818181818181818181818181818181818181818
            hash: ***
            height: 24
            parent: '0x0000000000000000000000000000000000000000000000000000000000000000'
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            view:
              epoch_id:
                chain_id: 255
                epoch: 1
              round: 1
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: '0x000000000000000000000000000000000000000000000000000000000a550c18'
            sequence_number: 3
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_coin::mint
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '1000'
          output:
            Move:
              gas_used: 8
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '1000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '0'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '1000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '4'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '8'
              vm_status: Executed successfully
          status: Success
        "
        )}
    );

    // Only the second
    let path = format!(
        "/rpc/v3/accounts/{}/coin_transactions?count=1&start=27",
        receiver_account
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,@r"
        - txn_type: user
          authenticator:
            Move:
              Ed25519:
                public_key: 0xe882f5be65879997cace0e5f5cbd527ab02febb96b8b40e090903f143d139d5e
                signature: ***
          block_header:
            author: 0x1818181818181818181818181818181818181818181818181818181818181818
            hash: ***
            height: 24
            parent: '0x0000000000000000000000000000000000000000000000000000000000000000'
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            view:
              epoch_id:
                chain_id: 255
                epoch: 1
              round: 1
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
            sequence_number: 0
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_account::transfer
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '2000'
          output:
            Move:
              gas_used: 9
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinWithdraw
                data:
                  account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '3'
                  account_address: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                sequence_number: '0'
                type: 0x1::coin::WithdrawEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '1'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '5'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '9'
              vm_status: Executed successfully
          status: Success
        "
        )}
    );

    // No additional coin transactions
    // Only the second
    let path = format!(
        "/rpc/v3/accounts/{}/coin_transactions?count=1&start=28",
        receiver_account
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,@"[]"
        )}
    );

    // Account resources list
    let path: String = format!("/rpc/v1/accounts/{}/resources", sender_account);
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::assert_snapshot!(&resp_body, @r###"
    Resources:
      resource:
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::account::Account
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          module: account
          name: Account
          type_args: []
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::coin::CoinStore<0000000000000000000000000000000000000000000000000000000000000001::supra_coin::SupraCoin>
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          module: coin
          name: CoinStore
          type_args:
          - struct:
              address: '0000000000000000000000000000000000000000000000000000000000000001'
              module: supra_coin
              name: SupraCoin
              type_args: []
      cursor: 0085e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f6801000000000000000000000000000000000000000000000000000000000000000104636f696e09436f696e53746f7265010700000000000000000000000000000000000000000000000000000000000000010a73757072615f636f696e095375707261436f696e0000
    "###);

    // Account resources by move struct type
    let path = format!(
        "/rpc/v1/accounts/{}/resources/{}",
        sender_account,
        urlencoding::encode(&format!("0x1::coin::CoinStore<{}>", coin_type()))
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;

    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        result:
        - coin:
            value: '49997100'
          deposit_events:
            counter: '1'
            guid:
              id:
                addr: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                creation_num: '2'
          frozen: false
          withdraw_events:
            counter: '1'
            guid:
              id:
                addr: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                creation_num: '3'
        "###
        );
    });

    // Account transactions v1
    let path = format!("/rpc/v1/accounts/{}/transactions", sender_account);
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        record:
        - authenticator:
            Move:
              Ed25519:
                public_key: 0xe882f5be65879997cace0e5f5cbd527ab02febb96b8b40e090903f143d139d5e
                signature: ***
          block_header:
            hash: ***
            height: 24
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
            sequence_number: 0
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_account::transfer
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '2000'
          output:
            Move:
              gas_used: 9
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinWithdraw
                data:
                  account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '3'
                  account_address: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                sequence_number: '0'
                type: 0x1::coin::WithdrawEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '1'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '5'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '9'
              vm_status: Executed successfully
          status: Success
        "###
        );
    });

    // Account transactions v2
    let path = format!("/rpc/v2/accounts/{}/transactions", sender_account);
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        record:
        - authenticator:
            Move:
              Ed25519:
                public_key: 0xe882f5be65879997cace0e5f5cbd527ab02febb96b8b40e090903f143d139d5e
                signature: ***
          block_header:
            author: 0x1818181818181818181818181818181818181818181818181818181818181818
            hash: ***
            height: 24
            parent: '0x0000000000000000000000000000000000000000000000000000000000000000'
            timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            view:
              epoch_id:
                chain_id: 255
                epoch: 1
              round: 1
          hash: ***
          header:
            chain_id: 255
            expiration_timestamp:
              microseconds_since_unix_epoch: ***,
              utc_date_time: ***
            sender:
              Move: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
            sequence_number: 0
            gas_unit_price: 100
            max_gas_amount: 500000
          payload:
            Move:
              type: entry_function_payload
              function: 0x1::supra_account::transfer
              type_arguments: []
              arguments:
              - 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              - '2000'
          output:
            Move:
              gas_used: 9
              events:
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinWithdraw
                data:
                  account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '3'
                  account_address: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                sequence_number: '0'
                type: 0x1::coin::WithdrawEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::coin::CoinDeposit
                data:
                  account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                  amount: '2000'
                  coin_type: 0x1::supra_coin::SupraCoin
              - guid:
                  creation_number: '2'
                  account_address: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
                sequence_number: '1'
                type: 0x1::coin::DepositEvent
                data:
                  amount: '2000'
              - guid:
                  creation_number: '0'
                  account_address: '0x0'
                sequence_number: '0'
                type: 0x1::transaction_fee::FeeStatement
                data:
                  execution_gas_units: '4'
                  io_gas_units: '5'
                  storage_fee_quants: '0'
                  storage_fee_refund_quants: '0'
                  total_charge_gas_units: '9'
              vm_status: Executed successfully
          status: Success
        "###
        );
    });

    // Account module list V1
    let path: String = format!("/rpc/v1/accounts/{}/modules", "0x1");
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::assert_snapshot!(&resp_body, @r###"
    Modules:
      modules:
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::acl
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: acl
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::any
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: any
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::bcs
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: bcs
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::dkg
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: dkg
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::code
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: code
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::coin
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: coin
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::guid
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: guid
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::hash
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: hash
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::jwks
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: jwks
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::util
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: util
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::block
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: block
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::debug
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: debug
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::error
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: error
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::event
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: event
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::stake
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: stake
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::table
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: table
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::math64
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: math64
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::object
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: object
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::option
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: option
      - - 0x0000000000000000000000000000000000000000000000000000000000000001::signer
        - address: '0000000000000000000000000000000000000000000000000000000000000001'
          name: signer
      cursor: 00000000000000000000000000000000000000000000000000000000000000000128000000000000000000000000000000000000000000000000000000000000000001067369676e657200
    "###);

    // Account module list V2
    let path: String = format!("/rpc/v2/accounts/{}/modules?count=2", "0x1");
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::assert_snapshot!(&resp_body, @r###"
    modules:
    - bytecode: 0xa11ceb0b060000000c010006020604030a30043a0605402f076f5e08cd012006ed011410810289010a8a03060c90038c010d9c04020000000100020003070000040001000005020100000602030000070104000008000100020607030100010a080800020b070a010002080b0c010005060706080602070800050002060800050101010800010605010502060a090006090001030206050302010302070a0900030109000361636c056572726f7206766563746f720341434c036164640f6173736572745f636f6e7461696e7308636f6e7461696e7305656d7074790672656d6f7665046c69737410696e76616c69645f617267756d656e7408696e6465785f6f6600000000000000000000000000000000000000000000000000000000000000010308000000000000000003080100000000000000126170746f733a3a6d657461646174615f7631750200000000000000000845434f4e5441494e255468652041434c20616c726561647920636f6e7461696e732074686520616464726573732e01000000000000000c454e4f545f434f4e5441494e255468652041434c20646f6573206e6f7420636f6e7461696e2074686520616464726573732e0000000201090a050001000005140a000f000e010c022e0b02380020040a050f0b000107001106270b000f000b014406020101000001090b000b011102040505080701110627020201000001050b0010000e01380002030100000103400600000000000000001200020401000009150a000f000e010c022e0b0238010c03040a050f0b000107011106270b000f000b0338020102000000
      abi:
        address: '0x1'
        name: acl
        friends: []
        exposed_functions:
        - name: add
          visibility: public
          is_entry: false
          is_view: false
          generic_type_params: []
          params:
          - '&mut 0x1::acl::ACL'
          - address
          return: []
        - name: assert_contains
          visibility: public
          is_entry: false
          is_view: false
          generic_type_params: []
          params:
          - '&0x1::acl::ACL'
          - address
          return: []
        - name: contains
          visibility: public
          is_entry: false
          is_view: false
          generic_type_params: []
          params:
          - '&0x1::acl::ACL'
          - address
          return:
          - bool
        - name: empty
          visibility: public
          is_entry: false
          is_view: false
          generic_type_params: []
          params: []
          return:
          - 0x1::acl::ACL
        - name: remove
          visibility: public
          is_entry: false
          is_view: false
          generic_type_params: []
          params:
          - '&mut 0x1::acl::ACL'
          - address
          return: []
        structs:
        - name: ACL
          is_native: false
          abilities:
          - copy
          - drop
          - store
          generic_type_params: []
          fields:
          - name: list
            type: vector<address>
    - bytecode: 0xa11ceb0b060000000d01000c020c08031428043c0605421b075d800108dd012006fd010a108702760afd02090c8603390dbf03040fc303020001000200030004000500060007060004090700000800010106000a020300000b01000100050a04050100010d06070100020e080800030f0700010003000400060001090001080001060800010608010001080101060900010a0201030c636f707961626c655f616e7903616e7903626373056572726f720866726f6d5f62637306737472696e6709747970655f696e666f03416e79047061636b06537472696e6709747970655f6e616d6506756e7061636b046461746108746f5f627974657310696e76616c69645f617267756d656e740a66726f6d5f6279746573000000000000000000000000000000000000000000000000000000000000000103080100000000000000126170746f733a3a6d657461646174615f7631620101000000000000000e45545950455f4d49534d415443484754686520747970652070726f766964656420666f722060756e7061636b60206973206e6f74207468652073616d652061732077617320676976656e20666f7220607061636b602e00000002020a08010c0a0200010000040538000e0038011200020101000004030b0010000202010000040f38000e00100014210407050a07001105270e0010011438020200000001000000
      abi:
        address: '0x1'
        name: any
        friends:
        - 0x1::copyable_any
        exposed_functions:
        - name: pack
          visibility: public
          is_entry: false
          is_view: false
          generic_type_params:
          - constraints:
            - drop
            - store
          params:
          - T0
          return:
          - 0x1::any::Any
        - name: type_name
          visibility: public
          is_entry: false
          is_view: false
          generic_type_params: []
          params:
          - '&0x1::any::Any'
          return:
          - '&0x1::string::String'
        - name: unpack
          visibility: public
          is_entry: false
          is_view: false
          generic_type_params:
          - constraints: []
          params:
          - 0x1::any::Any
          return:
          - T0
        structs:
        - name: Any
          is_native: false
          abilities:
          - drop
          - store
          generic_type_params: []
          fields:
          - name: type_name
            type: 0x1::string::String
          - name: data
            type: vector<u8>
    cursor: 0000000000000000000000000000000000000000000000000000000000000000012500000000000000000000000000000000000000000000000000000000000000000103616e7900
    "###);

    let path: String = format!("/rpc/v1/accounts/{}/modules/{}", "0x1", "coin");
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::assert_snapshot!(&resp_body, @r###"
    address: '0x1'
    name: coin
    friends:
    - 0x1::genesis
    - 0x1::supra_coin
    - 0x1::transaction_fee
    exposed_functions:
    - name: allow_supply_upgrades
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params: []
      params:
      - '&signer'
      - bool
      return: []
    - name: balance
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params:
      - address
      return:
      - u64
    - name: burn
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - 0x1::coin::Coin<T0>
      - '&0x1::coin::BurnCapability<T0>'
      return: []
    - name: burn_from
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - address
      - u64
      - '&0x1::coin::BurnCapability<T0>'
      return: []
    - name: coin_supply
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params: []
      return:
      - 0x1::option::Option<u128>
    - name: coin_to_fungible_asset
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - 0x1::coin::Coin<T0>
      return:
      - 0x1::fungible_asset::FungibleAsset
    - name: collect_into_aggregatable_coin
      visibility: friend
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - address
      - u64
      - '&mut 0x1::coin::AggregatableCoin<T0>'
      return: []
    - name: convert_and_take_paired_burn_ref
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - 0x1::coin::BurnCapability<T0>
      return:
      - 0x1::fungible_asset::BurnRef
    - name: create_coin_conversion_map
      visibility: public
      is_entry: true
      is_view: false
      generic_type_params: []
      params:
      - '&signer'
      return: []
    - name: create_pairing
      visibility: public
      is_entry: true
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&signer'
      return: []
    - name: decimals
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params: []
      return:
      - u8
    - name: deposit
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - address
      - 0x1::coin::Coin<T0>
      return: []
    - name: destroy_burn_cap
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - 0x1::coin::BurnCapability<T0>
      return: []
    - name: destroy_freeze_cap
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - 0x1::coin::FreezeCapability<T0>
      return: []
    - name: destroy_mint_cap
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - 0x1::coin::MintCapability<T0>
      return: []
    - name: destroy_zero
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - 0x1::coin::Coin<T0>
      return: []
    - name: drain_aggregatable_coin
      visibility: friend
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&mut 0x1::coin::AggregatableCoin<T0>'
      return:
      - 0x1::coin::Coin<T0>
    - name: ensure_paired_metadata
      visibility: friend
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params: []
      return:
      - 0x1::object::Object<0x1::fungible_asset::Metadata>
    - name: extract
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&mut 0x1::coin::Coin<T0>'
      - u64
      return:
      - 0x1::coin::Coin<T0>
    - name: extract_all
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&mut 0x1::coin::Coin<T0>'
      return:
      - 0x1::coin::Coin<T0>
    - name: force_deposit
      visibility: friend
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - address
      - 0x1::coin::Coin<T0>
      return: []
    - name: freeze_coin_store
      visibility: public
      is_entry: true
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - address
      - '&0x1::coin::FreezeCapability<T0>'
      return: []
    - name: get_paired_burn_ref
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&0x1::coin::BurnCapability<T0>'
      return:
      - 0x1::fungible_asset::BurnRef
      - 0x1::coin::BurnRefReceipt
    - name: get_paired_mint_ref
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&0x1::coin::MintCapability<T0>'
      return:
      - 0x1::fungible_asset::MintRef
      - 0x1::coin::MintRefReceipt
    - name: get_paired_transfer_ref
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&0x1::coin::FreezeCapability<T0>'
      return:
      - 0x1::fungible_asset::TransferRef
      - 0x1::coin::TransferRefReceipt
    - name: initialize
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&signer'
      - 0x1::string::String
      - 0x1::string::String
      - u8
      - bool
      return:
      - 0x1::coin::BurnCapability<T0>
      - 0x1::coin::FreezeCapability<T0>
      - 0x1::coin::MintCapability<T0>
    - name: initialize_aggregatable_coin
      visibility: friend
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&signer'
      return:
      - 0x1::coin::AggregatableCoin<T0>
    - name: initialize_supply_config
      visibility: friend
      is_entry: false
      is_view: false
      generic_type_params: []
      params:
      - '&signer'
      return: []
    - name: initialize_with_parallelizable_supply
      visibility: friend
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&signer'
      - 0x1::string::String
      - 0x1::string::String
      - u8
      - bool
      return:
      - 0x1::coin::BurnCapability<T0>
      - 0x1::coin::FreezeCapability<T0>
      - 0x1::coin::MintCapability<T0>
    - name: initialize_with_parallelizable_supply_with_limit
      visibility: friend
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&signer'
      - 0x1::string::String
      - 0x1::string::String
      - u8
      - bool
      - u128
      return:
      - 0x1::coin::BurnCapability<T0>
      - 0x1::coin::FreezeCapability<T0>
      - 0x1::coin::MintCapability<T0>
    - name: is_account_registered
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params:
      - address
      return:
      - bool
    - name: is_aggregatable_coin_zero
      visibility: friend
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&0x1::coin::AggregatableCoin<T0>'
      return:
      - bool
    - name: is_balance_at_least
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params:
      - address
      - u64
      return:
      - bool
    - name: is_coin_initialized
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params: []
      return:
      - bool
    - name: is_coin_store_frozen
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params:
      - address
      return:
      - bool
    - name: merge
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&mut 0x1::coin::Coin<T0>'
      - 0x1::coin::Coin<T0>
      return: []
    - name: merge_aggregatable_coin
      visibility: friend
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&mut 0x1::coin::AggregatableCoin<T0>'
      - 0x1::coin::Coin<T0>
      return: []
    - name: migrate_to_fungible_store
      visibility: public
      is_entry: true
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&signer'
      return: []
    - name: mint
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - u64
      - '&0x1::coin::MintCapability<T0>'
      return:
      - 0x1::coin::Coin<T0>
    - name: name
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params: []
      return:
      - 0x1::string::String
    - name: paired_burn_ref_exists
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params: []
      return:
      - bool
    - name: paired_coin
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params: []
      params:
      - 0x1::object::Object<0x1::fungible_asset::Metadata>
      return:
      - 0x1::option::Option<0x1::type_info::TypeInfo>
    - name: paired_metadata
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params: []
      return:
      - 0x1::option::Option<0x1::object::Object<0x1::fungible_asset::Metadata>>
    - name: paired_mint_ref_exists
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params: []
      return:
      - bool
    - name: paired_transfer_ref_exists
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params: []
      return:
      - bool
    - name: register
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&signer'
      return: []
    - name: return_paired_burn_ref
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params: []
      params:
      - 0x1::fungible_asset::BurnRef
      - 0x1::coin::BurnRefReceipt
      return: []
    - name: return_paired_mint_ref
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params: []
      params:
      - 0x1::fungible_asset::MintRef
      - 0x1::coin::MintRefReceipt
      return: []
    - name: return_paired_transfer_ref
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params: []
      params:
      - 0x1::fungible_asset::TransferRef
      - 0x1::coin::TransferRefReceipt
      return: []
    - name: supply
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params: []
      return:
      - 0x1::option::Option<u128>
    - name: symbol
      visibility: public
      is_entry: false
      is_view: true
      generic_type_params:
      - constraints: []
      params: []
      return:
      - 0x1::string::String
    - name: transfer
      visibility: public
      is_entry: true
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&signer'
      - address
      - u64
      return: []
    - name: unfreeze_coin_store
      visibility: public
      is_entry: true
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - address
      - '&0x1::coin::FreezeCapability<T0>'
      return: []
    - name: upgrade_supply
      visibility: public
      is_entry: true
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&signer'
      return: []
    - name: value
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&0x1::coin::Coin<T0>'
      return:
      - u64
    - name: withdraw
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params:
      - '&signer'
      - u64
      return:
      - 0x1::coin::Coin<T0>
    - name: zero
      visibility: public
      is_entry: false
      is_view: false
      generic_type_params:
      - constraints: []
      params: []
      return:
      - 0x1::coin::Coin<T0>
    structs:
    - name: AggregatableCoin
      is_native: false
      abilities:
      - store
      generic_type_params:
      - constraints: []
        is_phantom: true
      fields:
      - name: value
        type: 0x1::aggregator::Aggregator
    - name: BurnCapability
      is_native: false
      abilities:
      - copy
      - store
      generic_type_params:
      - constraints: []
        is_phantom: true
      fields:
      - name: dummy_field
        type: bool
    - name: BurnRefReceipt
      is_native: false
      abilities: []
      generic_type_params: []
      fields:
      - name: metadata
        type: 0x1::object::Object<0x1::fungible_asset::Metadata>
    - name: Coin
      is_native: false
      abilities:
      - store
      generic_type_params:
      - constraints: []
        is_phantom: true
      fields:
      - name: value
        type: u64
    - name: CoinConversionMap
      is_native: false
      abilities:
      - key
      generic_type_params: []
      fields:
      - name: coin_to_fungible_asset_map
        type: 0x1::table::Table<0x1::type_info::TypeInfo, 0x1::object::Object<0x1::fungible_asset::Metadata>>
    - name: CoinDeposit
      is_native: false
      abilities:
      - drop
      - store
      generic_type_params: []
      fields:
      - name: coin_type
        type: 0x1::string::String
      - name: account
        type: address
      - name: amount
        type: u64
    - name: CoinEventHandleDeletion
      is_native: false
      abilities:
      - drop
      - store
      generic_type_params: []
      fields:
      - name: event_handle_creation_address
        type: address
      - name: deleted_deposit_event_handle_creation_number
        type: u64
      - name: deleted_withdraw_event_handle_creation_number
        type: u64
    - name: CoinInfo
      is_native: false
      abilities:
      - key
      generic_type_params:
      - constraints: []
        is_phantom: true
      fields:
      - name: name
        type: 0x1::string::String
      - name: symbol
        type: 0x1::string::String
      - name: decimals
        type: u8
      - name: supply
        type: 0x1::option::Option<0x1::optional_aggregator::OptionalAggregator>
    - name: CoinStore
      is_native: false
      abilities:
      - key
      generic_type_params:
      - constraints: []
        is_phantom: true
      fields:
      - name: coin
        type: 0x1::coin::Coin<T0>
      - name: frozen
        type: bool
      - name: deposit_events
        type: 0x1::event::EventHandle<0x1::coin::DepositEvent>
      - name: withdraw_events
        type: 0x1::event::EventHandle<0x1::coin::WithdrawEvent>
    - name: CoinWithdraw
      is_native: false
      abilities:
      - drop
      - store
      generic_type_params: []
      fields:
      - name: coin_type
        type: 0x1::string::String
      - name: account
        type: address
      - name: amount
        type: u64
    - name: Deposit
      is_native: false
      abilities:
      - drop
      - store
      generic_type_params:
      - constraints: []
        is_phantom: true
      fields:
      - name: account
        type: address
      - name: amount
        type: u64
    - name: DepositEvent
      is_native: false
      abilities:
      - drop
      - store
      generic_type_params: []
      fields:
      - name: amount
        type: u64
    - name: FreezeCapability
      is_native: false
      abilities:
      - copy
      - store
      generic_type_params:
      - constraints: []
        is_phantom: true
      fields:
      - name: dummy_field
        type: bool
    - name: MigrationFlag
      is_native: false
      abilities:
      - key
      generic_type_params: []
      fields:
      - name: dummy_field
        type: bool
    - name: MintCapability
      is_native: false
      abilities:
      - copy
      - store
      generic_type_params:
      - constraints: []
        is_phantom: true
      fields:
      - name: dummy_field
        type: bool
    - name: MintRefReceipt
      is_native: false
      abilities: []
      generic_type_params: []
      fields:
      - name: metadata
        type: 0x1::object::Object<0x1::fungible_asset::Metadata>
    - name: PairCreation
      is_native: false
      abilities:
      - drop
      - store
      generic_type_params: []
      fields:
      - name: coin_type
        type: 0x1::type_info::TypeInfo
      - name: fungible_asset_metadata_address
        type: address
    - name: PairedCoinType
      is_native: false
      abilities:
      - key
      generic_type_params: []
      fields:
      - name: type
        type: 0x1::type_info::TypeInfo
    - name: PairedFungibleAssetRefs
      is_native: false
      abilities:
      - key
      generic_type_params: []
      fields:
      - name: mint_ref_opt
        type: 0x1::option::Option<0x1::fungible_asset::MintRef>
      - name: transfer_ref_opt
        type: 0x1::option::Option<0x1::fungible_asset::TransferRef>
      - name: burn_ref_opt
        type: 0x1::option::Option<0x1::fungible_asset::BurnRef>
    - name: SupplyConfig
      is_native: false
      abilities:
      - key
      generic_type_params: []
      fields:
      - name: allow_upgrades
        type: bool
    - name: TransferRefReceipt
      is_native: false
      abilities: []
      generic_type_params: []
      fields:
      - name: metadata
        type: 0x1::object::Object<0x1::fungible_asset::Metadata>
    - name: Withdraw
      is_native: false
      abilities:
      - drop
      - store
      generic_type_params:
      - constraints: []
        is_phantom: true
      fields:
      - name: account
        type: address
      - name: amount
        type: u64
    - name: WithdrawEvent
      is_native: false
      abilities:
      - drop
      - store
      generic_type_params: []
      fields:
      - name: amount
        type: u64
    "###);

    // Account info v2
    let path = format!("/rpc/v2/accounts/{}", sender_account);
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::assert_snapshot!(&resp_body, @r###"
    sequence_number: 1
    authentication_key: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
    "###);

    let path: String = format!("/rpc/v2/accounts/{}/modules/{}", "0x1", "coin");
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;
    insta::assert_snapshot!(&resp_body, @r###"
    bytecode: 0xa11ceb0b060000000f0100260226bd0103e301960704f908c80105c10abd0807fe12e71908e52c2006852d9803109d30a6160ac346d8010b9b480e0ca948f8200da169200ec169140fd569060003000400050006000700080009000a000b000c000d000e000f00100011001200130014001500160401000100170501000100180000001904010001001a0800001b0600001c0600001d08010001001e08010001001f060000200601000100210600002205010001002308000024050100010025000000260600002708000028080000290800002a0000002b06010001002c06000b330701000008350000083906000a440701000108450b00084d0600084f06000f5107001267070002780400117c0402030100010c820104000685010401060108960108000aa401020009c3010600002d000100002e02030100002f04010100003005010100003106030100003201020100003401070100003606080100003706080100003809010100003a0a0b0100003b0c0100003c0c010100003d010d0100003e0e010100003f0a01010000400f01010000411001010000420601010000431106010000460112010000471306010000481406010000490e010100004a15010100004b08060100004c16170100004e1819010000501a1b010000521c1d010000530c1e010000541f1d01000055201d010000560c010000571c1d01000058211d0100005902220100005a23220100005b24220100005c01220100005d02220100005e02010100005f2501010000602601010000610c01010000620c01010000632706010000640306010000650128010000660122010000681229000069012a0100006a01220100006b01220100006c0c010100006d170100006e190100006f1b010000700107010000710128010000722b01010000731501010000740c01010000752c03010000762d060100007701060100108d010c01000b8e01312201000b47323001000d2e340301080b8f01363001000d9001342201080591010303000592010303000a9301370201080594010303000b9501313801000d97013439010808303b0101080b9801323e01000c99013f0100129a0101400100129b014102000c9c014344000b9d01303601000b9e0101360100079f0101220005a00103030008644608000da1013402010808a20124080011a301014a02030405a50103030011a6014c2202030012a701012801000fa8014d28000aa9014e4f0004070250000faa015152000aab01534f000dac015401000aad015550000aae015556010811af01570102030006b0013001010608b10155590008b201555a0008b301550b001195014c5b02030005b40103030007b50101220006b6015f01010608b70102220007b8010122000d3e600100029c0161440005b90103030002990162010008ba0164120008bb0160010008bc016412000abd0102220108083108030003be016a6b000ebf010c020005c0010303000fc1015103000ca3016d3d000d5b702201080dc20134390108060c7273010609c40173020009c50173030006c60176010106083e7701010808c7015622010808c8017801010802af016201000caf013f010001c9010201010001ca010c76010608cb017e12000bcc017f01010008cd018001120008ce01810112000870560701080ccf014322000cd001840101000d7687010801083330431244124533043046124733153002304a33430b4c0b4d334e3a0530433d4f3d51304c3d54445544083014304130593319302a302b300f30440b5b495d495e3030303b300d306633674968585459545a540b6c49685d6f5e4a3a79672f3043594459435a445a1f30543d553d20302730800133243081013382015e820174687585015e850174123086013a87013a88013a2d30293054405540541255128b01308c015e8c01748e010b8e01598e015a063091013343444f44464440300e306886016f7494013302060c010001050103020b03010900060b01010900030503060b01010900010b03010900010b170104010818030503070b00010900010b0101090001081901060c010202050b03010900010b0c010900010b0e01090001070b00010900010b1a01081b02070b030109000301070b030109000205060b0c01090001060b01010900020819080201060b0e01090002081c080f01060b0c01090002081d081405060c081e081e0201030b010109000b0c0109000b0e010900010b0001090006060c081e081e02010107060c081e081e0201010406060c081e081e020104010101060b0001090002050302070b030109000b0301090002070b000109000b030109000203060b0e01090001081e010b1701081f010b17010b1a01081b03060c050301060b0301090002060c03010701040303050b17010b1a01081b01090001060b1701090001070b1701090001081b02050b1a0109000f030103030503070b170108190303030b17010b1a01081b0b1a01081b050b17010b1a01081b05010b1701090001060b1a01090001060900010b1a010824010824030608190b1a010900030203070b1701082201082201070900020708220401081f0106081f020b170104060b1701082201060822010402030b1a01081b020b1a01081b030e030103030b0301090005030b0301090003030818030b17010b1a01081b0504070b170108190b1a01081b050b17010b1a01081b02081f0b1a01081b010b2102090009010f081e0c060c08250c0819010708040b1a01081b0825060c081c081d081f081f02060b2102090009010900010a02020505010825010c0106081e01060a0202060c0a02070608250b170104081e081e02081e081e01060825010b1a01090003070b2102090009010900090101081001081c01081d0106090106010101070b080109000b17010b1a01081b0501080501080b02070b230109000900020508180106082002070820040801010108180b17010b1a01081b0b1a01081b050b1a0108240106081801070b08010900020b1a01081b05010811040b1a01081b050b17010b1a01081b070b1701081c040b1a01081b050b17010b1a01081b070b1701081d02060c04010820060b1701082202081e081e050b0701090002040106010101010b17010b1a01081b050603010303050b17010b1a01081b03050b1a01090003080c0b030109000b2301080b010b1a01081b0b1a010824050b2301081601060b2301090001060826010816010806010b23010900020b1a0109000818020b1a0109000102070b17010822070822030b1a01081b050b17010b1a01081b020b1701081f050301060b2102081f0b1a01081b081f02050b080109000106081902070b1701090009000106081c0106081d040b1701040b1701040b17010b1a01081b07040305070b170108220708220107082210030103030b030109000505030303070b080109000818030b17010b1a01081b050b0301090001080903060c0b1a010900030767656e657369730a73757072615f636f696e0f7472616e73616374696f6e5f66656504636f696e076163636f756e740a61676772656761746f721261676772656761746f725f666163746f72790d6372656174655f7369676e6572056572726f72056576656e740866656174757265730e66756e6769626c655f61737365740467756964066f626a656374066f7074696f6e136f7074696f6e616c5f61676772656761746f72167072696d6172795f66756e6769626c655f73746f7265067369676e657206737472696e671073797374656d5f616464726573736573057461626c6509747970655f696e666f10416767726567617461626c65436f696e0e4275726e4361706162696c6974790e4275726e5265665265636569707404436f696e11436f696e436f6e76657273696f6e4d61700b436f696e4465706f73697417436f696e4576656e7448616e646c6544656c6574696f6e08436f696e496e666f09436f696e53746f72650c436f696e5769746864726177074465706f7369740c4465706f7369744576656e7410467265657a654361706162696c6974790d4d6967726174696f6e466c61670e4d696e744361706162696c6974790e4d696e74526566526563656970740c506169724372656174696f6e0e506169726564436f696e547970651750616972656446756e6769626c654173736574526566730c537570706c79436f6e666967125472616e73666572526566526563656970740857697468647261770d57697468647261774576656e7415616c6c6f775f737570706c795f75706772616465730762616c616e6365046275726e096275726e5f66726f6d0d6275726e5f696e7465726e616c0c636f696e5f61646472657373064f7074696f6e0b636f696e5f737570706c790d46756e6769626c65417373657416636f696e5f746f5f66756e6769626c655f61737365741f636f696e5f746f5f66756e6769626c655f61737365745f696e7465726e616c1e636f6c6c6563745f696e746f5f616767726567617461626c655f636f696e074275726e52656620636f6e766572745f616e645f74616b655f7061697265645f6275726e5f7265661a6372656174655f636f696e5f636f6e76657273696f6e5f6d61700e6372656174655f70616972696e6708646563696d616c73076465706f7369741064657374726f795f6275726e5f6361701264657374726f795f667265657a655f6361701064657374726f795f6d696e745f6361700c64657374726f795f7a65726f17647261696e5f616767726567617461626c655f636f696e064f626a656374084d6574616461746116656e737572655f7061697265645f6d6574616461746107657874726163740b657874726163745f616c6c0d666f7263655f6465706f73697411667265657a655f636f696e5f73746f72651666756e6769626c655f61737365745f746f5f636f696e136765745f7061697265645f6275726e5f726566074d696e74526566136765745f7061697265645f6d696e745f7265660b5472616e73666572526566176765745f7061697265645f7472616e736665725f72656606537472696e670a696e697469616c697a651c696e697469616c697a655f616767726567617461626c655f636f696e13696e697469616c697a655f696e7465726e616c1e696e697469616c697a655f696e7465726e616c5f776974685f6c696d697418696e697469616c697a655f737570706c795f636f6e66696725696e697469616c697a655f776974685f706172616c6c656c697a61626c655f737570706c7930696e697469616c697a655f776974685f706172616c6c656c697a61626c655f737570706c795f776974685f6c696d69741569735f6163636f756e745f726567697374657265641969735f616767726567617461626c655f636f696e5f7a65726f1369735f62616c616e63655f61745f6c656173741369735f636f696e5f696e697469616c697a65641469735f636f696e5f73746f72655f66726f7a656e1f6d617962655f636f6e766572745f746f5f66756e6769626c655f73746f7265056d65726765176d657267655f616767726567617461626c655f636f696e196d6967726174655f746f5f66756e6769626c655f73746f7265226d6967726174655f746f5f66756e6769626c655f73746f72655f696e7465726e616c046d696e740d6d696e745f696e7465726e616c046e616d65167061697265645f6275726e5f7265665f6578697374730854797065496e666f0b7061697265645f636f696e0f7061697265645f6d65746164617461167061697265645f6d696e745f7265665f6578697374731a7061697265645f7472616e736665725f7265665f6578697374730872656769737465721672657475726e5f7061697265645f6275726e5f7265661672657475726e5f7061697265645f6d696e745f7265661a72657475726e5f7061697265645f7472616e736665725f72656606737570706c790673796d626f6c087472616e7366657213756e667265657a655f636f696e5f73746f72650e757067726164655f737570706c790576616c7565087769746864726177047a65726f0a41676772656761746f720b64756d6d795f6669656c64086d657461646174611a636f696e5f746f5f66756e6769626c655f61737365745f6d6170055461626c6509636f696e5f7479706506616d6f756e741d6576656e745f68616e646c655f6372656174696f6e5f616464726573732c64656c657465645f6465706f7369745f6576656e745f68616e646c655f6372656174696f6e5f6e756d6265722d64656c657465645f77697468647261775f6576656e745f68616e646c655f6372656174696f6e5f6e756d626572124f7074696f6e616c41676772656761746f720666726f7a656e0e6465706f7369745f6576656e74730b4576656e7448616e646c650f77697468647261775f6576656e74731f66756e6769626c655f61737365745f6d657461646174615f6164647265737304747970650c6d696e745f7265665f6f7074107472616e736665725f7265665f6f70740c6275726e5f7265665f6f70740e616c6c6f775f7570677261646573166173736572745f73757072615f6672616d65776f726b0769735f736f6d650c64657374726f795f736f6d65147072696d6172795f73746f72655f65786973747310696e76616c69645f617267756d656e74096e6f745f666f756e640e6f626a6563745f6164647265737308696e7465726e616c06626f72726f770d46756e6769626c6553746f72650d7072696d6172795f73746f72650a626f72726f775f6d75740373756207747970655f6f660f6163636f756e745f61646472657373047265616404736f6d65046e6f6e6530636f696e5f746f5f66756e6769626c655f61737365745f6d6967726174696f6e5f666561747572655f656e61626c65640b756e617661696c61626c65157072696d6172795f73746f72655f616464726573731177697468647261775f696e7465726e616c036e65770e436f6e7374727563746f725265660d696e76616c69645f737461746508636f6e7461696e7309747970655f6e616d6504757466381f6372656174655f737469636b795f6f626a6563745f61745f61646472657373056279746573136372656174655f6e616d65645f6f626a6563742b6372656174655f7072696d6172795f73746f72655f656e61626c65645f66756e6769626c655f61737365740f67656e65726174655f7369676e65721b6f626a6563745f66726f6d5f636f6e7374727563746f725f7265660361646404656d69741167656e65726174655f6d696e745f7265661567656e65726174655f7472616e736665725f7265661167656e65726174655f6275726e5f726566117065726d697373696f6e5f64656e6965641e6d6f64756c655f6576656e745f6d6967726174696f6e5f656e61626c65640a656d69745f6576656e740c73746f72655f6578697374732e6e65775f6163636f756e74735f64656661756c745f746f5f66615f73757072615f73746f72655f656e61626c65640c6f75745f6f665f72616e67650e61737365745f6d65746164617461106465706f7369745f696e7465726e616c136d657461646174615f66726f6d5f61737365740d6f626a6563745f657869737473116372656174655f61676772656761746f720a616464726573735f6f660e616c72656164795f657869737473066c656e6774681b656e737572655f7072696d6172795f73746f72655f65786973747304475549440f63726561746f725f616464726573730c6372656174696f6e5f6e756d0e64657374726f795f68616e646c650969735f66726f7a656e187365745f66726f7a656e5f666c61675f696e7465726e616c0d72656769737465725f636f696e106e65775f6576656e745f68616e646c65116275726e5f7265665f6d657461646174610466696c6c116d696e745f7265665f6d65746164617461157472616e736665725f7265665f6d657461646174611169735f706172616c6c656c697a61626c6506737769746368000000000000000000000000000000000000000000000000000000000000000103080e0000000000000003081c00000000000000030819000000000000000308180000000000000003081b0000000000000003080100000000000000030802000000000000000308030000000000000003080c00000000000000030804000000000000000308050000000000000003080b0000000000000003080d0000000000000003081200000000000000030811000000000000000308070000000000000003080a000000000000000308060000000000000003081a00000000000000030815000000000000000308140000000000000003080f0000000000000003081000000000000000030813000000000000000308170000000000000003081600000000000000030820000000000000000410ffffffffffffffffffffffffffffffff0410ffffffffffffffff0000000000000000052000000000000000000000000000000000000000000000000000000000000000010a021b1a3078313a3a73757072615f636f696e3a3a5375707261436f696e0520000000000000000000000000000000000000000000000000000000000000000a0a020100126170746f733a3a6d657461646174615f763191161a01000000000000001b45434f494e5f494e464f5f414444524553535f4d49534d415443486541646472657373206f66206163636f756e74207768696368206973207573656420746f20696e697469616c697a65206120636f696e2060436f696e547970656020646f65736e2774206d6174636820746865206465706c6f796572206f66206d6f64756c6502000000000000001c45434f494e5f494e464f5f414c52454144595f5055424c49534845442b60436f696e547970656020697320616c726561647920696e697469616c697a6564206173206120636f696e03000000000000001845434f494e5f494e464f5f4e4f545f5055424c49534845442c60436f696e5479706560206861736e2774206265656e20696e697469616c697a6564206173206120636f696e04000000000000001d45434f494e5f53544f52455f414c52454144595f5055424c495348454445446570726563617465642e204163636f756e7420616c7265616479206861732060436f696e53746f726560207265676973746572656420666f722060436f696e547970656005000000000000001945434f494e5f53544f52455f4e4f545f5055424c4953484544344163636f756e74206861736e277420726567697374657265642060436f696e53746f72656020666f722060436f696e547970656006000000000000001545494e53554646494349454e545f42414c414e4345284e6f7420656e6f75676820636f696e7320746f20636f6d706c657465207472616e73616374696f6e07000000000000001d454445535452554354494f4e5f4f465f4e4f4e5a45524f5f544f4b454e1d43616e6e6f742064657374726f79206e6f6e2d7a65726f20636f696e730a00000000000000074546524f5a454e3b436f696e53746f72652069732066726f7a656e2e20436f696e732063616e6e6f74206265206465706f7369746564206f722077697468647261776e0b000000000000002245434f494e5f535550504c595f555047524144455f4e4f545f535550504f525445444543616e6e6f7420757067726164652074686520746f74616c20737570706c79206f6620636f696e7320746f20646966666572656e7420696d706c656d656e746174696f6e2e0c000000000000001345434f494e5f4e414d455f544f4f5f4c4f4e471c4e616d65206f662074686520636f696e20697320746f6f206c6f6e670d000000000000001545434f494e5f53594d424f4c5f544f4f5f4c4f4e471e53796d626f6c206f662074686520636f696e20697320746f6f206c6f6e670e000000000000002245414747524547415441424c455f434f494e5f56414c55455f544f4f5f4c415247455c5468652076616c7565206f6620616767726567617461626c6520636f696e207573656420666f72207472616e73616374696f6e2066656573207265646973747269627574696f6e20646f6573206e6f742066697420696e207536342e0f000000000000000c455041495245445f434f494e404572726f7220726567617264696e672070616972656420636f696e2074797065206f66207468652066756e6769626c65206173736574206d657461646174612e100000000000000016455041495245445f46554e4749424c455f41535345543e4572726f7220726567617264696e67207061697265642066756e6769626c65206173736574206d65746164617461206f66206120636f696e20747970652e11000000000000001345434f494e5f545950455f4d49534d415443484d54686520636f696e20747970652066726f6d20746865206d617020646f6573206e6f74206d61746368207468652063616c6c696e672066756e6374696f6e207479706520617267756d656e742e12000000000000002b45434f494e5f544f5f46554e4749424c455f41535345545f464541545552455f4e4f545f454e41424c4544445468652066656174757265206f66206d6967726174696f6e2066726f6d20636f696e20746f2066756e6769626c65206173736574206973206e6f7420656e61626c65642e130000000000000025455041495245445f46554e4749424c455f41535345545f524546535f4e4f545f464f554e443050616972656446756e6769626c65417373657452656673207265736f7572636520646f6573206e6f742065786973742e14000000000000001a454d494e545f5245465f524543454950545f4d49534d415443483d546865204d696e745265665265636569707420646f6573206e6f74206d6174636820746865204d696e7452656620746f2062652072657475726e65642e150000000000000013454d494e545f5245465f4e4f545f464f554e441b546865204d696e7452656620646f6573206e6f742065786973742e16000000000000001e455452414e534645525f5245465f524543454950545f4d49534d4154434845546865205472616e736665725265665265636569707420646f6573206e6f74206d6174636820746865205472616e7366657252656620746f2062652072657475726e65642e170000000000000017455452414e534645525f5245465f4e4f545f464f554e441f546865205472616e7366657252656620646f6573206e6f742065786973742e18000000000000001a454255524e5f5245465f524543454950545f4d49534d415443483d546865204275726e5265665265636569707420646f6573206e6f74206d6174636820746865204275726e52656620746f2062652072657475726e65642e190000000000000013454255524e5f5245465f4e4f545f464f554e441b546865204275726e52656620646f6573206e6f742065786973742e1a0000000000000020454d4947524154494f4e5f4652414d45574f524b5f4e4f545f454e41424c454445546865206d6967726174696f6e2070726f636573732066726f6d20636f696e20746f2066756e6769626c65206173736574206973206e6f7420656e61626c6564207965742e1b000000000000001e45434f494e5f434f4e56455253494f4e5f4d41505f4e4f545f464f554e442b54686520636f696e20636f6e76657269736f6e206d6170206973206e6f742063726561746564207965742e1c000000000000001b454150545f50414952494e475f49535f4e4f545f454e41424c45442153555052412070616972696e67206973206e6f742065616e626c6564207965742e09074465706f7369740104000857697468647261770104000b436f696e4465706f7369740104000c436f696e57697468647261770104000c506169724372656174696f6e0104000d4d6967726174696f6e466c6167010301183078313a3a6f626a6563743a3a4f626a65637447726f75700e506169726564436f696e54797065010301183078313a3a6f626a6563743a3a4f626a65637447726f757017436f696e4576656e7448616e646c6544656c6574696f6e0104001750616972656446756e6769626c65417373657452656673010301183078313a3a6f626a6563743a3a4f626a65637447726f75700f046e616d6501010006737570706c790101000673796d626f6c0101000762616c616e636501010008646563696d616c730101000b636f696e5f737570706c790101000b7061697265645f636f696e0101000f7061697265645f6d657461646174610101001369735f62616c616e63655f61745f6c656173740101001369735f636f696e5f696e697469616c697a65640101001469735f636f696e5f73746f72655f66726f7a656e0101001569735f6163636f756e745f72656769737465726564010100167061697265645f6275726e5f7265665f657869737473010100167061697265645f6d696e745f7265665f6578697374730101001a7061697265645f7472616e736665725f7265665f65786973747301010000020175082001020179010202017a0b1a01081b03020175030402017b0b2102081f0b1a01081b0502037d081e04057e030602037f0580010381010307020465081e71081e3d02700b17010822080204030b0301090083010184010b2301080b86010b230108160902037d081e04057e030a020204057e030b02017e030c020179010d020179010e020179010f02017a0b1a01081b1002027d081f8701051102018801081f12020389010b1701081c8a010b1701081d8b010b170108191302018c01011402017a0b1a01081b15020204057e031602017e0308300330073001300c300e30003000010001132e0a0b001142071d2a130f000c020b010b0215020101000204082f1f38000c040a000c030a033b00040e0b033d0037003701140c0105100600000000000000000c010e04380104190b000d04380238030c02051b0600000000000000000c020b010b021602020100010701040b003804010203010004040708123585010a010600000000000000002104070b0201020a000b010c080c070a070c110a113b0004170b113d0037003701140c0305190600000000000000000c030b030c0b0a0b0a082604240b080600000000000000000c060c05053f38000c0d0e0d3801042f0b070b0d380538060c040531090c040b04043405390b020107111148270a0b0b080b0b170c060c050b050b060c0c0c0a0a0a06000000000000000022044e0a003c0036000b0a38070a0238080a0c060000000000000000220482010b020138000c100e103801045a055d07161149270b1038050c0e0e0e38090c0f0a0f29120467056a0717114b270b0f2a120f030c090a092e380a047305780b090107021149270b092e380b0b0038003805380c0b0c380d0584010b02010204000001073c1a280b003a010c010a01060000000000000000220418380e3c0236020c020a022e380f04160b0238100a0135115005180b02010b010205000000400538110c000e0011520206010001074213380e3d0237020c010a01380f040d0b013812115338130c0005110b010138140c000b0002070100020407010811560405070d1157270b00381502080000020407450938160c020b0038040c010b020b01115802090300040407081147650a010600000000000000002104070b0201020a000b010c090c080a080c100a103b0004170b103d0037003701140c0305190600000000000000000c030b030c0c0a0c0a092604240b090600000000000000000c060c05053f38000c0f0e0f3801042f0b080b0f380538060c040531090c040b04043405390b020107111148270a0c0b090b0c170c060c050b050b060c0e0c0b0a0b06000000000000000022044e0a003c0036000b0b38070c07055038170c070b070c0a0a0e0600000000000000002204610b003800380538180b0e115a0c0d0d0a0b0d3819381a0b020b0a381b020a010002041248290b00381c38000c040e0438010408050b07161149270b0438050c020e0238090c030a032912041505180717114b270b032a120f030c010a012e380a042105260b010107021149270b01381d020b010400010e0a001142071d290420040b0b00381e12042d04050d0b0001020c01040204074b7b0b0011421156040505080712115c27071d2904040c050f0704114927071d2a040c0838110c0e0a0810050a0e381f2004743820071e115f210c070a0720081e0425052a0b08010701115c270b070431071d071f11600c04053e071f11610c020e020c0338200c010b030e0111621411630c040b040c0a0e0a38143821382238230720115f0720115f11640e0a11650c050e050c0b38110c0f0a0b0a0f12112d110e0a38240c090a080f050a0f0a0938250b0f0e093809121038260e0a11690c0c0e0a116a0c0d0e0a116b0c060b0b0b0c38270b0d38280b06382912122d120b0810050b0e382a1401020d010001070105380e3d02370314020e0100030407085c500a003b0004260a003c000c050a0537041420040c05110b05010710116d27116e041a38200b000e013701141205382b0a0536050e01370114120b382c0b0536000b01381a054f38000c060e06380104430a000b06380538180c070a071170043e11710438080c02053b0b07290d0c020b020c030540090c030b030c040545090c040b040448054b070a1149270b000b0138151172020f01000001040b003a0301021001000001040b003a0401021101000001040b003a05010212010000010b280b003a01060000000000000000210407050a070f11482702130300004419280a00370611730c010a01071c25040a050f0b00010700117427280b0036060a011175280b01343901021403000204074b781156040305060712115c27071d2904040a050d0704114927071d2a040c0738110c0d0a0710050a0d381f2004723820071e115f210c060a0620091e042305280b07010701115c270b06042f071d071f11600c03053c071f11610c010e010c0238200c000b020e0011621411630c030b030c090e0938143821382238230720115f0720115f11640e0911650c040e040c0a38110c0e0a0a0a0e12112d110e0938240c080a070f050a0e0a0838250b0e0e083809121038260e0911690c0b0e09116a0c0c0e09116b0c050b0a0b0b38270b0c38280b05382912122d120b0710050b0d382a14021501000001190a003701140a01260407050c0b00010711114827280a003701140a01170b00360115280b0139010216010000030d0a003701140c01280600000000000000000b00360115280b0139010217030003040708633d0a003b0004090b003c0036000b01381a053c38000c060e06380104260a000b06380538180c080a08117004211171041b080c02051e0b08290d0c020b020c030523090c030b030c040528090c040b04042b052e070a1149270b0138150c050e0511760c070b000b07380c0c090e09382d0b05117702180104010865080b003c000c02080b0236041502190000020711661c0e0011780c010e0138090c020a02382e040a050d07151149270b022b11100a1438112104150518070e1148270b00117a382f021a0100020412482938000c040e0438010406050907161149270b0438050c020e0238090c030a032912041305160717114b270b032a120f030c010a012e380a041f05240b010107021149270b01381d0b021202021b0100020412682938000c030e0338010406050907161149270b0338050c010e0138090c020a022912041305160717114b270b022a120f0b0c040a042e3830041f05240b040107131149270b0438310b01120f021c0100020412692938000c030e0338010406050907161149270b0338050c010e0138090c020a022912041305160717114b270b022a120f0c0c040a042e3832041f05240b040107181149270b0438330b011214021d01000001080b000b010b020b030b04093834021e03000001050b00071c117b3906021f0000006c4d0a00117c0c0a380e0a0a210408050d0b000107051148270b0a3b0220041205170b00010706117d270e01117e071a25041d05220b000107081148270e02117e0710250428052d0b0001070c1148270b010c090b020c080b030c070b04043b071b0b05117f38350c06053d38360c060b090b080b070b0639020c0b0b000b0b3f0209390309390409390502200000006c4d0a00117c0c0b380e0a0b210408050d0b000107051148270b0b3b0220041205170b00010706117d270e01117e071a25041d05220b000107081148270e02117e0710250428052d0b0001070c1148270b010c0a0b020c090b030c080b04043b0b060b05117f38350c07053d38360c070b0a0b090b080b0739020c0c0b000b0c3f02093903093904093905022103000001070a0011420b000912132d130222030000010a0a0011420b000b010b020b030b040838340223030000010b0a0011420b000b010b020b030b04080b0538370224010001046e2f38380403050607071148270a003b00040c080c04052d38000c050e05380104290b000b05380538180c060a06117004241171041e080c0105210b06290d0c010b010c020526090c020b020c03052b090c030b030c040b04022503000001060b0037061173320000000000000000000000000000000021022601000204086f2a0a000c060a063b00040c0b063d0037003701140c02050e0600000000000000000c020b020c040a040a01260416080238000c070b010b04170c050e07380104260b000d0738020b0538390c030528090c030b0302270100000103380e3b0202280100020408010b0a00383a20040608020b003d003704140229000003040708714f1156200406070d11572738380409050c070711482738160c050a000b05383b0c060e06382d0c070a003b0004430b003e003a000c080c030c040c020e03383c1183010e03383c1184010e08383d1184011206383e0b03383f0b0838400e023701140600000000000000002104370b023841053b0a060b02381538420a040a0638432204430b060b0438440a07290d20044e0b0711610c010e0109120d2d0d022a010000030f28280b013a010c02280a003701140b02160b00360115022b030000440b280b013a01350c02280b0036060b02118901022c010403040708010a115604070b0001070d1157270b003845022d00000304070801040b00117c3846022e0100010701030b00382f022f00000107791e0a00060000000000000000210407060000000000000000390102380e3c0236020c010a012e380f04180b0138100c02280b020a0035118a01051a0b0101280b0039010230010001070105380e3d02370714023101000204127a1b38000c020e0238010406050907161149270b0238050c000e0038090c010a012912041305160717114b270b012b121003380a0232010001117b110e0038090c020a022911040d0b022b11100a1438470c01050f38480c010b010233010001047c1e071d2904040611560c000508090c000b00041c071d2b0410050c0138110c020a010a02381f041a0b010b02382a143849020b0101384a023401000204127a1b38000c020e0238010406050907161149270b0238050c000e0038090c010a012912041305160717114b270b012b12100b3830023501000204127a1b38000c020e0238010406050907161149270b0238050c000e0038090c010a012912041305160717114b270b012b12100c38320236010001047d180a00117c0c010a01383a04090b0001020b01384b0600000000000000003901090a00384c0a00384d39000c020b000b023f0002370100011212130b0113020c020e00118d010a02210409050c07031148270e0238092a120f030b00384e02380100011212130b01130f0c020e00118f010a02210409050c07141148270e0238092a120f0b0b00384f02390100011212130b0113140c020e001190010a02210409050c07191148270e0238092a120f0c0b003850023a010002040782011a38510c0038000c020e02380104180d02380238520c010e00385304180d0038540c030a03140b013855160b03150b00023b010001070105380e3d02370814023c0104040407081106080b000b0238560c030b010b033857023d0104010865080b003c000c02090b02360415023e010402071383012d0b00117c0c01380e0a01210408050b0705114827071d2a1310001404110514070b116d270b013c0236020c020a022e380f042a0b0238100c030a032e1192012004270b0311930105290b0301052c0b0201023f01000001040b0037011402400100040407081185017c0a00117c0c070a070b010c090c080a080c100a103b0004130b103d0037003701140c0205150600000000000000000c020b020c0b0a0b0a092604200b090600000000000000000c050c04053b38000c0f0e0f3801042b0b080b0f380538060c03052d090c030b03043005350b000107111148270a0b0b090b0b170c050c040b040b050c0e0c0a0a0a0600000000000000002204650a073c000c0c0a0c37041420044c05530b0c010b00010710116d27116e045a38200b070a0a120938580a0c36090a0a121638590b0c36000b0a38070c06056738170c060b060c110a0e0600000000000000002204780b00380038050b0e385a0c0d0d110b0d3819381a057a0b00010b110241010000010428060000000000000000390102130008000300120207030400070208010802000011001200120107000701080301300230043006300730083009300d300e300f3000000001000200
    abi:
      address: '0x1'
      name: coin
      friends:
      - 0x1::genesis
      - 0x1::supra_coin
      - 0x1::transaction_fee
      exposed_functions:
      - name: allow_supply_upgrades
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params: []
        params:
        - '&signer'
        - bool
        return: []
      - name: balance
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params:
        - address
        return:
        - u64
      - name: burn
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - 0x1::coin::Coin<T0>
        - '&0x1::coin::BurnCapability<T0>'
        return: []
      - name: burn_from
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - address
        - u64
        - '&0x1::coin::BurnCapability<T0>'
        return: []
      - name: coin_supply
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params: []
        return:
        - 0x1::option::Option<u128>
      - name: coin_to_fungible_asset
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - 0x1::coin::Coin<T0>
        return:
        - 0x1::fungible_asset::FungibleAsset
      - name: collect_into_aggregatable_coin
        visibility: friend
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - address
        - u64
        - '&mut 0x1::coin::AggregatableCoin<T0>'
        return: []
      - name: convert_and_take_paired_burn_ref
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - 0x1::coin::BurnCapability<T0>
        return:
        - 0x1::fungible_asset::BurnRef
      - name: create_coin_conversion_map
        visibility: public
        is_entry: true
        is_view: false
        generic_type_params: []
        params:
        - '&signer'
        return: []
      - name: create_pairing
        visibility: public
        is_entry: true
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&signer'
        return: []
      - name: decimals
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params: []
        return:
        - u8
      - name: deposit
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - address
        - 0x1::coin::Coin<T0>
        return: []
      - name: destroy_burn_cap
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - 0x1::coin::BurnCapability<T0>
        return: []
      - name: destroy_freeze_cap
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - 0x1::coin::FreezeCapability<T0>
        return: []
      - name: destroy_mint_cap
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - 0x1::coin::MintCapability<T0>
        return: []
      - name: destroy_zero
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - 0x1::coin::Coin<T0>
        return: []
      - name: drain_aggregatable_coin
        visibility: friend
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&mut 0x1::coin::AggregatableCoin<T0>'
        return:
        - 0x1::coin::Coin<T0>
      - name: ensure_paired_metadata
        visibility: friend
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params: []
        return:
        - 0x1::object::Object<0x1::fungible_asset::Metadata>
      - name: extract
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&mut 0x1::coin::Coin<T0>'
        - u64
        return:
        - 0x1::coin::Coin<T0>
      - name: extract_all
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&mut 0x1::coin::Coin<T0>'
        return:
        - 0x1::coin::Coin<T0>
      - name: force_deposit
        visibility: friend
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - address
        - 0x1::coin::Coin<T0>
        return: []
      - name: freeze_coin_store
        visibility: public
        is_entry: true
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - address
        - '&0x1::coin::FreezeCapability<T0>'
        return: []
      - name: get_paired_burn_ref
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&0x1::coin::BurnCapability<T0>'
        return:
        - 0x1::fungible_asset::BurnRef
        - 0x1::coin::BurnRefReceipt
      - name: get_paired_mint_ref
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&0x1::coin::MintCapability<T0>'
        return:
        - 0x1::fungible_asset::MintRef
        - 0x1::coin::MintRefReceipt
      - name: get_paired_transfer_ref
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&0x1::coin::FreezeCapability<T0>'
        return:
        - 0x1::fungible_asset::TransferRef
        - 0x1::coin::TransferRefReceipt
      - name: initialize
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&signer'
        - 0x1::string::String
        - 0x1::string::String
        - u8
        - bool
        return:
        - 0x1::coin::BurnCapability<T0>
        - 0x1::coin::FreezeCapability<T0>
        - 0x1::coin::MintCapability<T0>
      - name: initialize_aggregatable_coin
        visibility: friend
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&signer'
        return:
        - 0x1::coin::AggregatableCoin<T0>
      - name: initialize_supply_config
        visibility: friend
        is_entry: false
        is_view: false
        generic_type_params: []
        params:
        - '&signer'
        return: []
      - name: initialize_with_parallelizable_supply
        visibility: friend
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&signer'
        - 0x1::string::String
        - 0x1::string::String
        - u8
        - bool
        return:
        - 0x1::coin::BurnCapability<T0>
        - 0x1::coin::FreezeCapability<T0>
        - 0x1::coin::MintCapability<T0>
      - name: initialize_with_parallelizable_supply_with_limit
        visibility: friend
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&signer'
        - 0x1::string::String
        - 0x1::string::String
        - u8
        - bool
        - u128
        return:
        - 0x1::coin::BurnCapability<T0>
        - 0x1::coin::FreezeCapability<T0>
        - 0x1::coin::MintCapability<T0>
      - name: is_account_registered
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params:
        - address
        return:
        - bool
      - name: is_aggregatable_coin_zero
        visibility: friend
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&0x1::coin::AggregatableCoin<T0>'
        return:
        - bool
      - name: is_balance_at_least
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params:
        - address
        - u64
        return:
        - bool
      - name: is_coin_initialized
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params: []
        return:
        - bool
      - name: is_coin_store_frozen
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params:
        - address
        return:
        - bool
      - name: merge
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&mut 0x1::coin::Coin<T0>'
        - 0x1::coin::Coin<T0>
        return: []
      - name: merge_aggregatable_coin
        visibility: friend
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&mut 0x1::coin::AggregatableCoin<T0>'
        - 0x1::coin::Coin<T0>
        return: []
      - name: migrate_to_fungible_store
        visibility: public
        is_entry: true
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&signer'
        return: []
      - name: mint
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - u64
        - '&0x1::coin::MintCapability<T0>'
        return:
        - 0x1::coin::Coin<T0>
      - name: name
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params: []
        return:
        - 0x1::string::String
      - name: paired_burn_ref_exists
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params: []
        return:
        - bool
      - name: paired_coin
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params: []
        params:
        - 0x1::object::Object<0x1::fungible_asset::Metadata>
        return:
        - 0x1::option::Option<0x1::type_info::TypeInfo>
      - name: paired_metadata
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params: []
        return:
        - 0x1::option::Option<0x1::object::Object<0x1::fungible_asset::Metadata>>
      - name: paired_mint_ref_exists
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params: []
        return:
        - bool
      - name: paired_transfer_ref_exists
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params: []
        return:
        - bool
      - name: register
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&signer'
        return: []
      - name: return_paired_burn_ref
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params: []
        params:
        - 0x1::fungible_asset::BurnRef
        - 0x1::coin::BurnRefReceipt
        return: []
      - name: return_paired_mint_ref
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params: []
        params:
        - 0x1::fungible_asset::MintRef
        - 0x1::coin::MintRefReceipt
        return: []
      - name: return_paired_transfer_ref
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params: []
        params:
        - 0x1::fungible_asset::TransferRef
        - 0x1::coin::TransferRefReceipt
        return: []
      - name: supply
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params: []
        return:
        - 0x1::option::Option<u128>
      - name: symbol
        visibility: public
        is_entry: false
        is_view: true
        generic_type_params:
        - constraints: []
        params: []
        return:
        - 0x1::string::String
      - name: transfer
        visibility: public
        is_entry: true
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&signer'
        - address
        - u64
        return: []
      - name: unfreeze_coin_store
        visibility: public
        is_entry: true
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - address
        - '&0x1::coin::FreezeCapability<T0>'
        return: []
      - name: upgrade_supply
        visibility: public
        is_entry: true
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&signer'
        return: []
      - name: value
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&0x1::coin::Coin<T0>'
        return:
        - u64
      - name: withdraw
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params:
        - '&signer'
        - u64
        return:
        - 0x1::coin::Coin<T0>
      - name: zero
        visibility: public
        is_entry: false
        is_view: false
        generic_type_params:
        - constraints: []
        params: []
        return:
        - 0x1::coin::Coin<T0>
      structs:
      - name: AggregatableCoin
        is_native: false
        abilities:
        - store
        generic_type_params:
        - constraints: []
          is_phantom: true
        fields:
        - name: value
          type: 0x1::aggregator::Aggregator
      - name: BurnCapability
        is_native: false
        abilities:
        - copy
        - store
        generic_type_params:
        - constraints: []
          is_phantom: true
        fields:
        - name: dummy_field
          type: bool
      - name: BurnRefReceipt
        is_native: false
        abilities: []
        generic_type_params: []
        fields:
        - name: metadata
          type: 0x1::object::Object<0x1::fungible_asset::Metadata>
      - name: Coin
        is_native: false
        abilities:
        - store
        generic_type_params:
        - constraints: []
          is_phantom: true
        fields:
        - name: value
          type: u64
      - name: CoinConversionMap
        is_native: false
        abilities:
        - key
        generic_type_params: []
        fields:
        - name: coin_to_fungible_asset_map
          type: 0x1::table::Table<0x1::type_info::TypeInfo, 0x1::object::Object<0x1::fungible_asset::Metadata>>
      - name: CoinDeposit
        is_native: false
        abilities:
        - drop
        - store
        generic_type_params: []
        fields:
        - name: coin_type
          type: 0x1::string::String
        - name: account
          type: address
        - name: amount
          type: u64
      - name: CoinEventHandleDeletion
        is_native: false
        abilities:
        - drop
        - store
        generic_type_params: []
        fields:
        - name: event_handle_creation_address
          type: address
        - name: deleted_deposit_event_handle_creation_number
          type: u64
        - name: deleted_withdraw_event_handle_creation_number
          type: u64
      - name: CoinInfo
        is_native: false
        abilities:
        - key
        generic_type_params:
        - constraints: []
          is_phantom: true
        fields:
        - name: name
          type: 0x1::string::String
        - name: symbol
          type: 0x1::string::String
        - name: decimals
          type: u8
        - name: supply
          type: 0x1::option::Option<0x1::optional_aggregator::OptionalAggregator>
      - name: CoinStore
        is_native: false
        abilities:
        - key
        generic_type_params:
        - constraints: []
          is_phantom: true
        fields:
        - name: coin
          type: 0x1::coin::Coin<T0>
        - name: frozen
          type: bool
        - name: deposit_events
          type: 0x1::event::EventHandle<0x1::coin::DepositEvent>
        - name: withdraw_events
          type: 0x1::event::EventHandle<0x1::coin::WithdrawEvent>
      - name: CoinWithdraw
        is_native: false
        abilities:
        - drop
        - store
        generic_type_params: []
        fields:
        - name: coin_type
          type: 0x1::string::String
        - name: account
          type: address
        - name: amount
          type: u64
      - name: Deposit
        is_native: false
        abilities:
        - drop
        - store
        generic_type_params:
        - constraints: []
          is_phantom: true
        fields:
        - name: account
          type: address
        - name: amount
          type: u64
      - name: DepositEvent
        is_native: false
        abilities:
        - drop
        - store
        generic_type_params: []
        fields:
        - name: amount
          type: u64
      - name: FreezeCapability
        is_native: false
        abilities:
        - copy
        - store
        generic_type_params:
        - constraints: []
          is_phantom: true
        fields:
        - name: dummy_field
          type: bool
      - name: MigrationFlag
        is_native: false
        abilities:
        - key
        generic_type_params: []
        fields:
        - name: dummy_field
          type: bool
      - name: MintCapability
        is_native: false
        abilities:
        - copy
        - store
        generic_type_params:
        - constraints: []
          is_phantom: true
        fields:
        - name: dummy_field
          type: bool
      - name: MintRefReceipt
        is_native: false
        abilities: []
        generic_type_params: []
        fields:
        - name: metadata
          type: 0x1::object::Object<0x1::fungible_asset::Metadata>
      - name: PairCreation
        is_native: false
        abilities:
        - drop
        - store
        generic_type_params: []
        fields:
        - name: coin_type
          type: 0x1::type_info::TypeInfo
        - name: fungible_asset_metadata_address
          type: address
      - name: PairedCoinType
        is_native: false
        abilities:
        - key
        generic_type_params: []
        fields:
        - name: type
          type: 0x1::type_info::TypeInfo
      - name: PairedFungibleAssetRefs
        is_native: false
        abilities:
        - key
        generic_type_params: []
        fields:
        - name: mint_ref_opt
          type: 0x1::option::Option<0x1::fungible_asset::MintRef>
        - name: transfer_ref_opt
          type: 0x1::option::Option<0x1::fungible_asset::TransferRef>
        - name: burn_ref_opt
          type: 0x1::option::Option<0x1::fungible_asset::BurnRef>
      - name: SupplyConfig
        is_native: false
        abilities:
        - key
        generic_type_params: []
        fields:
        - name: allow_upgrades
          type: bool
      - name: TransferRefReceipt
        is_native: false
        abilities: []
        generic_type_params: []
        fields:
        - name: metadata
          type: 0x1::object::Object<0x1::fungible_asset::Metadata>
      - name: Withdraw
        is_native: false
        abilities:
        - drop
        - store
        generic_type_params:
        - constraints: []
          is_phantom: true
        fields:
        - name: account
          type: address
        - name: amount
          type: u64
      - name: WithdrawEvent
        is_native: false
        abilities:
        - drop
        - store
        generic_type_params: []
        fields:
        - name: amount
          type: u64
    "###);

    // Account resources by move struct type
    let path = format!(
        "/rpc/v2/accounts/{}/resources/{}",
        sender_account,
        urlencoding::encode(&format!("0x1::coin::CoinStore<{}>", coin_type()))
    );
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;

    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        type: 0x1::coin::CoinStore<0x1::supra_coin::SupraCoin>
        data:
          coin:
            value: '49997100'
          deposit_events:
            counter: '1'
            guid:
              id:
                addr: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                creation_num: '2'
          frozen: false
          withdraw_events:
            counter: '1'
            guid:
              id:
                addr: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
                creation_num: '3'
        "###
        );
    });
}

#[ntex::test]
async fn test_account_automated_txn_api_responses() {
    let _ = supra_logger::init_default_logger(LevelFilter::INFO);
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

    let block_auto_txn_hashes = get_block_transactions_v3(Some("auto"), bhash, &pipeline).await;
    assert!(!block_auto_txn_hashes.is_empty());
    let auto_txn_hash = block_auto_txn_hashes[0];

    // Get automated transaction info
    let v3_auto_txn = query_existing_tx_details_by_version::<Transaction>(
        &pipeline,
        auto_txn_hash,
        Some("auto"),
        3,
    )
    .await
    .expect("Valid txn info");
    let v3_auto_txn_info = v3_auto_txn.into_transaction_info().unwrap();
    let sender_account = v3_auto_txn_info.header().sender();
    let AccountAddress::Move(ca) = sender_account else {
        panic!("Expected move account address got {:?}", &sender_account);
    };
    let account_address_param = String::from(ca);
    let count = 5_usize;

    // Get automated transaction statements for the sender account
    let path =
        format!("/rpc/v3/accounts/{account_address_param}/automated_transactions?count={count}");
    let result_bytes = block::get(&path, &pipeline).await;
    let a_statement = serde_json::from_slice::<AccountStatementV3>(result_bytes.as_ref()).unwrap();
    assert_eq!(a_statement.0.len(), count);
    a_statement.0.iter().for_each(|item| {
        let Transaction::Automated(info_v2) = item else {
            panic!("Expected Transaction::Automated, got {item:?}")
        };
        assert!(matches!(
            info_v2.authenticator(),
            TransactionAuthenticator::Automation(_)
        ));
    });

    // Get automated transaction statements for the sender account
    let path = format!("/rpc/v3/accounts/{account_address_param}/coin_transactions?count={count}");
    let result_bytes = block::get(&path, &pipeline).await;
    let cstatement = serde_json::from_slice::<Vec<Transaction>>(result_bytes.as_ref()).unwrap();
    assert_eq!(cstatement.len(), count);
    cstatement.iter().for_each(|item| {
        let Transaction::Automated(info_v2) = item else {
            panic!("Expected Transaction::Automated, got {item:?}")
        };
        assert!(matches!(
            info_v2.authenticator(),
            TransactionAuthenticator::Automation(_)
        ));
    });

    // No user transaction statement for the account
    let path = format!("/rpc/v2/accounts/{account_address_param}/transactions?count={count}");
    let result_bytes = block::get(&path, &pipeline).await;
    let ua_statement = serde_json::from_slice::<AccountStatementV2>(result_bytes.as_ref()).unwrap();
    assert!(ua_statement.record.is_empty());
}
