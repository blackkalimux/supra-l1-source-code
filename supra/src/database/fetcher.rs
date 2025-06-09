use anyhow::Result;
use aptos_api_types::AsConverter;
use archive::schema::{
    AccountAutomatedTxnsSchema, AccountFinalizedCoinTxSchema, AccountFinalizedTxSchema,
    AutomatedTransactionSchema, AutomationTaskSchema, BlockHeaderInfoSchema,
    BlockHeightToHashSchema, BlockMetadataTxnSchema, BlockToAutomatedTransactionSchema,
    BlockToMetadataTxnSchema, BlockToTransactionSchema, EventByTypeSchema,
    TransactionHashToBlockHashSchema, TransactionOutputSchema, TransactionSchema,
    TransactionStatusSchema,
};
use archive::schemas::coin_transactions::AccountFinalizedCoinTaggedTxSchema;
use archive::schemas::executed_block_statistics::ExecutedBlockStatisticsSchema;
use execution::test_utils::move_store::blockchain_stores_with_genesis_state;
use execution::MoveStore;
use rocksdb::properties::TOTAL_SST_FILES_SIZE;
use rocksdb::{ColumnFamilyDescriptor, IteratorMode, Options, ReadOptions, DB};
use rocksstore::chain_storage::schemas::{
    BatchSchema, CertifiedBlockSchema, EpochAuthorizedCommitteeSchema, EpochCommitteeSchema,
    LastStateSchema, MoveStateSchema, QCSchema, UncommittedBlockSchema,
};
use rocksstore::schema::{PruneIndexSchema, TSimpleCachedTable};
use rocksstore::schema::{RocksDBConfig, Schema, SchemaDBManager};
use serde_json::json;
use std::fs::File;
use std::hash::Hash;
use std::io::{BufWriter, Write as _};
use std::sync::Arc;
use types::settings::test_utils::extended_genesis_state_provider::TestExtendedGenesisStateProvider;

pub struct Fetcher {
    dbm: SchemaDBManager,
    out_dir: String,
    move_store: MoveStore,
    cf_names: Vec<String>,
    selected_table: String,
}

// TODO: Explain why this code inits the store with fake data when it is used to perform real exports.
//
/// To be able to resolve state view, we need to create a MoveStore.
fn create_move_state_store() -> MoveStore {
    let mut genesis_state_provider = TestExtendedGenesisStateProvider::default();
    let (blockchain_store, ..) = blockchain_stores_with_genesis_state(&mut genesis_state_provider);
    let owner_validator_mapping = genesis_state_provider.owner_validator_mapping_vec();
    // We only need the framework bundle write set.
    // TODO: better to create common FakeMoveStore for testing and toolings.
    MoveStore::load_or_create_genesis(
        &mut genesis_state_provider,
        &blockchain_store,
        Some(owner_validator_mapping),
    )
    .expect("failed to load or create genesis")
}

impl Fetcher {
    pub fn new(db_path: &str, out_dir: &str, selected_table: &str) -> Result<Self> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(false);
        db_opts.create_missing_column_families(false);
        // Read all column families existing in the database.
        let cf_names = DB::list_cf(&db_opts, db_path)?;

        let config = RocksDBConfig {
            path: db_path.to_owned(),
            options: db_opts,
            cf_descriptors: cf_names
                .iter()
                .map(|cf_name| ColumnFamilyDescriptor::new(cf_name, Options::default()))
                .collect(),
        };
        let dbm =
            SchemaDBManager::new_read_only(config).expect("Creating SchemaDBManager must succeed");

        // Print size of each column family.
        println!("Column families in the database: {db_path}");
        for cf_name in &cf_names {
            let cf = dbm
                .db()
                .cf_handle(cf_name)
                .expect("Column family must exist");
            let cf_size = dbm
                .db()
                .property_int_value_cf(cf, TOTAL_SST_FILES_SIZE)
                .expect("Get total size should succeed")
                .map(human_readable_size)
                .unwrap_or("Unknown".to_owned());
            eprintln!("Column family: {cf_name}, size: {cf_size:?}");
        }

