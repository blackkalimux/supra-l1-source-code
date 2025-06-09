use crate::common::error::CliError;
use crate::node::identity_rotation::consensus_key::RotateConsensusKey;
use crate::node::identity_rotation::network_address::RotateNetworkAddress;
use crate::rest_utils::action::CliTransactionInfo;
use clap::Subcommand;
use serde::Serialize;
use std::fmt::{Display, Formatter};

pub mod consensus_key;
pub mod error;
pub mod network_address;

#[derive(Serialize)]
pub enum IdentityRotationResult {
    RotateConsensusKey(CliTransactionInfo),
    RotateNetworkAddress(CliTransactionInfo),
}

impl Display for IdentityRotationResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityRotationResult::RotateConsensusKey(res) => write!(f, "{res:?}"),
            IdentityRotationResult::RotateNetworkAddress(res) => write!(f, "{res:?}"),
        }
    }
}

#[derive(Subcommand, Debug)]
#[clap(
    about = "Identity rotation operations.",
    long_about = "Provides means to rotate validator identity, such as consensus key, Supra network address."
)]
pub enum IdentityRotationTool {
    /// Generate new consensus key and apply transaction on chain. Take effect in next epoch.
    RotateConsensusKey(RotateConsensusKey),
    /// Generate new network address and apply transaction on chain. Take effect in next epoch.
    RotateNetworkAddress(RotateNetworkAddress),
}

impl IdentityRotationTool {
    pub async fn execute(self) -> Result<IdentityRotationResult, CliError> {
        match self {
            IdentityRotationTool::RotateConsensusKey(args) => args.execute().await,
            IdentityRotationTool::RotateNetworkAddress(args) => args.execute().await,
        }
    }
}
