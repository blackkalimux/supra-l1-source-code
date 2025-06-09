mod fetcher;

use crate::common::error::CliError;
use anyhow::Result;
use clap::Parser;
use configurations::{
    databases::{DatabaseSetup, StorageInstance},
    rpc::RpcConfig,
    supra::SmrSettings,
};
pub use fetcher::*;
use migrations::DatabaseMigrator;
use serde::Serialize;
use std::{
    fmt::{Display, Formatter},
    path::PathBuf,
};

/// Arguments for exporting data from RocksDB
#[derive(Parser, Debug, Clone, Eq, PartialEq)]
pub struct ExportAllArgs {
    /// The directory where output JSON files will be saved
    #[clap(help = "Folder to store the output JSON files")]
    pub output_folder: String,

    /// The path to the [SmrSettings] file that should contain the latest [DatabaseSetup].
    /// This can be used instead of providing archive and chain store paths explicitly.
    /// If no paths are provided, [SmrSettings] from the default path will be used.
    #[clap(long, short = 'n', group = "source")]
    pub path_to_smr_settings: Option<PathBuf>,

    /// The path to the [RpcConfig] file that should contain the latest [DatabaseSetup].
    /// This can be used instead of providing archive and chain store paths explicitly.
    #[clap(long, short = 'r', group = "source")]
    pub path_to_rpc_config: Option<PathBuf>,

    /// The path to the RocksDB storage for archive data
    #[clap(long, short, group = "source")]
    pub archive_path: Option<String>,

    /// The path to the RocksDB storage for blockchain store data
    #[clap(long, short, group = "source")]
    pub chain_store_path: Option<String>,

    /// The table name to export.
    #[clap(long, short, default_value = "all")]
    pub table_name: String,

    /// Dry run mode. Only prints the tables.
    #[clap(long, short = 'd')]
    pub dry_run: bool,
}

#[derive(Parser, Clone, Debug, Eq, PartialEq)]
pub struct MigrateArgs {
    /// The path to the SmrSettings file that should contain the latest [DatabaseSetup]
    #[clap(long, short = 'p')]
    path_to_smr_settings: Option<PathBuf>,
    /// Db record writer buffer size.
    #[clap(long, default_value_t = migrations::DEFAULT_MAX_BUFFER_RECORD_COUNT)]
    max_buffer_record_count: usize,
    /// Skip verification steps. Use carefully.
    #[clap(long)]
    skip_verification: bool,
}

#[derive(Parser, Clone, Debug, Eq, PartialEq)]
#[clap(about = "Tool to manage and export database")]
pub enum DatabaseTool {
    #[clap(about = "Export all data from RocksDB to JSON")]
    ExportAll(ExportAllArgs),

    #[clap(about = "Migrate data to the latest schema version")]
    Migrate(MigrateArgs),
}

#[derive(Serialize)]
pub enum DatabaseToolResult {
    ExportAllResult(String),
    MigrateResult(String),
}

impl Display for DatabaseToolResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseToolResult::ExportAllResult(res) => writeln!(f, "{}", res),
            DatabaseToolResult::MigrateResult(res) => writeln!(f, "{}", res),
        }
    }
}

impl Display for DatabaseTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl DatabaseTool {
    pub async fn execute(self) -> Result<DatabaseToolResult, CliError> {
        match self {
            DatabaseTool::ExportAll(args) => args.export_to_json(),
            DatabaseTool::Migrate(migrate_args) => {
                let smr_settings = migrate_args
                    .path_to_smr_settings
                    .map(SmrSettings::read_from_file)
                    .unwrap_or(SmrSettings::read_from_default_path())?;

                let database_setup = smr_settings.node().database_setup();

                let migrator = DatabaseMigrator::new(
                    database_setup,
                    migrate_args.max_buffer_record_count,
                    migrate_args.skip_verification,
                );
                let migration_result = migrator
                    .migrate()
                    .map_err(|e| CliError::DataToolError(e.to_string()))?;

                Ok(DatabaseToolResult::MigrateResult(
                    migration_result.to_string(),
                ))
            }
        }
    }
}

fn database_paths(database_setup: &DatabaseSetup) -> (Option<String>, Option<String>) {
    let archive_db_path = database_setup
        .storage_config(StorageInstance::Archive)
        .and_then(|storage_config| storage_config.path())
        .cloned();

    let blockchain_store_db_path = database_setup
        .storage_config(StorageInstance::ChainStore)
        .and_then(|storage_config| storage_config.path())
        .cloned();

    (archive_db_path, blockchain_store_db_path)
}

