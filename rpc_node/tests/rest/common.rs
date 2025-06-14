use aptos_crypto::{
    ed25519::{Ed25519PrivateKey, Ed25519PublicKey, ED25519_PRIVATE_KEY_LENGTH},
    PrivateKey,
};
use aptos_types::chain_id::ChainId;
use aptos_types::{
    account_address::AccountAddress,
    transaction::{authenticator::AuthenticationKey, SignedTransaction, Transaction},
};
use archive::db::ArchiveDb;
use batches::BatchHeader;
use committees::{BlockHeader, BlockPayload, CertifiedBlock, SmrBlock, SmrQC};
use execution::{MoveExecutor, MoveStore};
use index_storage_ifc::ArchiveWrite;
use lifecycle::{EpochId, View};
use ntex::http::header::{HeaderName, HeaderValue};
use ntex::{
    http::{self, body::MessageBody as _, Request},
    web::{self, WebResponse},
    Pipeline, Service,
};
use rocksstore::schema::SchemaBatchWriter;
use rpc_node::rest::docs::OaiDocs;
use smr_timestamp::SmrTimestamp;
use socrypto::{Hash, Identity, HASH_LENGTH, IDENTITY_LENGTH};
use std::{collections::HashMap, fmt::Debug, future::poll_fn, sync::LazyLock};
use test_utils::execution_utils::execute_move_vm;
use transactions::SmrTransactionProtocol;
use types::settings::constants::LOCALNET_CHAIN_ID;
use types::tests::utils::transaction_maker::EXPIRATION_DURATION_IN_SECS;
use types::transactions::block_metadata::BlockMetadata;
use types::{
    api::v2::BlockHeaderInfo, tests::utils::transaction_maker::create_user_account_with_ttl,
};
use types::{
    batches::batch::{aggregate_gas_price, BatchPayload, SmrBatch},
    tests::utils::transaction_maker::{transfer_a_b, MoveTransaction},
    transactions::{smr_transaction::SmrTransaction, MoveTransactionOutput},
};
use types::{
    executed_block::ExecutedBlock, tests::utils::transaction_maker::mint_user_account_with_ttl,
};

// Static private key for sender and receiver accounts.
const SENDER_PRIVATE_KEY: [u8; ED25519_PRIVATE_KEY_LENGTH] = [
    169, 99, 200, 144, 41, 217, 139, 46, 249, 33, 169, 217, 134, 33, 100, 243, 97, 37, 214, 102,
    26, 66, 19, 192, 232, 211, 166, 196, 236, 32, 27, 100,
];
const RECEIVER_PRIVATE_KEY: [u8; ED25519_PRIVATE_KEY_LENGTH] = [
    34, 98, 238, 133, 127, 189, 82, 148, 184, 128, 106, 42, 131, 160, 211, 205, 135, 90, 23, 29,
    160, 113, 122, 5, 88, 56, 138, 247, 105, 57, 112, 21,
];
pub const MAGIC_NUMBER: u8 = 0x18;

// (Account, PrivateKey, PublicKey) for sender and receiver accounts.
pub static SENDER_ACCOUNT_KEY_PAIR: LazyLock<(
    AccountAddress,
    Ed25519PrivateKey,
    Ed25519PublicKey,
)> = LazyLock::new(|| {
    let private_key = Ed25519PrivateKey::try_from(SENDER_PRIVATE_KEY.as_slice()).unwrap();
    let public_key = private_key.public_key();
    (
        AuthenticationKey::ed25519(&private_key.public_key()).account_address(),
        private_key,
        public_key,
    )
});
pub static RECEIVER_ACCOUNT_KEY_PAIR: LazyLock<(
    AccountAddress,
    Ed25519PrivateKey,
    Ed25519PublicKey,
)> = LazyLock::new(|| {
    let private_key = Ed25519PrivateKey::try_from(RECEIVER_PRIVATE_KEY.as_slice()).unwrap();
    let public_key = private_key.public_key();
    (
        AuthenticationKey::ed25519(&private_key.public_key()).account_address(),
        private_key,
        public_key,
    )
});

