use crate::common::error::CliError;
use clap::ValueEnum;
use serde::Serialize;
use std::fmt::{Display, Formatter};
use std::io::{Stdout, Write};

#[derive(ValueEnum, Clone, Debug, Eq, PartialEq)]
pub enum StdOutFormat {
    None,
    Json,
    DisplayTrait,
}

impl Display for StdOutFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            StdOutFormat::None => "none",
            StdOutFormat::Json => "json",
            StdOutFormat::DisplayTrait => "display-trait",
        };
        write!(f, "{}", s)
    }
}

impl StdOutFormat {
    pub fn dump<T>(&self, console_writer: &mut Stdout, data: &T) -> Result<(), CliError>
    where
        T: Serialize + Display,
    {
        match self {
            StdOutFormat::None => Ok(()),
            StdOutFormat::Json => writeln!(console_writer, "{}", serde_json::to_string(data)?),
            StdOutFormat::DisplayTrait => writeln!(console_writer, "{}", data),
        }?;
        Ok(())
    }
}