impl ExportAllArgs {
    /// Exports column families from a RocksDB database to JSON files.
    ///
    /// # Arguments
    /// * `archive_db_path`: The path to the RocksDB database for archive data.
    /// * `blockchain_store_db_path`: The path to the RocksDB database for blockchain store data.
    /// * `output_folder`: The directory where JSON files will be saved.
    ///
    /// # Errors
    /// Returns an error if any operation with the database fails.
    fn export_to_json(self) -> Result<DatabaseToolResult, CliError> {
        let ExportAllArgs {
            archive_path,
            chain_store_path,
            output_folder,
            path_to_smr_settings,
            path_to_rpc_config,
            table_name,
            dry_run,
        } = self;

        let (archive_path, chain_store_path) = if let Some(path) = path_to_smr_settings {
            let smr_settings = SmrSettings::read_from_file(&path).map_err(|e| {
                CliError::DataToolError(format!(
                    "Failed to read smr settings from {}: {e}",
                    path.display()
                ))
            })?;
            let database_setup = smr_settings.node().database_setup();
            database_paths(database_setup)
        } else if let Some(path) = path_to_rpc_config {
            let rpc_config = RpcConfig::read_from_file(&path).map_err(|e| {
                CliError::DataToolError(format!(
                    "Failed to read rpc config from {}: {e}",
                    path.display()
                ))
            })?;

            let database_setup = rpc_config.database_setup();
            database_paths(database_setup)
        } else if archive_path.is_none() && chain_store_path.is_none() {
            let smr_settings = SmrSettings::read_from_default_path().map_err(|e| {
                CliError::DataToolError(format!(
                    "Failed to read smr settings from default path: {e}, pass path to smr settings or store directories"
                ))
            })?;
            let database_setup = smr_settings.node().database_setup();
            database_paths(database_setup)
        } else {
            (archive_path, chain_store_path)
        };

        if let Some(db_path) = archive_path {
            let mut fetcher = Fetcher::new(&db_path, &output_folder, &table_name)?;

            if !dry_run && fetcher.will_export(&table_name) {
                // Archive data export
                let mut archive_data = || {
                    fetcher.export_tx()?;
                    fetcher.export_block_to_tx()?;
                    fetcher.export_tx_status()?;
                    fetcher.export_account_tx()?;
                    fetcher.export_account_coin_tx()?;
                    fetcher.export_account_coin_tagged_tx()?;
                    fetcher.export_tx_output()?;
                    fetcher.export_tx_block_info()?;
                    fetcher.export_block_height_to_hash()?;
                    fetcher.export_block_header_info()?;
                    fetcher.export_event_by_type()?;
                    fetcher.export_automation_tasks()?;
                    fetcher.export_automated_transaction()?;
                    fetcher.export_account_automated_tx()?;
                    fetcher.export_block_automated_tx()?;
                    fetcher.export_block_metadata()?;
                    fetcher.export_block_to_metadata_txn()?;
                    fetcher.export_executed_block_stats()?;
                    anyhow::Result::<()>::Ok(())
                };

                archive_data().map_err(|e| CliError::DataToolError(format!("Failed to export archive data from {db_path}: {e}. Check if you specified the correct path.")))?;
                println!("'{table_name}' data in archive db exported successfully");
            }
        }

        if let Some(db_path) = chain_store_path {
            let mut fetcher = Fetcher::new(&db_path, &output_folder, &table_name)?;

            if !dry_run && fetcher.will_export(&table_name) {
                // Blockchain store export
                let mut blockchain_store = || {
                    fetcher.export_batch()?;
                    fetcher.export_uncommitted_block()?;
                    fetcher.export_certified_block()?;
                    fetcher.export_qc()?;
                    fetcher.export_epoch_committee()?;
                    fetcher.export_last_state()?;
                    fetcher.export_move_state()?;
                    fetcher.export_prune_index()?;
                    anyhow::Result::<()>::Ok(())
                };

                blockchain_store().map_err(|e| CliError::DataToolError(format!("Failed to export blockchain store from {db_path}: {e}. Check if you specified the correct path.")))?;
                println!("'{table_name}' data in blockchain store exported successfully");
            }
        }

        let output_message = if dry_run {
            "Dry run mode finished.".to_owned()
        } else {
            format!("Data exported to {}", output_folder)
        };
        Ok(DatabaseToolResult::ExportAllResult(output_message))
    }
}
