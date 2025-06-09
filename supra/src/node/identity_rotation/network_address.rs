use super::IdentityRotationResult;
use crate::common::error::CliError;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use crate::utils::parse_pool_address;
use aptos_cached_packages::aptos_stdlib;
use clap::Args;
use std::net::SocketAddr;
use types::cli_profile::profile_management::TransactionType;
use types::settings::node_identity::{NetworkIdentity, NodeIdentity, ValidatorNodePublicIdentity};

#[derive(Args, Debug)]
pub struct RotateNetworkAddress {
    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,
    /// Network socket address to use. Take effect in next epoch.
    #[arg(short, long)]
    pub socket_addr: Option<String>,
    /// Dns name with port, e.g., `example.com:35000`. Take effect in next epoch.
    #[arg(short, long)]
    pub dns: Option<String>,
    /// Whether to generate a new key pair. Take effect in next epoch.
    #[arg(short, long)]
    pub new_key: bool,
}

impl RotateNetworkAddress {
    pub async fn execute(self) -> Result<IdentityRotationResult, CliError> {
        let signer_profile_helper = self
            .transaction_option
            .signer_profile_helper(TransactionType::Execute, false)?;
        let mut validator_identity =
            NodeIdentity::from_encrypted_file(signer_profile_helper.cli_password())?;

        let pool_address =
            parse_pool_address(self.transaction_option.delegation_pool_address.as_deref())?;

        if self.new_key {
            validator_identity
                .set_new_network_key(NetworkIdentity::generate(None).secret_key().clone());
        }

        // Construct new network address and update network identity, but do not write to file yet.
        match (&self.socket_addr, &self.dns) {
            (None, None) => {
                if !self.new_key {
                    return Err(CliError::Aborted(
                        "Nothing to rotate".to_owned(),
                        "At least one of a new ip address or a dns name or request to generate a new key must be issued.".to_owned(),
                    ));
                }
            }
            (Some(_), Some(_)) => {
                return Err(CliError::Aborted(
                    "Conflicting configuration".to_owned(),
                    "Specify either a new socket address or a DNS name".to_owned(),
                ));
            }
            (Some(socket_addr), None) => {
                let Ok(socket_addr) = socket_addr.parse::<SocketAddr>() else {
                    return Err(CliError::Aborted(
                        "Invalid socket address".to_owned(),
                        "Please provide a valid socket address".to_owned(),
                    ));
                };
                validator_identity.set_new_network_socket_addr(socket_addr);
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

                validator_identity.set_new_network_dns_name_and_port(name, port);
            }
        };

        let new_network_addr = validator_identity.network.to_network_address();

        let transaction_payload = aptos_stdlib::stake_update_network_and_fullnode_addresses(
            pool_address,
            new_network_addr.encode()?,
            Vec::new(), // Unused fullnode addresses
        );

        let txn_info =
            MoveTransactionOptions::new(transaction_payload.clone(), self.transaction_option)
                .send_transaction(ResolveTransactionHash::UntilSuccess)
                .await?;

        // Proceed further if only the tx gets finalized with `Success` status.

        // Backup the current identity file
        let backup_path = NodeIdentity::backup()?;
        eprintln!(
            "Backup validator identity file at: {}",
            backup_path.display()
        );
        validator_identity.to_encrypted_file(signer_profile_helper.cli_password())?;
        validator_identity
            .to_public_identity()
            .write_to_default_path()?;
        eprintln!(
            "Exported validator public identity file at {:?}",
            ValidatorNodePublicIdentity::default_path()
        );

        Ok(IdentityRotationResult::RotateNetworkAddress(txn_info))
    }
}
