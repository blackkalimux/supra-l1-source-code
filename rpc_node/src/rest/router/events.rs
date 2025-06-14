use crate::rest::api;
use ntex::web;
use ntex::web::{DefaultError, WebServiceFactory};

/// Events API V1 routes
pub(crate) fn events_route_v1() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v1/events").service(api::v1::events::events_by_type)
}

/// Events API V3 routes
pub(crate) fn events_route_v3() -> impl WebServiceFactory<DefaultError> {
    web::scope("/v3/events").service(api::v3::events::events_by_type)
}
