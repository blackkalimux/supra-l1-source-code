use crate::rest::common::{
    api_response_snapshot_filters, call_endpoint, call_endpoints_with_headers,
    setup_test_data_fixture, setup_test_data_fixture_with_ttl, MAGIC_NUMBER,
};
use crate::rest::helpers::setup_executor_with_web_service;
use ntex::web;
use ntex::web::test;
use rpc_node::rest::router::default_route;
use types::api::X_SUPRA_CURSOR;

#[ntex::test]
async fn test_events_api() {
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

    let pipeline = test::init_service(rpc_app).await;

    // Events by type
    let event_type = urlencoding::encode("0x1::coin::CoinDeposit");
    let start = MAGIC_NUMBER;
    let end = MAGIC_NUMBER + 1;
    let path = format!("/rpc/v1/events/{event_type}?start={start}&end={end}",);
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;

    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        data:
        - guid:
            creation_number: '0'
            account_address: '0x0'
          sequence_number: '0'
          type: 0x1::coin::CoinDeposit
          data:
            account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
            amount: '50000000'
            coin_type: 0x1::supra_coin::SupraCoin
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
            creation_number: '0'
            account_address: '0x0'
          sequence_number: '0'
          type: 0x1::coin::CoinDeposit
          data:
            account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
            amount: '2000'
            coin_type: 0x1::supra_coin::SupraCoin
        "###
        );
    });
}

#[ntex::test]
async fn test_events_api_v3() {
    let (executor_with_resources, web_service) =
        setup_executor_with_web_service(false, &[], &Default::default()).await;
    let test_executor = executor_with_resources.executor;
    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    // Database fixture setup in order for test API.
    setup_test_data_fixture_with_ttl(
        &test_executor.move_executor,
        &test_executor.move_store,
        executor_with_resources.rpc_storage.archive(),
        None,
    );

    let pipeline = test::init_service(rpc_app).await;

    // Events by type
    let event_type = urlencoding::encode("0x1::coin::CoinDeposit");
    let start = MAGIC_NUMBER;
    let end = MAGIC_NUMBER + 1;
    let path = format!("/rpc/v3/events/{event_type}?start_height={start}&end_height={end}",);
    let resp_body = call_endpoint(&pipeline, &path, vec![]).await;

    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        data:
        - event:
            guid:
              creation_number: '0'
              account_address: '0x0'
            sequence_number: '0'
            type: 0x1::coin::CoinDeposit
            data:
              account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
              amount: '50000000'
              coin_type: 0x1::supra_coin::SupraCoin
          block_height: 24
          transaction_hash: ***
        - event:
            guid:
              creation_number: '0'
              account_address: '0x0'
            sequence_number: '0'
            type: 0x1::coin::CoinDeposit
            data:
              account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              amount: '1000'
              coin_type: 0x1::supra_coin::SupraCoin
          block_height: 24
          transaction_hash: ***
        - event:
            guid:
              creation_number: '0'
              account_address: '0x0'
            sequence_number: '0'
            type: 0x1::coin::CoinDeposit
            data:
              account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              amount: '2000'
              coin_type: 0x1::supra_coin::SupraCoin
          block_height: 24
          transaction_hash: ***
        "###
        );
    });
}

#[ntex::test]
async fn test_events_api_v3_with_cursor() {
    let (executor_with_resources, web_service) =
        setup_executor_with_web_service(false, &[], &Default::default()).await;
    let test_executor = executor_with_resources.executor;
    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    // Database fixture setup in order for test API.
    setup_test_data_fixture_with_ttl(
        &test_executor.move_executor,
        &test_executor.move_store,
        executor_with_resources.rpc_storage.archive(),
        None,
    );

    let pipeline = test::init_service(rpc_app).await;

    // Events by type
    let event_type = urlencoding::encode("0x1::coin::CoinDeposit");
    let start = MAGIC_NUMBER;
    let end = MAGIC_NUMBER + 1;
    let path =
        format!("/rpc/v3/events/{event_type}?start_height={start}&end_height={end}&limit=2",);
    let (resp_body, resp_headers) = call_endpoints_with_headers(&pipeline, &path, vec![]).await;

    // First page should return 2 events
    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        data:
        - event:
            guid:
              creation_number: '0'
              account_address: '0x0'
            sequence_number: '0'
            type: 0x1::coin::CoinDeposit
            data:
              account: 0x85e8ce0c84ddc0d751397c36f126be283295fe59fa60b47fe54f8c4ef167560f
              amount: '50000000'
              coin_type: 0x1::supra_coin::SupraCoin
          block_height: 24
          transaction_hash: ***
        - event:
            guid:
              creation_number: '0'
              account_address: '0x0'
            sequence_number: '0'
            type: 0x1::coin::CoinDeposit
            data:
              account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              amount: '1000'
              coin_type: 0x1::supra_coin::SupraCoin
          block_height: 24
          transaction_hash: ***
        "###
        );
    });

    // Cursor should be set to the second event
    let cursor = resp_headers.get(X_SUPRA_CURSOR).unwrap();
    insta::assert_snapshot!(
        &cursor,
        @"2357d5d16bbaa32707274baeba550b22b8c04fed970ab3eb668e6a13836412f2000000000000001800000000000000030000000000000000"
    );

    // Second call, with cursor
    let path = format!(
        "/rpc/v3/events/{event_type}?start={cursor}&start_height={start}&end_height={end}&limit=2",
    );
    let (resp_body, resp_headers) = call_endpoints_with_headers(&pipeline, &path, vec![]).await;

    // Second page should return only the last event
    insta::with_settings!({filters => api_response_snapshot_filters()}, {
        insta::assert_snapshot!(
            &resp_body,
            @r###"
        data:
        - event:
            guid:
              creation_number: '0'
              account_address: '0x0'
            sequence_number: '0'
            type: 0x1::coin::CoinDeposit
            data:
              account: 0x7da2973a2ac17f07577f8466253f153971bcc4d8e2fdff8dfa2661f02b20f955
              amount: '2000'
              coin_type: 0x1::supra_coin::SupraCoin
          block_height: 24
          transaction_hash: ***
        "###
        );
    });

    // Cursor should change
    let cursor = resp_headers.get(X_SUPRA_CURSOR).unwrap();
    insta::assert_snapshot!(
        &cursor,
        @"2357d5d16bbaa32707274baeba550b22b8c04fed970ab3eb668e6a13836412f2000000000000001800000000000000040000000000000002"
    );
}
