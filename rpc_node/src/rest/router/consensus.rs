use crate::rest::api;
use crate::Authorization;
use configurations::rpc::kyc_token::KycToken;
use ntex::web;
use ntex::web::{DefaultError, WebServiceFactory};

/// Consensus API V1 routes
pub(crate) fn consensus_route_v1(
    consensus_authz_tokens: &[KycToken],
) -> impl WebServiceFactory<DefaultError> {
    web::scope("/v1/consensus")
        .service(api::v1::consensus::consensus_block)
        .wrap(Authorization::new(consensus_authz_tokens))
}

/// Consensus API V2 routes
pub(crate) fn consensus_route_v2(
    consensus_authz_tokens: &[KycToken],
) -> impl WebServiceFactory<DefaultError> {
    web::scope("/v2/consensus")
        .service(api::v2::consensus::consensus_block)
        .service(api::v2::consensus::latest_consensus_block)
        .service(api::v2::consensus::committee_authorization)
        .wrap(Authorization::new(consensus_authz_tokens))
}

/// Consensus API V3 routes
/// Simply will redirect v3 endpoints to v2 APIs handlers.
// Cursor returned via header for v3-apis: consensus_block, latest_consensus_block, committee_authorization
pub(crate) fn consensus_route_v3(
    consensus_authz_tokens: &[KycToken],
) -> impl WebServiceFactory<DefaultError> {
    web::scope("/v3/consensus")
        .service(api::v2::consensus::consensus_block)
        .service(api::v2::consensus::latest_consensus_block)
        .service(api::v2::consensus::committee_authorization)
        .wrap(Authorization::new(consensus_authz_tokens))
}
