use crate::rest::api;
use ntex::web;
use ntex::web::{DefaultError, WebServiceFactory};

/// Wallet route for V1.
pub fn wallet_route_v1() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v1/wallet/faucet")
        .service(api::v1::wallet::faucet)
        .service(api::v1::wallet::faucet_transaction_by_hash)
}

/// Wallet route for V2.
pub fn wallet_route_v2() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v2/wallet/faucet").service(api::v2::wallet::faucet_transaction_by_hash)
}

/// Wallet route for V3.
/// Simply will redirect v3 endpoints to v2/v1 active(non-deprecated) handlers
// Cursor returned via header for v3-apis: faucet_transaction_by_hash, faucet
pub fn wallet_route_v3() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v3/wallet/faucet")
        .service(api::v2::wallet::faucet_transaction_by_hash)
        .service(api::v1::wallet::faucet)
}
