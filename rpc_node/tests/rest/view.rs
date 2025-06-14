use crate::rest::helpers::setup_executor_with_web_service;
use aptos_api_types::MoveType;
use ntex::web::test;
use ntex::{http, web};
use rpc_node::rest::router::default_route;
use serde::Serialize;

#[ntex::test]
async fn test_view_api() {
    let (_, web_service) = setup_executor_with_web_service(false, &[], &Default::default()).await;
    let rpc_app = web::App::new()
        .service(web_service)
        .default_service(web::route().to(default_route));

    let pipeline = test::init_service(rpc_app).await;

    // view timestamp

    #[derive(Serialize)]
    struct ViewRequest {
        function: String,
        type_arguments: Vec<MoveType>,
        arguments: Vec<serde_json::Value>,
    }

    let view = ViewRequest {
        function: "0x1::timestamp::now_microseconds".to_string(),
        type_arguments: vec![],
        arguments: vec![],
    };
    let req = test::TestRequest::post()
        .uri("/rpc/v1/view")
        .set_json(&view)
        .to_request();
    let resp = pipeline.call(req).await.unwrap();
    let resp = resp.response();
    assert_eq!(resp.status(), http::StatusCode::OK);
}
