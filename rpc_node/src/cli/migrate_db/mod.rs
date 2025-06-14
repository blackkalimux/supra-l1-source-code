use clap::Args;
use configurations::rpc::RpcConfig;
use migrations::{DatabaseMigrator, DbMigrationError, MigrationResult};
use std::path::PathBuf;

// Using the constant from the migrations crate

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct Migrate {
    /// The path to the RpcConfig
    #[clap(help = "The path to RpcConfig")]
    rpc_config_path: PathBuf,
    /// Db record writer buffer size.
    #[clap(long, default_value_t = migrations::DEFAULT_MAX_BUFFER_RECORD_COUNT)]
    max_buffer_record_count: usize,
    /// Skip verification steps. Use carefully.
    #[clap(long)]
    skip_verification: bool,
}

impl Migrate {
    pub fn execute(self) -> Result<MigrationResult, DbMigrationError> {
        let config = RpcConfig::read_from_file(&self.rpc_config_path)?;
        config
            .is_well_formed()
            .map_err(DbMigrationError::RpcConfigError)?;
        let database_setup = config.database_setup();

        let migrator = DatabaseMigrator::new(
            database_setup,
            self.max_buffer_record_count,
            self.skip_verification,
        );
        migrator.migrate()
    }
}
