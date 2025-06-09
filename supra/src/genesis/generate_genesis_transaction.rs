use crate::{common::error::CliError, genesis::GenesisToolResult};
use clap::Args;
use soserde::SmrSerialize;
use std::fs::File;
use std::io::Write;
use types::genesis::genesis_config::GenesisConfig;
use types::settings::committee::genesis_transaction_generator::GenesisTransactionGenerator;

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct GenerateGenesisTransaction {
    /// The path to the file that should contain the binary-encoded genesis [Transaction].
    #[clap(long)]
    pub output_path: String,
}

impl GenerateGenesisTransaction {
    pub fn execute(self) -> Result<GenesisToolResult, CliError> {
        println!("Loading the genesis config...");
        let config = GenesisConfig::try_from_supra_home()?;
        println!(
            "Generating the genesis transaction... This may take hours if the config is large."
        );
        let serialized_tx = match config {
            GenesisConfig::V0(cfg) => {
                let transaction = GenesisTransactionGenerator::generate_v0(&cfg)?;
                transaction.to_bytes()
            }
            GenesisConfig::V1(cfg) => {
                let transaction = GenesisTransactionGenerator::generate_v1(&cfg)?;
                transaction.to_bytes()
            }
            GenesisConfig::V2(cfg) => {
                let transaction = GenesisTransactionGenerator::generate_v2(cfg)?;
                transaction.to_bytes()
            }
        };
        println!("Writing the genesis transaction to {}...", self.output_path);
        let mut file = File::create(&self.output_path)?;
        file.write_all(&serialized_tx)?;
        Ok(GenesisToolResult::Blank)
    }
}
