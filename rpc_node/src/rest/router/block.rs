use crate::rest::api;
use ntex::web;
use ntex::web::{DefaultError, WebServiceFactory};

/// Block API V1 routes
pub(crate) fn block_route_v1() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v1/block")
        .service(api::v1::block::latest_block)
        .service(api::v1::block::block_header_info)
        .service(api::v1::block::txs_by_block)
        .service(api::v1::block::block_by_height)
}

/// Block API V2 routes
pub(crate) fn block_route_v2() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v2/block")
        .service(api::v2::block::latest_block)
        .service(api::v2::block::block_header_info)
        .service(api::v2::block::block_by_height)
}

/// Block API V3 routes
pub(crate) fn block_route_v3() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v3/block")
        .service(api::v3::block::block_by_height)
        .service(api::v3::block::txs_by_block)
        .service(api::v3::block::latest_block)
        .service(api::v3::block::block_info_by_hash)
}
