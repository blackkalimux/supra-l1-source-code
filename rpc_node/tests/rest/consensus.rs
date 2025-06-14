use crate::rest::common::{batch_fixture, MAGIC_NUMBER};
use crate::rest::helpers::setup_executor_with_web_service;
use committees::{GenesisStateProvider, TBlockHeader, TSmrBlock};
use configurations::rpc::kyc_token::KycToken;
use genesis::Genesis;
use lifecycle::EpochId;
use moonshot_storage_ifc::MoonshotStorageWriter;
use ntex::http::header;
use ntex::web::test;
use ntex::{http, web};
use rpc_node::rest::router::default_route;
use smr_timestamp::SmrTimestamp;
use socrypto::{Digest, Identity};
use soserde::SmrDeserialize;
use supra_logger::LevelFilter;
use test_utils_2::block::commit_certified_chain;
use tracing::debug;
use types::api::v1::ConsensusBlock;
use types::settings::constants::LOCALNET_CHAIN_ID;

#[ntex::test]
async fn test_consensus_api() {
    let _ = supra_logger::init_default_logger(LevelFilter::OFF);

    const BLOCK_COUNT: usize = 1;

    let token = "1".to_string();
    let tokens = vec![KycToken::new("localhost".into(), token.as_bytes().digest())];

    let (executor_with_resources, web_service) =
        setup_executor_with_web_service(false, &tokens, &Default::default()).await;
    let genesis_state_provider = &executor_with_resources.executor.genesis_state_provider;
    let signers = genesis_state_provider.validator_consensus_keys();

    // `commit_certified_chain` internally generates batches but drops and never returns them,
    // consequently, dummy batches must be generated.
    let block = commit_certified_chain(
        genesis_state_provider.genesis_block_initializer(),
        BLOCK_COUNT,
        signers,
    )
    .pop()
    .unwrap();

    debug!(
        "Height {}, Generated block {block:#?}\ncontains {} batches",
        block.height(),
        block.payload().items().len()
    );

    let store = executor_with_resources.executor.chain_store;

    store.put_certified_block(&block).unwrap();
    debug!("Block stored");

    for hash in block
        .payload()
        .items()
        .iter()
        .map(|item| item.batch().hash())
    {
        let smr_batch = batch_fixture(
            Identity::default(),
            EpochId::genesis(LOCALNET_CHAIN_ID),
            SmrTimestamp::new_from(MAGIC_NUMBER as u64),
        );

        store.batch.write(hash, smr_batch).unwrap();
    }
    debug!("All fake batches stored");

    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    let pipeline = test::init_service(rpc_app).await;

    let uri = format!(
        "/rpc/v1/consensus/block/height/{}?with_batches=false",
        block.height()
    );

    debug!(
        "[1] Submitting a consensus block request (no batches) without an auth token\nto {uri} "
    );
    let req = test::TestRequest::get().uri(uri.as_str()).to_request();
    let resp = pipeline.call(req).await.unwrap();
    debug!("[1] Response {resp:?}");
    assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);

    let body = test::read_body(resp).await;
    assert_eq!(
        body,
        ntex::util::Bytes::from_static(b"Authorization is absent")
    );

    debug!(
        "[2] Submitting a consensus block request (no batches) with a wrong auth token\nto {uri} "
    );
    let req = test::TestRequest::get()
        .header(header::AUTHORIZATION, "2")
        .uri(uri.as_str())
        .to_request();
    let resp = pipeline.call(req).await.unwrap();
    debug!("[2] Response {resp:?}");
    assert_eq!(resp.status(), http::StatusCode::NOT_ACCEPTABLE);

    let body = test::read_body(resp).await;
    assert_eq!(
        body,
        ntex::util::Bytes::from_static(b"Authorization data is incorrect")
    );

    debug!(
        "[3] Submitting a consensus block request (no batches) with a correct auth token\nto {uri}"
    );
    let req = test::TestRequest::get()
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .uri(uri.as_str())
        .to_request();
    let resp = pipeline.call(req).await.unwrap();
    let resp = resp.response();
    debug!("[3] Response {resp:?}");
    assert_eq!(resp.status(), http::StatusCode::OK);

    // V2
    let uri = format!(
        "/rpc/v2/consensus/block/height/{}?with_batches=true",
        block.height()
    );

    debug!("[4] Submitting a consensus block request (with batches) with a correct auth token\nto {uri}");
    let req = test::TestRequest::get()
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .uri(&uri)
        .to_request();

    let response = test::read_response(&pipeline, req).await;
    let consensus_block =
        ConsensusBlock::try_from_bytes(response.as_ref()).expect("ConsensusBlock must deserialize");

    assert_eq!(consensus_block.block().hash(), block.hash());

    // V1
    let uri = format!(
        "/rpc/v1/consensus/block/height/{}?with_batches=true",
        block.height()
    );

    debug!("[4] Submitting a consensus block request (with batches) with a correct auth token\nto {uri}");
    let req = test::TestRequest::get()
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .uri(&uri)
        .to_request();

    let response: Option<ConsensusBlock> = test::read_response_json(&pipeline, req).await;
    assert!(
        matches!(response, Some(consensus_block) if consensus_block.block().hash() == block.hash())
    );
}
