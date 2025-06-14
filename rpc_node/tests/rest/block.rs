use crate::rest::helpers::setup_executor_with_web_service;
use committees::Height;
use futures::StreamExt;
use ntex::http::Request;
use ntex::util::{Bytes, BytesMut};
use ntex::web::test::{self, TestRequest};
use ntex::web::{self, WebResponse};
use ntex::{Pipeline, Service};
use rpc_node::rest::router::default_route;
use socrypto::Hash;
use std::fmt::Debug;
use supra_logger::LevelFilter;
use tracing::info;
use types::api::v2::{Block, BlockHeaderInfo};
use types::api::v3::block_info::ExecutedBlockInfo;
use types::api::v3::BlockV3;

pub(crate) async fn get<E, S>(path: &str, pipeline: &Pipeline<S>) -> BytesMut
where
    E: Debug,
    S: Service<Request, Response = WebResponse, Error = E>,
{
    let request = TestRequest::get().uri(path).to_request();
    let mut response = pipeline.call(request).await.unwrap();
    info!("{:?}", response.response().body());
    assert!(response.status().is_success());
    let body = response.take_body().into_body::<Bytes>();
    body.fold(BytesMut::new(), |mut acc, chunk| async move {
        acc.extend_from_slice(&chunk.unwrap());
        acc
    })
    .await
}

pub(crate) async fn get_block_transactions_v3<E, S>(
    txn_type: Option<&str>,
    bhash: Hash,
    pipeline: &Pipeline<S>,
) -> Vec<Hash>
where
    E: Debug,
    S: Service<Request, Response = WebResponse, Error = E>,
{
    let txn_type_param = match txn_type {
        None => String::new(),
        Some(value) => format!("?type={value}"),
    };
    let latest_block_txn_hashes_bytes = get(
        &format!("/rpc/v3/block/{bhash}/transactions{txn_type_param}"),
        pipeline,
    )
    .await;
    serde_json::from_slice::<Vec<Hash>>(latest_block_txn_hashes_bytes.as_ref()).unwrap()
}

#[ntex::test]
async fn test_block_api() {
    let _ = supra_logger::init_default_logger(LevelFilter::OFF);
    let (_, web_service) = setup_executor_with_web_service(true, &[], &Default::default()).await;
    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    let pipeline = test::init_service(rpc_app).await;

    // Get the latest block header.
    let latest_header_bytes = get("/rpc/v1/block", &pipeline).await;
    let resp_block_header_info =
        serde_json::from_slice::<BlockHeaderInfo>(latest_header_bytes.as_ref()).unwrap();
    let height = resp_block_header_info.height;
    check_block_by_height_api(height, &pipeline).await;

    let blk_hash = resp_block_header_info.hash;
    check_block_txn_by_hash_api(blk_hash, &pipeline).await;

    // Get the latest block info via v3.
    let latest_header_bytes = get("/rpc/v3/block", &pipeline).await;
    let latest_block_info =
        serde_json::from_slice::<ExecutedBlockInfo>(latest_header_bytes.as_ref()).unwrap();
    assert_eq!(latest_block_info.header(), &resp_block_header_info);
    assert!(latest_block_info.execution_statistics().is_some());
}

