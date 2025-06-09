use clap::Parser;
use std::time::Duration;
use supra::common::error::CliError;
use supra::common::stdout_format::StdOutFormat;
use supra::common::verbose::VerboseTool;
use supra::SupraTool;
use supra_build::build;
use supra_logger::LogWorkerGuards;

#[derive(Parser)]
#[clap(
    version = build::COMMIT_HASH,
    name = "Supra CLI",
    about = "Supra Command Line Utilities",
    long_about = build::CLAP_LONG_VERSION
)]
struct SupraCli {
    #[clap(subcommand)]
    cli: SupraTool,
    #[arg(
        short = 'o',
        help = "Set the log level.",
        long_help = "Sets the log level as 1=>Info, 2=>Debug, 3=>Trace, the log file is stored in SUPRA_HOME with name supra.log"
    )]
    log: Option<u8>,
    #[clap(long, default_value_t = StdOutFormat::DisplayTrait)]
    stdout: StdOutFormat,
}

impl SupraCli {
    pub async fn execute(self) -> Result<LogWorkerGuards, CliError> {
        let verbose = VerboseTool::new(self.log);
        let (mut writer, guard, log_filter_reload) = verbose.get_cli_history_writer()?;

        let command_display = self.cli.to_string();

        let cli_execution_result = self.cli.execute(Some(log_filter_reload)).await;

        match cli_execution_result {
            Ok(cli_res) => {
                writer.write_command_history(command_display, cli_res, &self.stdout, true)?;
                Ok(guard)
            }
            Err(cli_err) => {
                writer.write_command_history(
                    command_display,
                    cli_err.to_string(),
                    &StdOutFormat::None,
                    true,
                )?;
                Err(cli_err)
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_name("supra-node")
        .enable_all()
        .build()?;
    let _ = runtime.block_on(SupraCli::parse().execute())?; // error gets to stderr
    runtime.shutdown_timeout(Duration::from_secs(3));
    Ok(())
}
