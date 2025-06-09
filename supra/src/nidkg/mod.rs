use crate::common::error::CliError;
use crate::nidkg::tool::{
    generate_cg_encryption_key_pair, generate_committee_key, generate_dealing, generate_key,
    generate_reshare_key, generate_resharing_dealing, print_bls_pub_key,
};
use crate::nidkg::DkgToolResult::{
    CGEncKeyPairResult, CommitteeKeyGenResult, DealingResult, NodeKeyGenResult,
    OutputThresholdBlsPubKey,
};
use clap::Subcommand;
use serde::Serialize;
use std::fmt::{Display, Formatter};

pub mod tool;

#[derive(Debug, Serialize)]
pub enum DkgToolResult {
    CGEncKeyPairResult(String),
    DealingResult(String),
    NodeKeyGenResult(String),
    CommitteeKeyGenResult(String),
    OutputThresholdBlsPubKey,
}

impl Display for DkgToolResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CGEncKeyPairResult(res) => writeln!(f, "CGEncKeyPairResult({})", res),
            DealingResult(res) => writeln!(f, "DealingResult({})", res),
            NodeKeyGenResult(res) => writeln!(f, "NodeKeyGenResult({})", res),
            CommitteeKeyGenResult(res) => writeln!(f, "CommitteeKeyGenResult({})", res),
            OutputThresholdBlsPubKey => writeln!(f),
        }
    }
}

#[derive(Subcommand, Clone, Debug, Eq, PartialEq)]
#[clap(
    alias = "d",
    about = "Manage NiDKG dealings",
    long_about = "Provides means to generate a new/resharing dealing, combine dealings to form a transcript, and generate bls12381 and bn254 signing keys."
)]
pub enum DKGTool {
    #[clap(
        short_flag = 'c',
        long_flag = "gen-cg-enc-keypair",
        about = "Generates centralized classgroup encryption/decryption key pair.",
        long_about = "Generates centralized classgroup encryption/decryption key pair."
    )]
    GenerateCGKeyPair,
    #[clap(
        short_flag = 'd',
        long_flag = "create-dealing",
        about = "Creates a dealing.",
        long_about = "Creates a dealing."
    )]
    GenerateDealing,
    #[clap(
        short_flag = 'r',
        long_flag = "create-reshare-dealing",
        about = "Creates a reshare dealing.",
        long_about = "Creates a reshare dealing using an existing BLS Private Key"
    )]
    GenerateReshareDealing,
    #[clap(
        short_flag = 'k',
        long_flag = "compute-key",
        about = "Compute node bls12381 and bn254 signing keys.",
        long_about = "Compute node bls12381 and bn254 signing keys."
    )]
    ComputeBLSKey,
    #[clap(
        short_flag = 'l',
        long_flag = "compute-key-reshared",
        about = "Compute node bls12381 and bn254 signing keys from reshared dealings.",
        long_about = "Compute node bls12381 and bn254 signing keys from reshared dealings."
    )]
    ComputeBLSKeyReshared,
    #[clap(
        short_flag = 'p',
        long_flag = "compute-bls-committee-key",
        about = "Compute committee bls12381 and bn254 signing key.",
        long_about = "Compute committee bls12381 and bn254 signing key."
    )]
    ComputeCommitteeBLSKey,
    #[clap(
        short_flag = 'b',
        long_flag = "output-threshold-bls-key",
        about = "Print threshold bls12381 and bn254 public keys.",
        long_about = "Print threshold bls12381 and bn254 public keys."
    )]
    OutputBLSPubKey,
}

impl Display for DKGTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl DKGTool {
    pub async fn execute(self) -> Result<DkgToolResult, CliError> {
        match self {
            DKGTool::GenerateCGKeyPair => {
                generate_cg_encryption_key_pair()?;
                Ok(CGEncKeyPairResult(
                    "CG encryption keypair generated".to_owned(),
                ))
            }
            DKGTool::GenerateDealing => {
                generate_dealing()?;
                Ok(DealingResult("Dealing generated".to_owned()))
            }
            DKGTool::GenerateReshareDealing => {
                generate_resharing_dealing()?;
                Ok(DealingResult("Dealing generated".to_owned()))
            }

            DKGTool::ComputeBLSKey => {
                generate_key()?;
                Ok(NodeKeyGenResult("BLS Keypair generated".to_owned()))
            }
            DKGTool::ComputeCommitteeBLSKey => {
                generate_committee_key()?;
                Ok(CommitteeKeyGenResult(
                    "Committee BLS Public Key generated".to_owned(),
                ))
            }
            DKGTool::ComputeBLSKeyReshared => {
                generate_reshare_key()?;
                Ok(NodeKeyGenResult("Key generated".to_owned()))
            }
            DKGTool::OutputBLSPubKey => {
                print_bls_pub_key()?;
                Ok(OutputThresholdBlsPubKey)
            }
        }
    }
}
