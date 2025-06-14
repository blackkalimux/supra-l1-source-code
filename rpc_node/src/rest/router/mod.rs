use crate::rest::ApiServerState;
use configurations::rpc::kyc_token::KycToken;
use ntex::web;
use ntex::web::{DefaultError, WebServiceFactory};

pub mod accounts;
pub mod block;
pub mod consensus;
pub mod events;
pub mod resources;
pub mod transactions;
pub mod view;
pub mod wallet;

pub async fn default_route() -> web::HttpResponse {
    web::HttpResponse::NotFound().finish()
}

pub fn rest_root_route(
    state: ApiServerState,
    consensus_authz_tokens: &[KycToken],
) -> impl WebServiceFactory<DefaultError> {
    let is_faucet_enabled = state.is_faucet_enabled();

    let mut route = web::scope("/rpc").state(state);

    if is_faucet_enabled {
        route = route.service(wallet::wallet_route_v1());
        route = route.service(wallet::wallet_route_v2());
        route = route.service(wallet::wallet_route_v3());
    }

    route = route
        // V1
        .service(accounts::account_route_v1())
        .service(transactions::transaction_route_v1())
        .service(block::block_route_v1())
        .service(view::view_function_route_v1())
        .service(events::events_route_v1())
        .service(resources::tables_route_v1())
        .service(consensus::consensus_route_v1(consensus_authz_tokens))
        // v2
        .service(accounts::account_route_v2())
        .service(transactions::transaction_route_v2())
        .service(block::block_route_v2())
        .service(view::view_function_route_v2())
        .service(resources::tables_route_v2())
        .service(consensus::consensus_route_v2(consensus_authz_tokens))
        // v3
        .service(accounts::account_route_v3())
        .service(block::block_route_v3())
        .service(transactions::transaction_route_v3())
        .service(events::events_route_v3())
        .service(view::view_function_route_v3())
        .service(resources::tables_route_v3())
        .service(consensus::consensus_route_v3(consensus_authz_tokens));

    route
}
