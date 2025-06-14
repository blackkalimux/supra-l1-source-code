#![allow(deprecated)] // To support API v1.

use crate::error::{Error, Result};
use crate::rest::api_server_error::ApiServerError;
use crate::rest::{ApiServerState, PublicApiServerError};
use anyhow::anyhow;
use lifecycle::ChainId;
use mempool::BatchProposerMoveTransactionVerifier;
use ntex::web;
use ntex::web::types::{Json, Path, State};
use socrypto::{Digest as _, Hash};
use std::time::Instant;
use tracing::debug;
use traits::Verifier;
use transactions::{TSignerData, TransactionKind};
use types::api::v1::TransactionInfoV1;
use types::api::v1::{GasPriceRes, TransactionParameters};
use types::api::{self, v1};
use types::transactions::smr_transaction::SmrTransaction;
use types::transactions::transaction::SupraTransaction;
use types::transactions::transaction_verifier::TransactionVerifier;

/// Chain id  v1
///
/// Fetch chain id
#[utoipa::path(
    get,
    path = "/rpc/v1/transactions/chain_id",
    operation_id = "chain_id",
    responses(
        (status = 200, description = "Current chain id", content((u8 = "application/json")))
    )
)]
#[web::get("/chain_id")]
pub async fn chain_id(state: State<ApiServerState>) -> Result<Json<ChainId>> {
    Ok(Json(state.chain_id()))
}

/// Estimate gas price v1
///
/// Get statistics derived from the gas prices of recently executed transactions.
#[utoipa::path(
    get,
    path = "/rpc/v1/transactions/estimate_gas_price",
    operation_id = "estimate_gas_price_v1",
    responses(
        (status = 200, description = "Returns the mean and maximum gas prices of recently executed transactions.", content((GasPriceRes = "application/json")))
    ),
)]
#[web::get("/estimate_gas_price")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn estimate_gas_price(state: State<ApiServerState>) -> Result<Json<GasPriceRes>> {
    Ok(state
        .gas_price_reader
        .statistics(TransactionKind::MoveVm)
        .map(GasPriceRes::from)
        .map(Json)
        .expect("Gas prices stats for MoveVM must be maintained"))
}

#[deprecated(note = "This function is deprecated.")]
pub(crate) async fn transaction_by_hash_internal(
    state: State<ApiServerState>,
    hash: Hash,
) -> Result<Json<Option<v1::TransactionInfoV1>>> {
    debug!("Received a query for transaction {hash}");
    let info = match state.get_user_transaction_by_hash(hash, true) {
        Ok(option) => Ok(option.map(v1::TransactionInfoV1::from)),
        Err(PublicApiServerError(ApiServerError::TransactionNotFound(_))) => Ok(None),
        Err(err) => Err(Error::PublicApiServerError(err)),
    }?;

    Ok(Json(info))
}

/// Get transaction by hash v1
///
/// Get information about a transaction by its hash.
#[utoipa::path(
    get,
    path = "/rpc/v1/transactions/{hash}",
    operation_id = "transaction_by_hash_v1",
    params(
        ("hash" = String, Path, description = "Hash of the transaction to lookup")
    ),
    responses(
        (status = 200, description = "Transaction data of the given transaction hash", content((Option<TransactionInfoV1> = "application/json")))
    ),
)]
#[web::get("/{hash}")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn transaction_by_hash(
    state: State<ApiServerState>,
    hash: Path<Hash>,
) -> Result<Json<Option<TransactionInfoV1>>> {
    transaction_by_hash_internal(state, hash.into_inner()).await
}

