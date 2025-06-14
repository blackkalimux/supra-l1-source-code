use crate::rest::api;
use ntex::web;
use ntex::web::{DefaultError, WebServiceFactory};

/// Account API V1 routes
pub(crate) fn account_route_v1() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v1/accounts")
        .service(api::v1::accounts::get_account_v1)
        .service(api::v1::accounts::get_account_resources_v1)
        .service(api::v1::accounts::get_account_modules_v1)
        .service(api::v1::accounts::get_account_resource_v1)
        .service(api::v1::accounts::get_account_module_v1)
        .service(api::v1::accounts::get_account_transactions_v1)
        .service(api::v1::accounts::coin_transactions_v1)
}

/// Account API V2 routes
pub(crate) fn account_route_v2() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v2/accounts")
        .service(api::v2::accounts::get_account_v2)
        .service(api::v2::accounts::get_account_resource_v2)
        .service(api::v2::accounts::get_account_resources_v2)
        .service(api::v2::accounts::get_account_module_v2)
        .service(api::v2::accounts::get_account_modules_v2)
        .service(api::v2::accounts::get_account_transactions_v2)
        .service(api::v2::accounts::coin_transactions_v2)
}

/// Account API V3 routes
pub(crate) fn account_route_v3() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v3/accounts")
        .service(api::v3::accounts::get_account_transactions_v3)
        .service(api::v3::accounts::get_account_automated_transactions_v3)
        .service(api::v3::accounts::coin_transactions_v3)
        .service(api::v3::accounts::get_account_resources_v3)
        .service(api::v3::accounts::get_account_modules_v3)
        // Add also v2 active services to v3, v3 endpoints will be simply redirected to v2 APIs handlers
        // Cursor returned via header for v3-apis: get_account_v2, get_account_resource_v2, get_account_module_v2
        .service(api::v2::accounts::get_account_v2)
        .service(api::v2::accounts::get_account_resource_v2)
        .service(api::v2::accounts::get_account_module_v2)
}