/// Find all API endpoints that contains the given string tag.
pub fn find_api_endpoints_by_tag(tag: &str) -> Vec<String> {
    let apis = OaiDocs::openapi(vec![]);
    apis.paths
        .paths
        .iter()
        .filter_map(|(path, _)| {
            if path.contains(tag) {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Generate a set of transactions, to inject to move store and archive database.
pub fn setup_test_data_fixture(
    move_executor: &MoveExecutor,
    move_store: &MoveStore,
    archive_db: &ArchiveDb,
) {
    setup_test_data_fixture_with_ttl(
        move_executor,
        move_store,
        archive_db,
        Some(EXPIRATION_DURATION_IN_SECS),
    )
}

/// Generate a set of transactions, to inject to move store and archive database, with specified time-to-live.
pub fn setup_test_data_fixture_with_ttl(
    move_executor: &MoveExecutor,
    move_store: &MoveStore,
    archive_db: &ArchiveDb,
    expiration_duration_secs: Option<u64>,
) {
    let txns = transactions_fixture_with_ttl(expiration_duration_secs);
    let txns = txns
        .clone()
        .into_iter()
        .map(|txn| Transaction::UserTransaction(txn.into_inner()))
        .collect::<Vec<_>>();

    // Execute transactions.
    let vm_outputs = execute_move_vm(move_executor, move_store, txns.clone());

    let mut writer = SchemaBatchWriter::default();

    let block_header_info = insert_magic_block(archive_db, &mut writer);

    txns.clone()
        .into_iter()
        .zip(vm_outputs)
        .enumerate()
        .for_each(|(idx, (txn, vm_output))| {
            let smr_txn = SmrTransaction::from(SignedTransaction::try_from(txn).unwrap());
            let output = MoveTransactionOutput::from(vm_output);
            archive_db
                .insert_transaction(
                    &mut writer,
                    &smr_txn,
                    transactions::TxExecutionStatus::Success,
                )
                .unwrap();
            archive_db
                .insert_executed_transaction(
                    block_header_info.hash,
                    block_header_info.clone(),
                    idx as u64,
                    &smr_txn,
                    &types::transactions::transaction_output::TransactionOutput::Move(output),
                    &mut writer,
                )
                .unwrap();
        });

    writer.write_all().unwrap();
}

fn transactions_fixture() -> Vec<aptos_types::transaction::SignatureCheckedTransaction> {
    transactions_fixture_with_ttl(Some(EXPIRATION_DURATION_IN_SECS))
}

fn transactions_fixture_with_ttl(
    expiration_duration_secs: Option<u64>,
) -> Vec<aptos_types::transaction::SignatureCheckedTransaction> {
    // chain id should be consistent with MoveStore.
    let chain_id = ChainId::new(LOCALNET_CHAIN_ID);

    let sender_private_key = SENDER_ACCOUNT_KEY_PAIR.1.clone();
    let sender_account = SENDER_ACCOUNT_KEY_PAIR.0;
    let receiver_account = RECEIVER_ACCOUNT_KEY_PAIR.0;

    // Transactions to be executed and inserted into the archive database.
    let txns = vec![
        MoveTransaction::try_signing_with_root_account(create_user_account_with_ttl(
            sender_account,
            chain_id,
            0,
            expiration_duration_secs,
        )),
        MoveTransaction::try_signing_with_root_account(create_user_account_with_ttl(
            receiver_account,
            chain_id,
            1,
            expiration_duration_secs,
        )),
        MoveTransaction::try_signing_with_root_account(mint_user_account_with_ttl(
            sender_account,
            50_000_000,
            chain_id,
            2,
            expiration_duration_secs,
        )),
        MoveTransaction::try_signing_with_root_account(mint_user_account_with_ttl(
            receiver_account,
            1_000,
            chain_id,
            3,
            expiration_duration_secs,
        )),
        MoveTransaction::try_signing_with_user_account(
            transfer_a_b(
                sender_account,
                receiver_account,
                2_000,
                MoveTransaction::transaction_header_from_chain_id_seq_number(
                    sender_account,
                    chain_id,
                    0,
                    expiration_duration_secs,
                ),
            ),
            sender_private_key.public_key(),
            &sender_private_key,
        ),
    ];
    txns
}

pub fn batch_fixture(id: Identity, epoch: EpochId, timestamp: SmrTimestamp) -> SmrBatch {
    let txns = transactions_fixture()
        .into_iter()
        .map(|txn| SmrTransaction::from(txn.into_inner()))
        .collect();
    let payload = BatchPayload::new_from(txns);
    let header = BatchHeader::new(
        id,
        epoch,
        SmrTransactionProtocol::Move,
        timestamp,
        aggregate_gas_price(&payload).expect("Successful gas price aggregation"),
    );
    SmrBatch::new_with_payload(header, payload)
}

pub async fn call_endpoint<S: Service<Request, Response = WebResponse, Error = E>, E: Debug>(
    pipeline: &Pipeline<S>,
    endpoint: &str,
    headers: Vec<(HeaderName, HeaderValue)>,
) -> String {
    let (body, _headers) = call_endpoints_with_headers(pipeline, endpoint, headers).await;
    body
}

pub async fn call_endpoints_with_headers<
    S: Service<Request, Response = WebResponse, Error = E>,
    E: Debug,
>(
    pipeline: &Pipeline<S>,
    endpoint: &str,
    headers: Vec<(HeaderName, HeaderValue)>,
) -> (String, HashMap<String, String>) {
    let mut req_builder = web::test::TestRequest::get().uri(endpoint);

    for (name, value) in headers {
        req_builder = req_builder.header(name, value);
    }

    let req = req_builder.to_request();

    let mut resp = pipeline.call(req).await.unwrap();
    let resp_body = poll_fn(|cx| resp.take_body().poll_next_chunk(cx))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        resp.response().status(),
        http::StatusCode::OK,
        "{:?}",
        String::from_utf8_lossy(resp_body.as_ref())
    );

    // Extract headers into a HashMap
    let header_map = resp
        .response()
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();

    let body: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    // Serialize json_value back into a JSON string
    let json_string = serde_json::to_string(&body).unwrap();
    // Deserialize the JSON string into a clean serde_yaml::Value
    let clean_yaml_value: serde_yaml::Value = serde_yaml::from_str(&json_string).unwrap();
    // Serialize the YAML value to a proper YAML string
    let yaml_output = serde_yaml::to_string(&clean_yaml_value).unwrap();

    (yaml_output, header_map)
}

/// The base filters for filtering dynamic content in API responses.
/// Because we aim only for detect  _type_ changes.
pub fn api_response_snapshot_filters() -> Vec<(&'static str, &'static str)> {
    vec![
        (r"signature: 0x[[:xdigit:]]{128}", r"signature: ***"),
        (r"hash: 0x[[:xdigit:]]{64}", r"hash: ***"),
        (r"chain_id: [[:digit:]]+$", r"chain_id: ***,"),
        (r"utc_date_time: [[:digit:]-.:+_T]+", r"utc_date_time: ***"),
        (
            r"microseconds_since_unix_epoch: [[:digit:]]+",
            r"microseconds_since_unix_epoch: ***,",
        ),
        // (r"signatures: [.+?]", r"signatures: ***"),
    ]
}

pub fn insert_magic_block(
    archive_db: &ArchiveDb,
    writer: &mut SchemaBatchWriter,
) -> BlockHeaderInfo {
    let parent_hash = Hash::new([0; HASH_LENGTH]);
    let block_hash = Hash([MAGIC_NUMBER; HASH_LENGTH]);
    let block_time_stamp = SmrTimestamp::from(MAGIC_NUMBER as u64);
    let block_header = BlockHeader::new(
        Identity::new([MAGIC_NUMBER; IDENTITY_LENGTH]),
        MAGIC_NUMBER as u64,
        parent_hash,
        block_time_stamp,
        View::new(LOCALNET_CHAIN_ID, 1, 1),
    );
    let block_header_info = BlockHeaderInfo::new(block_hash, block_header.clone());
    let certified_block = CertifiedBlock::new(
        SmrBlock::new(block_header.clone(), BlockPayload::new()),
        SmrQC::default(),
    );
    let block_metadata = BlockMetadata::dummy(&certified_block);
    let executed_block = ExecutedBlock::new(certified_block, vec![], vec![], block_metadata);
    archive_db.insert_executed_block(executed_block);
    archive_db
        .insert_block_header_info(writer, block_header_info.clone())
        .unwrap();
    block_header_info
}