// TODO: The SDK should construct SmrTransactions so that the client can know the hash of
// their transaction without having to trust the RPC node. After the [SmrTransaction]
// refactor is integrated, this API should take [SmrTransaction] instead of [SupraTransaction].
//
/// Submit transaction v1
///
/// Submit a transaction to Supra.
#[utoipa::path(
    post,
    path = "/rpc/v1/transactions/submit",
    operation_id = "submit_txn",
    request_body(content = api::delegates::SupraTransaction, content_type = "application/json"),
    responses(
    (status = 200, description = "transaction submitted", content((api::delegates::Hash = "application/json")))
    )
)]
#[web::post("/submit")]
pub async fn submit_txn(
    state: State<ApiServerState>,
    req: Json<SupraTransaction>,
) -> Result<Json<Hash>> {
    let start = Instant::now();
    let supra_tx = req.into_inner();

    if let Some(ref limiter) = state.rest_call_limiter {
        let mut guard = limiter
            .write()
            .map_err(|_| anyhow!("PoisonError while holding lock RootStateCache"))?;

        // Limit how often the sender can submit txs.
        match supra_tx {
            SupraTransaction::Move(ref aptos_tx) => guard.register_account(aptos_tx.sender()),
            SupraTransaction::Smr(ref smr_tx) => guard.register_public_key(*smr_tx.signer()),
        }?;
    }

    let smr_tx = SmrTransaction::from(supra_tx);
    let txn_digest = smr_tx.digest();
    debug!("Received submitted transaction: {txn_digest}");

    let smr_tx2 = smr_tx.clone();
    let move_state = state.move_store.clone();
    // Run the transaction validation in a blocking task.
    // Each ntex worker is run in a single-thread tokio runtime, blocking operations should not run in the worker context.
    tokio::task::spawn_blocking(move || {
        let verifier =
            TransactionVerifier::new(BatchProposerMoveTransactionVerifier::new(move_state));
        debug!(
            "Created verifier for submitted transaction {txn_digest} in {:?} us",
            start.elapsed().as_micros()
        );

        verifier
            .verify(&smr_tx2)
            .map_err(|e| Error::BadTransactionRequest(e.to_string()))
    })
    .await
    .map_err(|e| Error::Other(anyhow!("Transaction validation task failed: {e}")))??;
    debug!(
        "Verified submitted transaction {txn_digest} in {:?} us",
        start.elapsed().as_micros()
    );
    // Now submit the transaction.
    // Send tx for execution to validators.
    let tx_hash = state
        .get_ref()
        .tx_sender
        .send(smr_tx)
        .await
        .inspect_err(|e| debug!("Submitted transaction redirection failed: {txn_digest}: {e} "))?;
    debug!(
        "Submitted transaction {txn_digest} in {:?} us",
        start.elapsed().as_micros()
    );
    Ok(Json(tx_hash))
}

/// Simulate a transaction v1
#[utoipa::path(
    post,
    path = "/rpc/v1/transactions/simulate",
    operation_id = "simulate_txn_v1",
    request_body(
        content = api::delegates::SupraTransaction,
        content_type = "application/json",
        example = json!({
            "Move": {
                "raw_txn": {
                "sender": "3ef46383308ad31c00ce1ca02bf113a9327a94e302d934f32f70954bb4cba035",
                "sequence_number": 0,
                "payload": {
                    "EntryFunction": {
                    "module": {
                        "address": "0x0000000000000000000000000000000000000000000000000000000000000001",
                        "name": "supra_account"
                    },
                    "function": "transfer",
                    "ty_args": [
                    ],
                    "args": [
                        [
                        234,
                        100,
                        210,
                        42,
                        5,
                        4,
                        135,
                        249,
                        11,
                        193,
                        212,
                        28,
                        224,
                        115,
                        170,
                        202,
                        20,
                        90,
                        51,
                        180,
                        133,
                        54,
                        138,
                        63,
                        213,
                        95,
                        209,
                        36,
                        173,
                        235,
                        38,
                        224
                        ],
                        [
                        0,
                        45,
                        49,
                        1,
                        0,
                        0,
                        0,
                        0
                        ]
                    ]
                    }
                },
                "max_gas_amount": 500000,
                "gas_unit_price": 100,
                "expiration_timestamp_secs": 1841875694,
                "chain_id": 255
                },
                "authenticator": {
                "Ed25519": {
                    "public_key": "0x7b017e7c4b83e06ad2dab990ff3b091f9995a2a2610dee1d665dc8a2510b539f",
                    "signature": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
                }
                }
            }
        })
    ),
    responses(
        (status = 200, description = "Simulate a transaction against the current state of the RPC node. The transaction must have an invalid signature.", content((TransactionInfoV1 = "application/json")))
    ),
)]
#[web::post("/simulate")]
#[deprecated(note = "This api is deprecated, use v2 instead.")]
pub async fn simulate_txn(
    state: State<ApiServerState>,
    req: Json<SupraTransaction>,
) -> Result<Json<TransactionInfoV1>> {
    debug!("Received simulation request for transaction: {req}");
    match req.0 {
        SupraTransaction::Smr(_) => Err(Error::BadTransactionRequest(
            "Only move transaction simulation is supported for now".to_string(),
        )),
        SupraTransaction::Move(txn) => state
            .run_move_transaction_simulation(txn)
            .await
            .map(TransactionInfoV1::from)
            .map(Json)
            .map_err(Error::from),
    }
}