        Ok(Fetcher {
            dbm,
            out_dir: out_dir.to_owned(),
            move_store: create_move_state_store(),
            cf_names,
            selected_table: selected_table.to_owned(),
        })
    }

    pub fn cf_names(&self) -> &[String] {
        &self.cf_names
    }

    pub fn will_export(&self, cf_name: &str) -> bool {
        self.cf_names.contains(&cf_name.to_owned()) || cf_name == "all"
    }
    /// Export schema table to json file.
    /// The key and value of the schema table will be converted to [serde_json::Value] by the given function `conv_fn`.
    pub fn export_schema_table<S: Schema, F>(&mut self, conv_fn: F) -> Result<()>
    where
        F: Fn(S::Key, S::Value) -> serde_json::Value,
        S::Key: Eq + Hash,
        S::Value: Clone,
    {
        const BATCH_SIZE: usize = 10_000;
        let table = self.dbm.create_schema_table::<S>(None)?;
        let file_path = format!("{}/{}.json", self.out_dir, table.name());

        // Truncate the file if it exists and open the JSON array.
        std::fs::create_dir_all(self.out_dir.clone())?;
        let file = File::create(file_path)?;
        let mut writer = BufWriter::new(file);
        writeln!(writer, "[")?;

        // Populate the JSON array progressively so that we do not have to store too much data in memory.
        for (i, item) in table
            .iter(ReadOptions::default(), IteratorMode::Start)
            .enumerate()
        {
            if i != 0 {
                // Add the comma for the previous entry. Ensures that the last entry
                // doesn't have a comma.
                write!(writer, ",")?;
            }

            if i % BATCH_SIZE == 0 {
                writer.flush()?;
            }

            let (key, value) = item.expect("Read key-value pair must succeed");
            let json_value = conv_fn(key, value);
            // Manually add the indentation for each array element.
            write!(writer, "{}", serde_json::to_string_pretty(&json_value)?)?;
        }

        // Close the JSON array.
        writeln!(writer, "]")?;
        writer.flush()?;
        Ok(())
    }

    fn should_export<S: Schema>(&self) -> bool {
        self.selected_table == S::TABLE || self.selected_table == "all"
    }

    pub fn export_tx(&mut self) -> Result<()> {
        if !self.should_export::<TransactionSchema>() {
            return Ok(());
        }
        self.export_schema_table::<TransactionSchema, _>(|key, value| {
            // SmrTransaction is serialized into intermediate format, do manual conversion to provide user-friendly output.
            match (value.move_transaction(), value.signed_smr_transaction()) {
                (Some(move_txn), None) => json!({
                    "tx_hash": key,
                    "tx": move_txn
                }),
                (None, Some(smr_txn)) => json!({
                    "tx_hash": key,
                    "tx": smr_txn
                }),
                (None, None) => json!({
                    "tx_hash": key,
                    "tx": "Unknown"
                }),
                (Some(_), Some(_)) => panic!("Transaction cannot be both Move and Smr"),
            }
        })
    }

    pub fn export_automation_tasks(&mut self) -> Result<()> {
        if !self.should_export::<AutomationTaskSchema>() {
            return Ok(());
        }
        self.export_schema_table::<AutomationTaskSchema, _>(|_key, value| json!(value))
    }

    pub fn export_automated_transaction(&mut self) -> Result<()> {
        if !self.should_export::<AutomatedTransactionSchema>() {
            return Ok(());
        }
        self.export_schema_table::<AutomatedTransactionSchema, _>(|key, value| {
            json!({
                "tx_hash": key,
                "tx_meta": value
            })
        })
    }

    pub fn export_block_metadata(&mut self) -> Result<()> {
        if !self.should_export::<BlockMetadataTxnSchema>() {
            return Ok(());
        }
        self.export_schema_table::<BlockMetadataTxnSchema, _>(|key, _value| {
            json!({
                "tx_hash": key,
            })
        })
    }

    pub fn export_block_to_metadata_txn(&mut self) -> Result<()> {
        if !self.should_export::<BlockToMetadataTxnSchema>() {
            return Ok(());
        }
        self.export_schema_table::<BlockToMetadataTxnSchema, _>(|key, value| {
            json!({
                "block_height": key,
                "tx_hash": value,
            })
        })
    }

    pub fn export_block_to_tx(&mut self) -> Result<()> {
        if !self.should_export::<BlockToTransactionSchema>() {
            return Ok(());
        }
        self.export_schema_table::<BlockToTransactionSchema, _>(|key, _| {
            json!({
                "block_height": key.0,
                "tx_hash": key.1,
            })
        })
    }

    pub fn export_block_automated_tx(&mut self) -> Result<()> {
        if !self.should_export::<BlockToAutomatedTransactionSchema>() {
            return Ok(());
        }
        self.export_schema_table::<BlockToAutomatedTransactionSchema, _>(|key, _| {
            json!({
                "block_height": key.0,
                "tx_hash": key.1,
            })
        })
    }

    pub fn export_tx_status(&mut self) -> Result<()> {
        if !self.should_export::<TransactionStatusSchema>() {
            return Ok(());
        }
        self.export_schema_table::<TransactionStatusSchema, _>(|key, value| {
            json!({
                "tx_hash": key,
                "status": value
            })
        })
    }

    pub fn export_tx_output(&mut self) -> Result<()> {
        if !self.should_export::<TransactionOutputSchema>() {
            return Ok(());
        }
        self.export_schema_table::<TransactionOutputSchema, _>(|key, value| {
            json!({
                "tx_hash": key,
                "output": value
            })
        })
    }

    pub fn export_tx_block_info(&mut self) -> Result<()> {
        if !self.should_export::<TransactionHashToBlockHashSchema>() {
            return Ok(());
        }
        self.export_schema_table::<TransactionHashToBlockHashSchema, _>(|key, value| {
            json!({
                "tx_hash": key,
                "block_hash": value
            })
        })
    }

    pub fn export_block_height_to_hash(&mut self) -> Result<()> {
        if !self.should_export::<BlockHeightToHashSchema>() {
            return Ok(());
        }
        self.export_schema_table::<BlockHeightToHashSchema, _>(|key, value| {
            json!({
                "block_height": key,
                "block_hash": value
            })
        })
    }

    pub fn export_block_header_info(&mut self) -> Result<()> {
        if !self.should_export::<BlockHeaderInfoSchema>() {
            return Ok(());
        }
        self.export_schema_table::<BlockHeaderInfoSchema, _>(|key, value| {
            json!({
                "block_hash": key,
                "block_header_info": value
            })
        })
    }

    pub fn export_account_tx(&mut self) -> Result<()> {
        if !self.should_export::<AccountFinalizedTxSchema>() {
            return Ok(());
        }
        self.export_schema_table::<AccountFinalizedTxSchema, _>(|key, _| {
            let (account, sequence_number, tx_hash) = key.into();
            json!({
                "account": account,
                "sequence_number": sequence_number,
                "tx_hash": tx_hash
            })
        })
    }

    pub fn export_account_automated_tx(&mut self) -> Result<()> {
        if !self.should_export::<AccountAutomatedTxnsSchema>() {
            return Ok(());
        }
        self.export_schema_table::<AccountAutomatedTxnsSchema, _>(|key, _| {
            let (account, block_height, tx_hash) = key.into();
            json!({
                "account": account,
                "block": block_height,
                "tx_hash": tx_hash
            })
        })
    }

    pub fn export_account_coin_tx(&mut self) -> Result<()> {
        if !self.should_export::<AccountFinalizedCoinTxSchema>() {
            return Ok(());
        }
        self.export_schema_table::<AccountFinalizedCoinTxSchema, _>(|key, _| {
            let (account, cursor, tx_hash) = key.into();
            json!({
                "account": account,
                "cursor": cursor,
                "tx_hash": tx_hash
            })
        })
    }

    pub fn export_account_coin_tagged_tx(&mut self) -> Result<()> {
        if !self.should_export::<AccountFinalizedCoinTaggedTxSchema>() {
            return Ok(());
        }
        self.export_schema_table::<AccountFinalizedCoinTaggedTxSchema, _>(|key, _| {
            let (account, cursor, tx_hash) = key.into();
            json!({
                "account": account,
                "cursor": cursor,
                "tx_hash": tx_hash
            })
        })
    }

    pub fn export_move_state(&mut self) -> Result<()> {
        if !self.should_export::<MoveStateSchema>() {
            return Ok(());
        }
        self.export_schema_table::<MoveStateSchema, _>(|key, value| {
            json!({
                "state_key": format!("{key:?}"),
                "state_value": format!("{value:?}")
            })
        })
    }

    pub fn export_prune_index(&mut self) -> Result<()> {
        if !self.should_export::<PruneIndexSchema>() {
            return Ok(());
        }
        self.export_schema_table::<PruneIndexSchema, _>(|key, value| {
            json!({
                "timestamp": key,
                "table_name": value.table_name,
                "store_key": hex::encode(value.raw_key), // This is raw bytes, so encode it to hex.
            })
        })
    }

    pub fn export_executed_block_stats(&mut self) -> Result<()> {
        if !self.should_export::<ExecutedBlockStatisticsSchema>() {
            return Ok(());
        }
        self.export_schema_table::<ExecutedBlockStatisticsSchema, _>(|key, value| {
            json!({
                "block_height": key,
                "stats": value
            })
        })
    }

    pub fn export_epoch_authorized_committee(&mut self) -> Result<()> {
        if !self.should_export::<EpochAuthorizedCommitteeSchema>() {
            return Ok(());
        }
        self.export_schema_table::<EpochAuthorizedCommitteeSchema, _>(|key, value| {
            json!({
                "epoch": key,
                "authorized_committee": value
            })
        })
    }

    pub fn export_epoch_committee(&mut self) -> Result<()> {
        if !self.should_export::<EpochCommitteeSchema>() {
            return Ok(());
        }
        self.export_schema_table::<EpochCommitteeSchema, _>(|key, value| {
            json!({
                "epoch": key,
                "committee": value
            })
        })
    }

    pub fn export_batch(&mut self) -> Result<()> {
        if !self.should_export::<BatchSchema>() {
            return Ok(());
        }
        self.export_schema_table::<BatchSchema, _>(|key, value| {
            json!({
                "batch_hash": key,
                "batch": value,
            })
        })
    }
    pub fn export_uncommitted_block(&mut self) -> Result<()> {
        if !self.should_export::<UncommittedBlockSchema>() {
            return Ok(());
        }
        self.export_schema_table::<UncommittedBlockSchema, _>(|key, value| {
            json!({
                "block_hash": key,
                "block": value
            })
        })
    }

    pub fn export_certified_block(&mut self) -> Result<()> {
        if !self.should_export::<CertifiedBlockSchema>() {
            return Ok(());
        }
        self.export_schema_table::<CertifiedBlockSchema, _>(|key, value| {
            json!({
                "block_height": key,
                "block": value
            })
        })
    }

    pub fn export_qc(&mut self) -> Result<()> {
        if !self.should_export::<QCSchema>() {
            return Ok(());
        }
        self.export_schema_table::<QCSchema, _>(|key, value| {
            json!({
                "qc_hash": key,
                "qc": value
            })
        })
    }

    pub fn export_last_state(&mut self) -> Result<()> {
        if !self.should_export::<LastStateSchema>() {
            return Ok(());
        }
        self.export_schema_table::<LastStateSchema, _>(|key, value| {
            json!({
                "name": format!("{key:?}"),
                "state_item": format!("{value:?}"),
            })
        })
    }

    pub fn export_event_by_type(&mut self) -> Result<()> {
        if !self.should_export::<EventByTypeSchema>() {
            return Ok(());
        }
        let move_store = self.move_store.clone();
        let converter = move_store.as_converter(Arc::new(move_store.clone()), None);
        self.export_schema_table::<EventByTypeSchema, _>(|key, value| {
            let events = converter
                .try_into_events(&[value])
                .expect("failed to convert into events");
            json!({
                "event_key": key,
                "contract_event": events // TODO: Convert to `Event` api type.
            })
        })
    }
}

fn human_readable_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    match size {
        0..KB => format!("{size} B"),
        KB..MB => format!("{:.2} KB", size as f64 / KB as f64),
        MB..GB => format!("{:.2} MB", size as f64 / MB as f64),
        _ => format!("{:.2} GB", size as f64 / GB as f64),
    }
}
