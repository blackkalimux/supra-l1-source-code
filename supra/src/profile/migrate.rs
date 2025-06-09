use crate::common::error::CliError;
use crate::profile::ProfileToolResult;
use anyhow::anyhow;
use clap::Args;
use encryption::error::EncryptionError;
use encryption::{prompt_password, PasswordPolicyValidator};
use regex::Regex;
use types::cli_profile::profile_management::{CliProfileError, ProtectedProfileManager};
use types::version_compat::past_support::release_7::cli_files::ProtectedProfileManager6;

#[derive(Debug)]
pub enum ProfileErrorType {
    ChainId,
    Undefined,
}

impl TryFrom<&str> for ProfileErrorType {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "chain_id" => Ok(ProfileErrorType::ChainId),
            _ => Err(anyhow!("capture key: {}", value)),
        }
    }
}

fn parse_error(msg: &str) -> Result<ProfileErrorType, anyhow::Error> {
    let re = Regex::new(r"missing field `([^`]+)`")?;

    if let Some(captures) = re.captures(msg) {
        ProfileErrorType::try_from(captures.get(1).map_or("", |m| m.as_str()))
    } else {
        Ok(ProfileErrorType::Undefined)
    }
}

/// Set CLI profiles to align with the latest policy.
///
/// This command updates the CLI profile format to match new requirements,
/// and enforces updated password policies by prompting for a secure password if necessary.
#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct Migrate {}

impl Migrate {
    pub async fn execute(self) -> Result<ProfileToolResult, CliError> {
        match ProtectedProfileManager::read_or_create(false, false) {
            Ok((profile_manager, password)) => {
                if PasswordPolicyValidator::is_weak_password(&password, &[]) {
                    println!("{}", EncryptionError::WeakPassword);
                } else {
                    println!("All of your Supra CLI profiles already meet the latest security policy requirements.\
                    No action is required.")
                }

                let cli_profile = profile_manager.cli_profile_manager();
                cli_profile.write_to_default_path()?;

                Ok(ProfileToolResult::CliProfileManager(cli_profile))
            }
            Err(e) => match &e {
                CliProfileError::Aborted(_, _)
                | CliProfileError::Uninitialized(_, _)
                | CliProfileError::ProfileNameTooLong(_, _)
                | CliProfileError::GeneralError(_)
                | CliProfileError::KeyNotFound(_)
                | CliProfileError::AlreadyExists(_)
                | CliProfileError::StdIoError(_)
                | CliProfileError::Encryption(_)
                | CliProfileError::FileIoError(_)
                | CliProfileError::ProfileFile(_) => Err(CliError::ProfileError(e)),

                CliProfileError::SerdeJsonError(err) => {
                    println!("Your CLI profile format is outdated and needs to be migrated to the latest version.");

                    let password = prompt_password(
                        "Please enter the current password for your existing CLI profiles: ",
                    )?;

                    if err.is_data() {
                        let cli_profile = match parse_error(err.to_string().as_str())? {
                            ProfileErrorType::ChainId => {
                                ProtectedProfileManager6::migrate_to_cli_profile_with_chain_id(
                                    &password,
                                )
                                .await?
                            }
                            ProfileErrorType::Undefined => {
                                return Err(CliError::ProfileError(CliProfileError::Aborted(
                                    "Unable to migrate CLI profile to the new version.".to_string(),
                                    "Please contact the Supra team for further assistance."
                                        .to_string(),
                                )))
                            }
                        };
                        Ok(ProfileToolResult::CliProfileManager(cli_profile))
                    } else {
                        Err(CliError::ProfileError(e))
                    }
                }
            },
        }
    }
}
