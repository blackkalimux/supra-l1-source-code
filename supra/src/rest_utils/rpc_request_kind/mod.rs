use crate::rest_utils::rpc_request_kind::public_query::PublicQueryHelper;
use crate::rest_utils::rpc_request_kind::signed_transaction::SignedTransactionHelper;
use url::Url;

pub mod public_query;
pub mod signed_transaction;

/// Kind of request
///
/// ### SignedTransaction
/// ```rust
/// # use aptos_types::transaction::SignedTransaction;
/// ```
/// Usually POST request with [SignedTransaction] as payload
///
/// ### Public Query
/// Could be GET or POST request where the caller of the API do not need to sign
/// the payload.
#[derive(Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum RpcRequestKind {
    SignedTransaction(SignedTransactionHelper),
    PublicQuery(PublicQueryHelper),
}

impl RpcRequestKind {
    pub fn url(&self) -> Url {
        match self {
            RpcRequestKind::SignedTransaction(helper) => helper.url().clone(),
            RpcRequestKind::PublicQuery(helper) => helper.url().clone(),
        }
    }
}
