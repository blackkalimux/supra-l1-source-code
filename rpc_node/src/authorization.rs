use configurations::rpc::kyc_token::{AuthorizedSource, KycToken};
use ntex::http::StatusCode;
use ntex::service::{Middleware, Service, ServiceCtx};
use ntex::web::{HttpResponse, WebRequest, WebResponse};
use socrypto::Digest;
use std::rc::Rc;
use tracing::{trace, warn};

/// [Authorization] manages access to RPC synchronization endpoints and contains a list of [KycToken] entries.
///
/// The authorization process works as follows:
/// 1. The client sends a request with the authorization header set to `"Bearer {token}"`, where `{token}` is the plain text token.
/// 2. The authorization middleware processes the request by:
///     - Extracting the token from the authorization header,
///     - Hashing the extracted token,
///     - Comparing the hash of the extracted token with the hashes of stored [KycToken] entries,
///     - Verifying that the source of the submitted token matches the source stored in the corresponding [KycToken], if a match is found.
pub struct Authorization {
    kyc_tokens: Rc<Vec<KycToken>>,
}

impl Authorization {
    pub fn new(kyc_tokens: &[KycToken]) -> Self {
        Self {
            kyc_tokens: Rc::new(kyc_tokens.to_vec()),
        }
    }
}

impl<S> Middleware<S> for Authorization {
    type Service = AuthorizationMiddleware<S>;

    fn create(&self, service: S) -> Self::Service {
        AuthorizationMiddleware {
            kyc_tokens: self.kyc_tokens.clone(),
            service,
        }
    }
}

#[derive(Debug)]
pub struct AuthorizationMiddleware<S> {
    kyc_tokens: Rc<Vec<KycToken>>,
    service: S,
}

impl<S, E> Service<WebRequest<E>> for AuthorizationMiddleware<S>
where
    S: Service<WebRequest<E>, Response = WebResponse>,
{
    type Response = WebResponse;
    type Error = S::Error;

    ntex::forward_ready!(service);

    async fn call(
        &self,
        req: WebRequest<E>,
        ctx: ServiceCtx<'_, Self>,
    ) -> Result<Self::Response, Self::Error> {
        trace!(
            "Inspecting authorization for {} {:?} to access {}",
            req.connection_info().host(),
            req.peer_addr(),
            req.uri()
        );

        let Some(authz) = req.headers().get("Authorization") else {
            trace!("Missing authorization information. Refusing.");

            return Ok(req.into_response(
                HttpResponse::build(StatusCode::BAD_REQUEST).body("Authorization is absent"),
            ));
        };

        let Ok(auth_str) = authz.to_str() else {
            trace!("Authorization data {authz:?} is unacceptable. Refusing.");

            return Ok(req.into_response(
                HttpResponse::build(StatusCode::BAD_REQUEST)
                    .body("Authorization data is incorrect"),
            ));
        };

        let token_hash = if auth_str.starts_with("Bearer") {
            auth_str
                .trim_start_matches("Bearer")
                .trim()
                .to_string()
                .as_bytes()
                .digest()
        } else {
            return Ok(req.into_response(
                HttpResponse::build(StatusCode::NOT_ACCEPTABLE)
                    .body("Authorization data is incorrect"),
            ));
        };

        trace!("Authorization token hash {token_hash}");
        trace!(
            "Authorization token hashes {:#?}",
            self.kyc_tokens
                .iter()
                .map(|t| t.token_hash().to_string())
                .collect::<Vec<_>>()
        );

        // Check if the token provides authorization
        let is_authorized = self.kyc_tokens.iter().any(|kyc_token| {
            // Check if the provided token matches an authorized token.
            if *kyc_token.token_hash() != token_hash {
                return false;
            }

            // Token hash has matched.
            // Check if the matched token comes from the right source.
            match &kyc_token.authorized_source() {
                AuthorizedSource::Host(host) => {
                    // Hostname may include a port, e.g., localhost:8080,
                    // therefore we require any of the split parts match `host`.
                    req.connection_info()
                        .host()
                        .split(":")
                        .any(|part| part == host)
                }
                AuthorizedSource::IpAddr(ip) => req
                    .peer_addr()
                    .map(|peer_addr| peer_addr.ip() == *ip)
                    .unwrap_or_default(),
            }
        });

        if !is_authorized {
            warn!("Authorization data does not provide authorization. Refusing.");

            return Ok(req.into_response(HttpResponse::Unauthorized().finish()));
        }

        trace!(
            "Authorizing {host} {addr:?}: {token_hash}",
            host = req.connection_info().host(),
            addr = req.peer_addr()
        );

        let res = ctx.call(&self.service, req).await?;

        Ok(res)
    }
}
