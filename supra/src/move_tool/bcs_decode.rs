use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use clap::Args;
use serde_json;

/// Converts the given input JSON byte array to a String using Binary Canonical Serialization (BCS).
#[derive(Args, Debug)]
pub struct BcsJsonDecode {
    /// The JSON byte array to decode from BCS bytes into a String.
    #[arg(short, long)]
    input: String,
}

impl BcsJsonDecode {
    pub fn execute(&self) -> Result<MoveToolResult, CliError> {
        let bytes: Vec<u8> = serde_json::from_str(&self.input)?;
        let string_type = bcs::from_bytes::<String>(&bytes)?;
        Ok(MoveToolResult::String(string_type))
    }
}
