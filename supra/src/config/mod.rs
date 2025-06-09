use crate::common::error::CliError;
use crate::config::serialize_on_chain_config::OnChainConfigSerializer;
use clap::{Args, Subcommand};
use configurations::rpc::{KnownOrigin, KnownOriginMode};
use serde::Serialize;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use test_utils::local_testnet::{LocalTestNetConfiguration, RunType};
use types::settings::path_config::PathConfiguration;

pub mod serialize_on_chain_config;

#[derive(Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum ConfigToolResult {
    DisplayConfiguration,
    GenerateConfiguration(LocalTestNetConfiguration),
    FileWrittenToDisk(PathBuf),
}

impl Display for ConfigToolResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DisplayConfiguration => {
                fn format_path_result(
                    result: Result<PathBuf, std::io::Error>,
                    label: &str,
                ) -> String {
                    match result {
                        Ok(path) => format!("{}: {}", label, path.display()),
                        Err(e) => format!("{}: Error retrieving path - {}", label, e),
                    }
                }

                writeln!(
                    f,
                    "{}\n{}\n{}",
                    format_path_result(PathConfiguration::supra_home(), "SUPRA_HOME"),
                    format_path_result(PathConfiguration::cli_history(), "History"),
                    format_path_result(PathConfiguration::cli_logs(), "Logs")
                )
            }
            Self::GenerateConfiguration(config) => {
                writeln!(f, "{:?}", config)
            }
            Self::FileWrittenToDisk(path) => {
                writeln!(
                    f,
                    "Serialized OnChainConfig written to disk at path {:?}",
                    path
                )
            }
        }
    }
}

#[derive(Subcommand, Clone, Debug, Eq, PartialEq)]
#[clap(
    about = "Manage the configurations",
    long_about = "Manage the configurations"
)]
pub enum ConfigTool {
    #[clap(alias = "s", about = "Display the configuration for supra cli tool")]
    Show,
    #[clap(about = "Testing only. Generate local TestNet configurations. Requires .env file.")]
    Generate(GenerateArgs),
    #[clap(
        about = "Generate latest version of serialized OnChainConfig from the supplied SmrSettings file"
    )]
    OnChainConfigSerializer(OnChainConfigSerializer),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct GenerateArgs {
    /// Test running environment type.
    #[arg(value_enum)]
    run_type: RunType,
    /// Force to overwrite the existing test configurations.
    #[arg(short, long)]
    force: bool,
    /// Use the automation parameters from file as reference to set up local testnet, if any specified.
    #[arg(short, long)]
    reference_automation_parameters: Option<PathBuf>,
}

impl Display for ConfigTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl ConfigTool {
    pub fn preloaded_known_origins() -> Vec<KnownOrigin> {
        vec![
            KnownOrigin::new(
                "http://localhost:27000".to_string(),
                Some("LocalNet".to_string()),
                None,
            ),
            KnownOrigin::new(
                "http://localhost:27001".to_string(),
                Some("LocalNet".to_string()),
                None,
            ),
            KnownOrigin::new(
                "http://localhost:27002".to_string(),
                Some("LocalNet".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-qanet.supra.com".to_string(),
                Some("QANet".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-wallet.supra.com".to_string(),
                Some("WalletNet".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-devnet.supra.com".to_string(),
                Some("DevNet".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-staging.supra.com".to_string(),
                Some("StagingNet".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-cex-testnet.supra.com".to_string(),
                Some("Exchange Integration TestNet".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-testnet.supra.com".to_string(),
                Some("RPC For Supra Scan and Faucet".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-testnet1.supra.com".to_string(),
                Some("RPC For nodeops group1".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-testnet2.supra.com".to_string(),
                Some("RPC For nodeops group2".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-testnet3.supra.com".to_string(),
                Some("RPC For nodeops group3".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-testnet4.supra.com".to_string(),
                Some("RPC For nodeops group4".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://rpc-testnet5.supra.com".to_string(),
                Some("RPC For nodeops group5".to_string()),
                None,
            ),
            KnownOrigin::new(
                "https://www.starkey.app".to_string(),
                Some("Starkey domain".to_string()),
                Some(KnownOriginMode::Cors),
            ),
            KnownOrigin::new(
                "chrome-extension://fcpbddmagekkklbcgnjclepnkddbnenp".to_string(),
                Some("Starkey wallet extension".to_string()),
                Some(KnownOriginMode::Cors),
            ),
            KnownOrigin::new(
                "chrome-extension://hcjhpkgbmechpabifbggldplacolbkoh".to_string(),
                Some("Starkey wallet extension".to_string()),
                Some(KnownOriginMode::Cors),
            ),
        ]
    }
    pub async fn execute(self) -> Result<ConfigToolResult, CliError> {
        match self {
            ConfigTool::Show => Ok(ConfigToolResult::DisplayConfiguration),
            ConfigTool::Generate(args) => {
                // The testing utils always return anyhow::Result, so we need to convert it to CliError.
                let config = LocalTestNetConfiguration::build(
                    args.run_type,
                    Self::preloaded_known_origins(),
                    args.force,
                    args.reference_automation_parameters,
                )
                .await?;
                Ok(ConfigToolResult::GenerateConfiguration(config))
            }
            Self::OnChainConfigSerializer(args) => args.execute(),
        }
    }
}
