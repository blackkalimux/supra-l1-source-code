#![allow(deprecated)] // To support API v1.

use crate::common::error::CliError;
use crate::profile::sign_command::{ContentSignature, SignCommand};
use chainspec::ChainSpec;
use clap::{Args, Subcommand};
use serde::Serialize;
use std::fmt::{Display, Formatter};
use types::cli_profile::profile_management::{
    CliProfileManager, ProfileAttributeOptions, ProtectedProfileManager,
};
use types::cli_profile::socrypto_secret_key_wrapper::SoCryptoSecretKey;
use types::cli_profile::unsafe_protected_profile::UnsafeProfileManager;
use types::settings::node_identity::{NodeIdentity, ValidatorNodePublicIdentity};
use types::supra_network::SupraNetwork;

pub mod migrate;
pub mod sign_command;

#[derive(Serialize)]
pub enum ProfileToolResult {
    Blank,
    CliProfileManager(CliProfileManager),
    SignatureTool(ContentSignature),
}

impl Display for ProfileToolResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileToolResult::Blank => Ok(()),
            ProfileToolResult::CliProfileManager(res) => write!(f, "{res}"),
            ProfileToolResult::SignatureTool(res) => write!(f, "{res:?}"),
        }
    }
}

/// NOTE:
///   - Subcommands with arguments should not provide flags to avoid confusion.
///   - Subcommand with no more than 3 char does not require alias.
#[derive(Subcommand, Clone, Debug, Eq, PartialEq)]
#[clap(
    about = "Manage Supra CLI profile",
    long_about = "Provides means to manage cli profile and validator identity."
)]
pub enum ProfileTool {
    #[clap(
        alias = "ls",
        short_flag = 'l',
        long_flag = "list",
        about = "List public keys of all profiles.",
        long_about = "List public keys of all profiles."
    )]
    List(ListProfileArgs),
    #[clap(
        alias = "act",
        about = "Activate the profile.",
        long_about = "Activate the profile by name, so that following operations are done on this profile."
    )]
    Activate(ActivateProfileArgs),
    #[clap(
        about = "Creates a new profile.",
        long_about = "Create a new profile or import using private key."
    )]
    New(NewProfileArgs),
    #[clap(
        about = "Generate validator identity file.",
        long_about = "Generate validator identity file."
    )]
    GenerateValidatorIdentity(GenerateValidatorIdentityArgs),
    #[clap(
        about = "Remove existing cli profile",
        long_about = "Remove existing cli profile"
    )]
    Remove(RemoveProfile),
    #[clap(
        alias = "mod",
        about = "Modify the profile attributes",
        long_about = "Modify the profile attributes"
    )]
    Modify(ModifyProfileAttribute),
    #[clap(
        about = "Sign the payload",
        long_about = "Sign the payload or verify the signature on payload"
    )]
    Sign(SignCommand),
    Migrate(migrate::Migrate),
    // TODO: keep a better track of which struct is going to get encrypted using this password
    // and then reset password for all struct
    // right now NiDkg reset password is unhandled
    #[clap(about = "Reset cli profile password and validator node identity")]
    ResetPassword,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ListProfileArgs {
    /// Prints the secret keys of the profile along with the public information.
    /// The `NODE_OPERATOR_KEY_PASSWORD` environment variable is ignored when this
    /// option is given.
    #[arg(short, long)]
    pub reveal_secrets: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ActivateProfileArgs {
    /// Name of the profile to activate.
    pub name: String,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct NewProfileArgs {
    /// Name of the profile to generate.
    name: Option<String>,
    /// Private key. Should be provided in hexadecimal format.
    /// It may optionally start with '0x'. Example: "0xabc123" or "abc123".
    private_key: Option<SoCryptoSecretKey>,
    #[clap(flatten)]
    profile_option: ProfileAttributeOptionsMaker,
    #[arg(short, long, help = "Interactively confirms the profile creation.")]
    interactive: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct GenerateValidatorIdentityArgs {
    /// Socket address of the validator node. e.g. <ip_address>:<port>
    #[arg(short, long)]
    pub socket_addr: Option<String>,
    /// DNS name and a port of the validator node. e.g. <name>:<port>
    #[arg(short, long)]
    pub dns: Option<String>,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct RemoveProfile {
    /// List of the profile to be removed.
    pub name: Vec<String>,
}

impl ListProfileArgs {
    /// Lists the public keys and optionally the secret keys if `reveal_secrets` is set to `true`.
    pub fn execute(&self) -> Result<ProfileToolResult, CliError> {
        if self.reveal_secrets {
            eprintln!(
                "WARNING: You have chosen to reveal secret keys. Please be cautious around you!"
            );
        }

        // Try to read a password from the related environment variable, but only if not revealing
        // private information.
        let (protected_profile_manager, _) =
            ProtectedProfileManager::read_or_create(self.reveal_secrets, false)?;

        // If reveal_secrets is set to true, then return the secret key string.
        if self.reveal_secrets {
            // reveal the secret key onto the terminal screen session that goes to stderr instead of cli history writer
            // user is responsible for managing the std error
            let unsafe_profile_manager = UnsafeProfileManager::from(&protected_profile_manager);
            println!("{}", unsafe_profile_manager.unsafe_display_string());
            Ok(ProfileToolResult::Blank)
        } else {
            Ok(ProfileToolResult::CliProfileManager(
                protected_profile_manager.cli_profile_manager(),
            ))
        }
    }
}

impl NewProfileArgs {
    /// Generates supra key with provided name or `Default` and then returns the public key set while storing the [private](SupraPathList::SmrPrivateKeyFile) key.
    pub fn execute(&self) -> Result<ProfileToolResult, CliError> {
        let p_name = self.name.as_deref().unwrap_or("default");

        let protected_profile_manager = ProtectedProfileManager::create_new(
            p_name,
            self.private_key.clone(),
            self.profile_option.clone().make()?,
            false,
            self.interactive,
        )?;
        let cli_profile_manager = protected_profile_manager.cli_profile_manager();
        // Export public profile.
        cli_profile_manager.write_to_default_path()?;
        eprintln!(
            "Exported public profile file at {}",
            CliProfileManager::default_path()?.to_string_lossy()
        );
        Ok(ProfileToolResult::CliProfileManager(
            protected_profile_manager.cli_profile_manager(),
        ))
    }
}

impl GenerateValidatorIdentityArgs {
    /// Generates the validator identity file.
    pub fn execute(&self) -> Result<ProfileToolResult, CliError> {
        // Read profile and password. Use the same password to encrypt the validator identity.
        let (_, password) = ProtectedProfileManager::read_or_create(false, false)?;

        let validator_identity = match (&self.socket_addr, &self.dns) {
            (None, None) => {
                return Err(CliError::Aborted(
                    "Neither socket address nor DNS name was provided".to_owned(),
                    "Please provide either a socket address or a DNS name.".to_owned(),
                ));
            }
            (Some(socket_addr), None) => {
                let socket_addr = socket_addr.parse().map_err(|_e| {
                    CliError::Aborted(
                        format!("Input socket address is invalid: {socket_addr}"),
                        "Please check your input socket address.".to_owned(),
                    )
                })?;
                NodeIdentity::generate_with_ip(Some(socket_addr))
            }
            (None, Some(dns)) => {
                let parts = dns.split(":").take(2).collect::<Vec<_>>();

                if parts.len() != 2 {
                    return Err(CliError::Aborted(
                        "Invalid DNS specification".to_owned(),
                        "Please specify DNS name as NAME:PORT, e.g., example.com:35000".to_owned(),
                    ));
                }

                let name = parts
                    .first()
                    .expect("`parts` has been verified to have 2 elements");

                let port = {
                    let port = parts
                        .get(1)
                        .expect("`parts` has been verified to have 2 elements");

                    port.parse::<u16>().map_err(|_|
                        CliError::Aborted(
                            "Invalid port in DNS name specification".to_owned(),
                            format!("Please provide a valid numeric port (received {port}), e.g., example.com:35000"),
                        ))?
                };

                NodeIdentity::generate_with_name_and_port(name, port)
            }
            (Some(_), Some(_)) => {
                return Err(CliError::Aborted(
                    "Conflicting configuration".to_owned(),
                    "Specify either a new socket address or a DNS name".to_owned(),
                ));
            }
        };

        eprintln!(
            "Generated validator identity file at: {:?}",
            NodeIdentity::default_path()
        );
        validator_identity.to_encrypted_file(&password)?;
        // Export public identity.
        validator_identity
            .to_public_identity()
            .write_to_default_path()?;
        eprintln!(
            "Exported validator public identity file at {:?}",
            ValidatorNodePublicIdentity::default_path()
        );
        Ok(ProfileToolResult::Blank)
    }
}

impl ActivateProfileArgs {
    /// Activates the profile by name.
    pub fn execute(&self) -> Result<ProfileToolResult, CliError> {
        let protected_profile_manager =
            ProtectedProfileManager::activate(self.name.as_str(), false)?;
        let cli_profile_manager = protected_profile_manager.cli_profile_manager();
        // Update the public profile active status.
        cli_profile_manager.write_to_default_path()?;
        Ok(ProfileToolResult::CliProfileManager(cli_profile_manager))
    }
}

impl RemoveProfile {
    pub fn execute(&self) -> Result<ProfileToolResult, CliError> {
        let p_name = self.name.clone();

        let protected_profile_manager = ProtectedProfileManager::remove_keys(&p_name, false)?;
        let cli_profile_manager = protected_profile_manager.cli_profile_manager();
        // Export public profile.
        cli_profile_manager.write_to_default_path()?;
        eprintln!(
            "Exported public profile file at {}",
            CliProfileManager::default_path()?.to_string_lossy()
        );
        Ok(ProfileToolResult::CliProfileManager(
            protected_profile_manager.cli_profile_manager(),
        ))
    }
}

impl Display for ProfileTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl ProfileTool {
    pub async fn execute(self) -> Result<ProfileToolResult, CliError> {
        match self {
            ProfileTool::New(args) => args.execute(),
            ProfileTool::GenerateValidatorIdentity(args) => args.execute(),
            ProfileTool::List(args) => args.execute(),
            ProfileTool::Activate(args) => args.execute(),
            ProfileTool::Remove(args) => args.execute(),
            ProfileTool::Modify(args) => args.execute(),
            ProfileTool::Sign(args) => args.execute(),
            ProfileTool::Migrate(tool) => tool.execute().await,
            ProfileTool::ResetPassword => {
                let (profile_manager, old_password) =
                    ProtectedProfileManager::read_or_create(false, false)?;
                let new_password = profile_manager.reset_password()?;
                println!("Your CLI Profile password has been updated successfully.");
                let node_identity = NodeIdentity::from_encrypted_file(&old_password)?;
                node_identity.to_encrypted_file(&new_password)?;
                println!("Your Node Identity password has been updated successfully.");
                Ok(ProfileToolResult::Blank)
            }
        }
    }
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ModifyProfileAttribute {
    /// Name of the profile.
    name: String,
    #[clap(flatten)]
    profile_option: ProfileAttributeOptionsMaker,
    #[arg(short, long, help = "Interactively confirms the profile creation.")]
    interactive: bool,
}

impl ModifyProfileAttribute {
    pub fn execute(&self) -> Result<ProfileToolResult, CliError> {
        let p_name = self.name.clone();

        let protected_profile_manager = ProtectedProfileManager::modify_profile_attribute(
            &p_name,
            self.profile_option.clone().make()?,
            self.interactive,
            false,
        )?;
        let cli_profile_manager = protected_profile_manager.cli_profile_manager();
        // Export public profile.
        cli_profile_manager.write_to_default_path()?;
        eprintln!(
            "Exported public profile file at {}",
            CliProfileManager::default_path()?.to_string_lossy()
        );
        Ok(ProfileToolResult::CliProfileManager(
            protected_profile_manager.cli_profile_manager(),
        ))
    }
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ProfileAttributeOptionsMaker {
    #[clap(long, conflicts_with_all = &["rpc_url", "faucet_url", "chain_id"])]
    network: Option<SupraNetwork>,
    #[clap(flatten)]
    profile_option: Option<ProfileAttributeOptions>,
}

impl ProfileAttributeOptionsMaker {
    fn make(self) -> Result<ProfileAttributeOptions, CliError> {
        Ok(match (self.network, self.profile_option) {
            (Some(network), None) => ProfileAttributeOptions::new(
                None,
                Some(network.rpc_url()?),
                Some(network.faucet_url()?),
                Some(u8::try_from(ChainSpec::try_from(network)?).map_err(|e| anyhow::anyhow!(e))?),
            ),
            (None, Some(profile_option)) => profile_option,
            (Some(network), Some(profile_option)) => {
                if let Some(address) = profile_option.account_address() {
                    ProfileAttributeOptions::new(
                        Some(*address),
                        Some(network.rpc_url()?),
                        Some(network.faucet_url()?),
                        Some(
                            u8::try_from(ChainSpec::try_from(network)?)
                                .map_err(|e| anyhow::anyhow!(e))?,
                        ),
                    )
                } else {
                    unreachable!("`--network` cannot be used along with `--rpc_url`, `--faucet_url`, `--chain_id`")
                }
            }
            (None, None) => ProfileAttributeOptions::new(None, None, None, None),
        })
    }
}
