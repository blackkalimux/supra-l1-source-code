use crate::rest::api;
use ntex::web;
use ntex::web::{DefaultError, WebServiceFactory};

/// Table API V1 routes
pub(crate) fn tables_route_v1() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v1/tables").service(api::v1::resources::table_item_by_key)
}

/// Table API V2 routes
pub(crate) fn tables_route_v2() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v2/tables").service(api::v2::resources::table_item_by_key)
}

/// Table API V3 routes
/// Simply will redirect v3 endpoint calls to v2
// Cursor returned via header for v3-apis: table_item_by_key
pub(crate) fn tables_route_v3() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v3/tables").service(api::v2::resources::table_item_by_key)
}
