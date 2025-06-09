use super::error::IdentityRotationError;
use crate::common::error::CliError;
use crate::node::identity_rotation::IdentityRotationResult;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use crate::utils::parse_pool_address;
use aptos_cached_packages::aptos_stdlib;
use aptos_crypto::ed25519::Ed25519PublicKey;
use clap::Args;
use types::cli_profile::profile_management::TransactionType;
use types::settings::node_identity::{
    ConsensusIdentity, NodeIdentity, ValidatorNodePublicIdentity,
};

#[derive(Args, Debug)]
pub struct RotateConsensusKey {
    #[clap(flatten)]
    transaction_option: SupraTransactionOptions,
}

impl RotateConsensusKey {
    pub async fn execute(self) -> Result<IdentityRotationResult, CliError> {
        let signer_profile_helper = self
            .transaction_option
            .signer_profile_helper(TransactionType::Execute, false)?;

        let pool_address =
            parse_pool_address(self.transaction_option.delegation_pool_address.as_deref())?;

        let mut validator_identity =
            NodeIdentity::from_encrypted_file(signer_profile_helper.cli_password())?;

        let new_consensus_identity = ConsensusIdentity::generate();

        let new_key =
            Ed25519PublicKey::try_from(new_consensus_identity.public_key().to_bytes().as_slice())
                .map_err(|e| IdentityRotationError::ConsensusKey(format!("{e:?}")))?;

        let transaction_payload =
            aptos_stdlib::stake_rotate_consensus_key(pool_address, new_key.to_bytes().to_vec());

        let txn_info =
            MoveTransactionOptions::new(transaction_payload.clone(), self.transaction_option)
                .send_transaction(ResolveTransactionHash::UntilSuccess)
                .await?;

        // Backup the current identity file and update the new consensus identity
        let backup_file = NodeIdentity::backup()?;
        eprintln!(
            "Backup validator identity file at: {}",
            backup_file.display()
        );
        validator_identity
            .set_new_consensus_identity(new_consensus_identity)
            .to_encrypted_file(signer_profile_helper.cli_password())?;
        validator_identity
            .to_public_identity()
            .write_to_default_path()?;
        eprintln!(
            "Exported validator public identity file at {:?}",
            ValidatorNodePublicIdentity::default_path()
        );
        Ok(IdentityRotationResult::RotateConsensusKey(txn_info))
    }
}