/// Simulate a transaction (RPC Node) v1
///
/// Simulate a transaction in scope of RPC node. Simulated transaction must not have a valid signature.
#[utoipa::path(
    get,
    path = "/rpc/v1/transactions/parameters",
    operation_id = "transaction_parameter",
    request_body( content = api::delegates::SupraTransaction, content_type = "application/json"),
    responses(
        (status = 200, description = "Acceptable parameters for transaction submission", body = TransactionParameters)
    )
)]
#[web::get("/parameters")]
pub async fn transaction_parameters(state: State<ApiServerState>) -> Json<TransactionParameters> {
    debug!("Received a request for transaction parameters");

    Json(TransactionParameters {
        max_transaction_time_to_live_seconds: state.storage.get_max_tx_ttl_secs(),
    })
}

#[cfg(test)]
mod tests {
    use crate::gas::GasPriceStatistics;
    use committees::GenesisStateProvider;
    use execution::test_utils::move_store::blockchain_stores_with_genesis_state;
    use mempool::BatchProposerMoveTransactionVerifier;
    use smr_timestamp::SmrTimestamp;
    use traits::Verifier;
    use transactions::{SmrTransactionHeaderBuilder, TTransactionHeaderProperties};
    use types::settings::test_utils::extended_genesis_state_provider::TestExtendedGenesisStateProvider;
    use types::tests::utils::transaction_maker::TransactionGenerator;
    use types::transactions::transaction_verifier::TransactionVerifier;

    #[test]
    fn check_transaction_verification() {
        let account_number = 4;
        let mut genesis_state_provider = TestExtendedGenesisStateProvider::default();
        let chain_id = genesis_state_provider.chain_id();
        let (_, move_store) = blockchain_stores_with_genesis_state(&mut genesis_state_provider);

        let mut transaction_generator = TransactionGenerator::new(account_number, chain_id);
        let txn1 = transaction_generator.create_account(0);
        let txn2 = transaction_generator.create_account(1);
        let txn3 = transaction_generator.create_account(3);
        let verifier =
            TransactionVerifier::new(BatchProposerMoveTransactionVerifier::new(move_store));

        assert!(verifier.verify(&txn1).is_ok());
        assert!(verifier.verify(&txn3).is_ok());
        assert!(verifier.verify(&txn2).is_ok());
        let header = SmrTransactionHeaderBuilder::new()
            .with_gas_price(GasPriceStatistics::MVM_MIN_GAS_UNIT_PRICE)
            .with_expiration_timestamp(SmrTimestamp::seconds_from_now(5))
            .with_sender(txn1.sender())
            .with_chain_id(chain_id)
            .with_max_gas_amount(GasPriceStatistics::MVM_MIN_AMOUNT_GAS_UNIT - 1)
            .with_sequence_number(5)
            .build()
            .expect("Successful transaction header creation");
        let invalid_gas_amount = transaction_generator.create_transfer_transaction_with_header(
            0,
            1,
            100,
            header.clone(),
        );
        assert!(verifier.verify(&invalid_gas_amount).is_err());
        let header = SmrTransactionHeaderBuilder::from(&header)
            .with_gas_price(GasPriceStatistics::MVM_MIN_GAS_UNIT_PRICE - 1)
            .with_expiration_timestamp(SmrTimestamp::seconds_from_now(5))
            .with_max_gas_amount(GasPriceStatistics::MVM_MIN_AMOUNT_GAS_UNIT)
            .with_sequence_number(6)
            .build()
            .expect("Successful transaction header creation");
        let invalid_gas_unit =
            transaction_generator.create_transfer_transaction_with_header(0, 1, 100, header);
        let res = verifier.verify(&invalid_gas_unit);
        println!("{:?}", res);
        assert!(verifier.verify(&invalid_gas_unit).is_err());
    }
}
