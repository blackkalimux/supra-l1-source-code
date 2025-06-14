use crate::rest::api;
use ntex::web;
use ntex::web::{DefaultError, WebServiceFactory};

/// TODO: Finish tuning this constant.
///
/// Allow [submit_txn] payloads of up to 5MB to continue processing. This is a loose limit as the MoveVM will
/// reject regular transactions larger than 64KiB and authorized governance transactions larger than 1MiB.
const MAX_PAYLOAD_SIZE: usize = 5_000_000;

/// Transactions API V1 routes
pub(crate) fn transaction_route_v1() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v1/transactions")
        .state(web::types::JsonConfig::default().limit(MAX_PAYLOAD_SIZE))
        .service(api::v1::transactions::chain_id)
        .service(api::v1::transactions::transaction_parameters)
        .service(api::v1::transactions::estimate_gas_price)
        .service(api::v1::transactions::submit_txn)
        .service(api::v1::transactions::simulate_txn)
        // static apis should be registered first and the dynamic ones,
        .service(api::v1::transactions::transaction_by_hash)
}

/// Transactions API V2 routes
pub(crate) fn transaction_route_v2() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v2/transactions")
        .state(web::types::JsonConfig::default().limit(MAX_PAYLOAD_SIZE))
        .service(api::v2::transactions::estimate_gas_price)
        .service(api::v2::transactions::simulate_txn)
        // Simply redirects v2 endpoints to v1 API handlers
        .service(api::v1::transactions::chain_id)
        .service(api::v1::transactions::submit_txn)
        // static apis should be registered first and the dynamic ones,
        .service(api::v2::transactions::transaction_by_hash)
}

/// Transactions API V3 routes
/// Duplicates V1 and V2 active(non-deprecated) endpoints in V3 as well
pub(crate) fn transaction_route_v3() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v3/transactions")
        .state(web::types::JsonConfig::default().limit(MAX_PAYLOAD_SIZE))
        // Simply redirects v3 endpoints to v2 API handlers
        // Cursor returned via header for v3-apis: estimate_gas_price, simulate_txn
        .service(api::v2::transactions::estimate_gas_price)
        .service(api::v2::transactions::simulate_txn)
        // Simply redirects v3 endpoints to v1 API handlers
        // Cursor returned via header for v3-apis: chain_id, transaction_parameters, submit_txn
        .service(api::v1::transactions::chain_id)
        .service(api::v1::transactions::transaction_parameters)
        .service(api::v1::transactions::submit_txn)
        // v3 new version
        // static apis should be registered first and the dynamic ones,
        // otherwise ntex starts to parse `/transactions/chain_id` assuming chain_id is a hex string,
        // then it fails as h is not in hex literals
        .service(api::v3::transactions::transaction_by_hash)
}