async fn check_block_by_height_api<E, S>(height: Height, pipeline: &Pipeline<S>)
where
    E: Debug,
    S: Service<Request, Response = WebResponse, Error = E>,
{
    println!("Check block by height api");

    // Get the related API [Block] by height without transactions.
    let latest_block_without_transactions_bytes = get(
        &format!("/rpc/v1/block/height/{height}?with_finalized_transactions=false"),
        pipeline,
    )
    .await;
    let response_block =
        serde_json::from_slice::<Block>(latest_block_without_transactions_bytes.as_ref()).unwrap();
    assert!(response_block.transactions().is_none());
    assert_eq!(
        response_block.header().height,
        height,
        "Block requested by height must have the requested height"
    );

    // Get the related API [Block] by height with transactions.
    let latest_block_bytes = get(
        &format!("/rpc/v1/block/height/{height}?with_finalized_transactions=true"),
        pipeline,
    )
    .await;
    let response_block_v1 = serde_json::from_slice::<Block>(latest_block_bytes.as_ref()).unwrap();
    assert!(response_block_v1.transactions().is_some());
    let block_txn_count = response_block_v1.transactions().as_ref().unwrap().len();
    assert!(block_txn_count > 0);
    assert_eq!(
        response_block_v1.header().height,
        height,
        "Block requested by height must have the requested height"
    );

    // Get the related API [BlockV3] by height with transactions using v3 API.
    let latest_block_bytes = get(
        &format!("/rpc/v3/block/height/{height}?with_finalized_transactions=true"),
        pipeline,
    )
    .await;
    let response_block = serde_json::from_slice::<BlockV3>(latest_block_bytes.as_ref()).unwrap();
    assert!(response_block.executed_block_stats().is_some());
    assert!(response_block.transactions().is_some());
    let block_v3_transactions_count = response_block.transactions().as_ref().unwrap().len();
    assert!(block_v3_transactions_count > 0);
    assert_eq!(
        response_block.header().height,
        height,
        "Block requested by height must have the requested height"
    );

    // Get the related API [BlockV3] by height with automated transactions only using v3 API.
    let latest_block_bytes = get(
        &format!("/rpc/v3/block/height/{height}?with_finalized_transactions=true&type=auto"),
        pipeline,
    )
    .await;
    let response_block = serde_json::from_slice::<BlockV3>(latest_block_bytes.as_ref()).unwrap();
    assert!(response_block.transactions().is_some());
    assert!(response_block.executed_block_stats().is_some());
    let automated_txn_count = response_block.transactions().as_ref().unwrap().len();
    assert!(automated_txn_count > 0);
    assert_eq!(
        response_block.header().height,
        height,
        "Block requested by height must have the requested height"
    );
    // Get the related API [BlockV3] by height with user transactions only using v3 API.
    let latest_block_bytes = get(
        &format!("/rpc/v3/block/height/{height}?with_finalized_transactions=true&type=user"),
        pipeline,
    )
    .await;
    let response_block = serde_json::from_slice::<BlockV3>(latest_block_bytes.as_ref()).unwrap();
    assert!(response_block.transactions().is_some());
    assert!(response_block.executed_block_stats().is_some());
    let user_txn_count = response_block.transactions().as_ref().unwrap().len();
    assert!(user_txn_count > 0);
    assert_eq!(
        response_block.header().height,
        height,
        "Block requested by height must have the requested height"
    );
    assert_eq!(user_txn_count, block_txn_count, "{:?}", response_block);
    // Get the related API [BlockV3] by height with block metadata txn only using v3 API.
    let latest_block_bytes = get(
        &format!("/rpc/v3/block/height/{height}?with_finalized_transactions=true&type=meta"),
        pipeline,
    )
    .await;
    let response_block = serde_json::from_slice::<BlockV3>(latest_block_bytes.as_ref()).unwrap();
    assert!(response_block.transactions().is_some());
    assert!(response_block.executed_block_stats().is_some());
    let block_metadata_count = response_block.transactions().as_ref().unwrap().len();
    assert_eq!(block_metadata_count, 1);
    assert_eq!(
        response_block.header().height,
        height,
        "Block requested by height must have the requested height"
    );
    assert_eq!(
        user_txn_count + automated_txn_count + block_metadata_count,
        block_v3_transactions_count
    );
}

async fn check_block_txn_by_hash_api<E, S>(bhash: Hash, pipeline: &Pipeline<S>)
where
    E: Debug,
    S: Service<Request, Response = WebResponse, Error = E>,
{
    println!("Check block transactions by HASH api");

    // Get transaction hash of the block by block hash via v1 API
    let latest_block_txn_hashes_bytes =
        get(&format!("/rpc/v1/block/{bhash}/transactions"), pipeline).await;
    let block_user_txn_hashes_v1 =
        serde_json::from_slice::<Vec<Hash>>(latest_block_txn_hashes_bytes.as_ref()).unwrap();
    assert!(!block_user_txn_hashes_v1.is_empty());

    // Get transaction hash of the block by block hash via v3 API
    let block_all_txn_hashes = get_block_transactions_v3(None, bhash, pipeline).await;
    assert!(!block_all_txn_hashes.is_empty());

    // Get user transaction hash of the block by block hash via v3 API
    let block_user_txn_hashes = get_block_transactions_v3(Some("user"), bhash, pipeline).await;
    assert!(!block_user_txn_hashes.is_empty());

    // Get automated transaction hash of the block by block hash via v3 API
    let block_auto_txn_hashes = get_block_transactions_v3(Some("auto"), bhash, pipeline).await;
    assert!(!block_auto_txn_hashes.is_empty());
    // Get automated transaction hash of the block by block hash via v3 API
    let block_meta_txn_hashes = get_block_transactions_v3(Some("meta"), bhash, pipeline).await;
    assert_eq!(block_meta_txn_hashes.len(), 1);
    let metadata_hash = block_meta_txn_hashes[0];

    assert_eq!(block_user_txn_hashes_v1.len(), block_user_txn_hashes.len());
    assert_eq!(
        block_all_txn_hashes.len(),
        block_user_txn_hashes.len() + block_auto_txn_hashes.len() + block_meta_txn_hashes.len()
    );

    // Check that block header info can be queried only by block hash and not by transaction hash
    let block_header_bytes = get(&format!("/rpc/v2/block/{bhash}"), pipeline).await;
    assert!(serde_json::from_slice::<BlockHeaderInfo>(block_header_bytes.as_ref()).is_ok());

    let request = TestRequest::get()
        .uri(&format!("/rpc/v2/block/{metadata_hash}"))
        .to_request();
    let response = pipeline.call(request).await.unwrap();
    assert!(!response.status().is_success());

    let executed_block_info_bytes = get(&format!("/rpc/v3/block/{bhash}"), pipeline).await;
    assert!(
        serde_json::from_slice::<ExecutedBlockInfo>(executed_block_info_bytes.as_ref()).is_ok()
    );
}
