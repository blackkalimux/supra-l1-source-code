use crate::common::error::CliError;
use crate::common::stdout_format::StdOutFormat;
use chrono::{DateTime, Local, Utc};
use clap::Args;
use serde::Serialize;
use smrcommon::get_random_port;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{stdout, Stdout, Write};
use std::path::PathBuf;
use supra_logger::{init_runtime_logger, LogFilterReload, LogWorkerGuards};
use tracing::level_filters::LevelFilter;
use tracing::{debug, info, trace};
use types::settings::path_config::PathConfiguration;

/// Verbose level
/// ```text
/// 0 => LogLevel::Off,
/// 1 => LogLevel::Info,
/// 2 => LogLevel::Debug,
/// 3 => LogLevel::Trace,
/// 4 => LogLevel::Error,
/// 5 => LogLevel::Warn,
/// _ => LogLevel::Trace,
/// ```
#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct VerboseTool {
    verbose: Option<u8>,
}

/// Handler for writing the cli history to console and file.
pub struct CliHistoryWriter {
    console_writer: Stdout,
    file_writer: File,
}

/// Date, time and offset data which will be used while writing cli history.
pub struct HistoryTime {
    local: DateTime<Local>,
    utc: DateTime<Utc>,
}

impl Display for HistoryTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Local: ({})\t Utc: ({})", self.local, self.utc)
    }
}

impl HistoryTime {
    /// Returns the current date, time and offset which corresponds to the current date, time and offset.
    pub fn now() -> Self {
        Self {
            local: Local::now(),
            utc: Utc::now(),
        }
    }
}

impl CliHistoryWriter {
    /// Create the provided path in append mode and then returns [HistoryWriter](CliHistoryWriter) instance.
    fn new(file_path: PathBuf) -> Result<Self, CliError> {
        let console_writer = stdout();
        let file_writer = OpenOptions::new()
            .create(true)
            .truncate(false)
            .append(true)
            .open(file_path)
            .map_err(CliError::StdIoError)?;

        Ok(Self {
            console_writer,
            file_writer,
        })
    }

    ///  Writes the command and its output to both console and file.
    pub fn write_command_history(
        &mut self,
        command: impl Display,
        output: impl Display + Serialize,
        console: &StdOutFormat,
        file: bool,
    ) -> Result<(), CliError> {
        let time = HistoryTime::now();
        console.dump(&mut self.console_writer, &output)?;
        if file {
            writeln!(self.file_writer, "{}", time)?;
            writeln!(self.file_writer, "CMD:\n{}\nOutput:\n{}", command, output)?;
        }

        Ok(())
    }
}

impl VerboseTool {
    pub fn new(verbose: Option<u8>) -> Self {
        Self { verbose }
    }

    /// Initializes the global file logger and returns [HistoryWriter](CliHistoryWriter) instance which stores the history to [history](Configuration::cli_history()) file.
    ///
    pub fn get_cli_history_writer(
        &self,
    ) -> Result<(CliHistoryWriter, LogWorkerGuards, LogFilterReload), CliError> {
        let log_level: LogLevel = self.verbose.unwrap_or(1).into();
        let (guard, log_filter_reload) = log_level.set_cli_file_logger();
        log_level.set_panic_hook();
        info!("Log level INFO enabled.");
        debug!("Log level DEBUG enabled.");
        trace!("Log level TRACE enabled.");
        CliHistoryWriter::new(PathConfiguration::cli_history()?)
            .map(|writer| (writer, guard, log_filter_reload))
    }
}

pub enum LogLevel {
    Off,
    Info,
    Debug,
    Trace,
    Error,
    Warn,
}

impl From<u8> for LogLevel {
    fn from(value: u8) -> Self {
        match value {
            0 => LogLevel::Off,
            1 => LogLevel::Info,
            2 => LogLevel::Debug,
            3 => LogLevel::Trace,
            4 => LogLevel::Error,
            5 => LogLevel::Warn,
            _ => LogLevel::Trace,
        }
    }
}

impl From<&LogLevel> for LevelFilter {
    fn from(value: &LogLevel) -> Self {
        match value {
            LogLevel::Off => LevelFilter::OFF,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warn => LevelFilter::WARN,
        }
    }
}

impl LogLevel {
    /// must be set after setting the global logger
    /// This function writes the panic message in log file
    fn set_panic_hook(&self) {
        supra_logger::setup_panic_hook();
    }

    ///
    /// Create the provided path in append mode and then initialize the global logger which logs the filtered output to [log_file](Configuration::cli_logs())
    ///
    fn set_cli_file_logger(&self) -> (LogWorkerGuards, LogFilterReload) {
        let tokio_console_port =
            get_random_port().expect("Could not get random port for Tokio console");

        let (guard, log_filter_reload) =
            init_runtime_logger(|| Box::new(std::io::sink()), tokio_console_port)
                .expect("Failed to start logger runtime");

        (guard, log_filter_reload)
    }
}
