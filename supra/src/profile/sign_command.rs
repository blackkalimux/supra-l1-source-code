use crate::common::error::CliError;
use crate::profile::ProfileToolResult;
use crate::rest_utils::tool_option::SupraProfileOptions;
use anyhow::anyhow;
use clap::Parser;
use file_io_types::read_string_from_file;
use serde::Serialize;
use soserde::SmrSerialize;
use std::path::PathBuf;

#[derive(Parser, Clone, Debug, Eq, PartialEq)]
pub struct SignCommand {
    #[clap(flatten)]
    profile_options: SupraProfileOptions,
    #[clap(flatten)]
    pub content: FileOrStdIn,
    #[clap(long)]
    pub expected_signature: Option<String>,
}

#[derive(Parser, Clone, Debug, Eq, PartialEq)]
pub struct FileOrStdIn {
    #[clap(long, group = "content_option")]
    file: Option<PathBuf>,
    #[clap(long, group = "content_option")]
    stdin: Option<String>,
}

pub enum SigningContent {
    File(String, PathBuf),
    StdIn(String),
}

#[derive(Serialize, Debug)]
pub struct ContentSignature {
    signature: String,
}

impl TryFrom<FileOrStdIn> for SigningContent {
    type Error = anyhow::Error;

    fn try_from(value: FileOrStdIn) -> Result<Self, Self::Error> {
        let content = match (value.file, value.stdin) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(anyhow!("Either stdin or file path is allowed"))
            }
            (Some(path), None) => SigningContent::File(
                read_string_from_file(
                    path.to_string_lossy().to_string(),
                    std::any::type_name::<Self>(),
                )?,
                path,
            ),
            (None, Some(content)) => SigningContent::StdIn(content),
        };
        Ok(content)
    }
}

impl SignCommand {
    pub fn execute(self) -> Result<ProfileToolResult, CliError> {
        let (signer_protected_profile, _password) =
            self.profile_options.protected_profile(false)?;
        let private_key = signer_protected_profile.ed25519_secret_key();
        let content = SigningContent::try_from(self.content)?;
        let signature = match content {
            SigningContent::File(content, _path) => {
                let signature = private_key.sign(&content.to_bytes()).to_string();
                ContentSignature { signature }
            }
            SigningContent::StdIn(content) => {
                let signature = private_key.sign(&content.to_bytes()).to_string();
                ContentSignature { signature }
            }
        };
        if let Some(to_verify) = self.expected_signature {
            match to_verify.eq(signature.signature.as_str()) {
                true => eprintln!("Signature is correct"),
                false => eprintln!("Signature is incorrect"),
            }
        };
        Ok(ProfileToolResult::SignatureTool(signature))
    }
}
