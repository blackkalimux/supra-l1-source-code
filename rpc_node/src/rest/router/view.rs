use crate::rest::api;
use ntex::web;
use ntex::web::{DefaultError, WebServiceFactory};

/// View API V1 routes
pub(crate) fn view_function_route_v1() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v1/view").service(api::v1::view::view_function)
}

/// View API V2 routes
pub(crate) fn view_function_route_v2() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v2/view").service(api::v2::view::view_function)
}

/// View API V3 routes
/// Simply will redirect v3 endpoints to v2 APIs handlers.
// Cursor returned via header for v3-apis: view_function
pub(crate) fn view_function_route_v3() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v3/view").service(api::v2::view::view_function)
}
