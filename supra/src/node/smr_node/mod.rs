use crate::common::error::CliError;
use clap::{Args, Parser};
use configurations::supra::SmrSettings;
use serde::Serialize;
use std::{
    fmt::{Display, Formatter},
    io::Write as _,
    path::PathBuf,
};
use supra_build::build;
use supra_logger::LogFilterReload;
use tracing::info;
use types::settings::path_config::PathConfiguration;

#[cfg(all(feature = "memory_profile", target_os = "linux"))]
pub mod memory_profile;
pub mod run;

#[derive(Serialize)]
pub enum SmrToolResult {
    SmrSetting(Box<SmrSettings>),
    Blank,
}

impl Display for SmrToolResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SmrToolResult::SmrSetting(s) => writeln!(f, "{}", s),
            SmrToolResult::Blank => Ok(()),
        }
    }
}

#[derive(Parser, Clone, Debug, Eq, PartialEq)]
#[clap(about = "Configure and run SMR node.")]
pub enum SmrTool {
    #[clap(about = "Start the SMR node.")]
    Run(RunArgs),
    #[clap(
        about = "Generate settings template for SMR node.",
        long_about = "Generate settings template required to run SMR node. Edit the generated files to customize."
    )]
    Settings(SettingsArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct RunArgs {
    #[arg(short, long)]
    resume: Option<bool>,
    #[arg(
        long,
        help = r#"Disable automatic re-loading of the node identity file during epoch changes.
        If set, the node identity will only be loaded when the node starts and
        any modifications to it will not be applied until the node restarts."#
    )]
    disable_dynamic_identity_loading: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct SettingsArgs {
    /// The Output directory for the configuration files.
    #[arg(short, long)]
    out_dir: Option<String>,
}

impl SmrTool {
    pub async fn execute(
        self,
        log_filter_reload: Option<LogFilterReload>,
    ) -> Result<SmrToolResult, CliError> {
        match self {
            SmrTool::Run(args) => {
                let msg = format!("Build Version: {}", build::VERSION);
                eprintln!("{msg}");
                // Ensure the message is ordered and flushed to stderr
                let _ = std::io::stderr().lock().flush();
                info!("{msg}");
                run::start_validator_node(
                    args.resume,
                    args.disable_dynamic_identity_loading,
                    log_filter_reload,
                )
                .await?;
                Ok(SmrToolResult::Blank)
            }
            SmrTool::Settings(args) => {
                let out_dir = if let Some(out_dir) = args.out_dir {
                    PathBuf::from(out_dir)
                } else {
                    PathConfiguration::supra_home()?
                };

                let out_dir = out_dir.canonicalize()?;
                std::fs::create_dir_all(&out_dir)?;
                let settings = SmrSettings::generate_recommended();
                settings.write_to_dir(&out_dir)?;
                eprintln!(
                    "Exported SMR settings file at: {}",
                    out_dir.join(SmrSettings::path_relative_to_home()).display()
                );
                Ok(SmrToolResult::SmrSetting(Box::new(settings)))
            }
        }
    }
}
