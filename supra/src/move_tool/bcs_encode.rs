use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use clap::Args;
use serde_json;

/// Converts the given input string to a JSON byte array using Binary Canonical Serialization (BCS).
#[derive(Args, Debug)]
pub struct BcsJsonEncode {
    /// The string to encode into BCS bytes.
    #[arg(short, long)]
    input: String,
}

impl BcsJsonEncode {
    pub fn execute(&self) -> Result<MoveToolResult, CliError> {
        let encoded = bcs::to_bytes(&self.input)?;
        let output =
            serde_json::to_string(&encoded).expect("Bytes JSON serialization must not fail.");
        Ok(MoveToolResult::String(output))
    }
}
