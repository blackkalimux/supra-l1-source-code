use crate::common::error::CliError;
use crate::config::ConfigToolResult;
use clap::Args;
use configurations::supra::SmrSettings;
use file_io_types::{serialize_to_string, TextFileFormat};
use soserde::SmrSerialize;
use std::path::PathBuf;
use types::settings::on_chain_parameters::OnChainParametersV1;

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct OnChainConfigSerializer {
    /// The path to the SmrSettings file that should contain the latest OnChain config parameters
    #[clap(long, short = 'p')]
    path_to_smr_settings: PathBuf,
    /// Path to output file where the serialized data will be written to
    #[clap(long, short = 'o')]
    output_path: PathBuf,
}

impl OnChainConfigSerializer {
    pub fn execute(&self) -> Result<ConfigToolResult, CliError> {
        let new_params = SmrSettings::read_from_file(&self.path_to_smr_settings)?;

        let on_chain_config =
            OnChainParametersV1::new(new_params.mempool().clone(), new_params.moonshot().clone());

        let serialized = serialize_to_string(
            &on_chain_config.to_bytes(),
            &TextFileFormat::Json,
            &format!("{:?}", self.output_path),
            "txt",
        )?;

        std::fs::write(&self.output_path, serialized)?;

        Ok(ConfigToolResult::FileWrittenToDisk(
            self.output_path.clone(),
        ))
    }
}
