use crate::common::error::CliError;
use crate::genesis::error::GenesisToolError;
use aptos_types::transaction::Transaction;
use clap::{Args, Subcommand};
use committees::GenesisStateProvider;
use configurations::supra::SmrSettings;
use fs_extra::{dir, move_items_with_progress};
use generate_genesis_transaction::GenerateGenesisTransaction;
use generate_mainnet_accounts_config::GenerateMainnetAccountsConfig;
use nidkg_helper::errors::DkgError;
use rayon::prelude::*;
use serde::Serialize;
use socrypto::{PublicKey, Signature, PUBLIC_KEY_LENGTH};
use soserde::SmrDeserialize;
use std::collections::{BTreeMap, HashSet};
use std::fmt::{Display, Formatter};
use std::fs::read;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::{fs, io};
use tracing::log::info;
use transactions::SmrDkgCommitteeType;
use types::cli_profile::profile_management::{CliProfileManager, ProtectedProfileManager};
use types::genesis::genesis_config::GenesisConfig;
use types::settings::committee::committee_config::config_v2::{
    CommitteeConfigV2, SupraCommitteesConfigV2,
};
use types::settings::committee::genesis_blob::genesis_blob_v1::GenesisBlobV1;
use types::settings::committee::genesis_blob::GenesisBlob;
use types::settings::committee::genesis_transaction::GenesisTransaction;
use types::settings::constants::GenesisConfigConstants;
use types::settings::node_identity::{NodeIdentity, ValidatorNodePublicIdentity};
use types::validator::SupraValidator;

const RETRY_DOWNLOAD_COUNT: usize = 3;

pub mod error;
pub mod utils;

mod generate_genesis_transaction;
mod generate_mainnet_accounts_config;

#[derive(Serialize)]
pub enum GenesisToolResult {
    Blank,
}

impl Display for GenesisToolResult {
    fn fmt(&self, _f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GenesisToolResult::Blank => Ok(()),
        }
    }
}

#[derive(Subcommand, Clone, Debug, Eq, PartialEq)]
#[clap(
    about = "Perform Genesis Ceremony",
    long_about = "Provides commands to perform Supra's Genesis Ceremony"
)]
pub enum GenesisTool {
    #[clap(
        about = "Refresh the local Supra-NodeOps repository",
        long_about = "Refresh Supra-NodeOps repository for the Genesis Ceremony. \
        Downloads the repository if it is not present locally"
    )]
    RefreshRepo(RefreshRepo),
    #[clap(
        about = "Sign the supra_committees.json file",
        long_about = "Hash the supra_committees.json file and sign using the active keypair."
    )]
    SignSupraCommittee(SignSupraCommittee),
    #[clap(
        about = "Build the supra_committees.json file from given GitHub URL",
        long_about = "Build the supra_committees.json file by downloading the necessary info from the github URL"
    )]
    BuildSupraCommittee(BuildSupraCommittee),
    #[clap(
        about = "Build the genesis.blob file from the optional GitHub URL or from disk",
        long_about = "Build the genesis.blob file by downloading the necessary info from the GitHub URL and validating the signatures.\
        If GitHub "
    )]
    GenerateGenesisBlob(GenerateGenesisBlob),
    #[clap(
        about = "Generate genesis.blob file from configs and compare it with an externally sourced genesis.blob",
        long_about = "Generate genesis.blob file from the configurations present at SUPRA_HOME and assert that the genesis.blob file located at the given path has the \
         same hash as the one built locally."
    )]
    VerifyGenesisBlob(VerifyGenesisBlob),
    #[clap(
        about = "Generates account-related configuration files for mainnet.",
        long_about = "Provides a simplified interface for generating accounts_to_pools.json, foundation_multisig_accounts.json and genesis_configs.json for Supra mainnet."
    )]
    GenerateMainnetAccountsConfig(GenerateMainnetAccountsConfig),
    #[clap(about = "Generates the genesis Move transaction from the genesis configuration file.")]
    GenerateGenesisTransaction(GenerateGenesisTransaction),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct BuildSupraCommittee {
    /// URL to pull the GitHub repo.
    /// Example URL: https://github.com/Entropy-Foundation/supra-nodeops-data/archive/refs/heads/master.zip
    /// If URL is not passed, `SUPRA_HOME/extracted/operators` path is directly read for the key files
    #[arg(long)]
    url: Option<String>,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct SignSupraCommittee;

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct RefreshRepo {
    /// URL of the repository to refresh.
    /// Example URL: https://github.com/Entropy-Foundation/supra-nodeops-data/archive/refs/heads/master.zip
    #[arg(long)]
    url: String,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct GenerateGenesisBlob {
    /// The path to the binary-encoded genesis [Transaction], if any. If no path is provided
    /// the transaction will be generated from the local [GenesisConfig].
    #[arg(long)]
    genesis_transaction_path: Option<String>,
    /// Path to the directory containing the genesis signatures
    #[arg(short, long)]
    genesis_signatures_dir: Option<PathBuf>,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct VerifyGenesisBlob {
    /// Path to the input genesis.blob file.
    #[arg(short, long)]
    input_genesis_blob_path: String,
    /// Compare the input blob with an already existing local blob file(at SUPRA_HOME), rather than
    /// generating it from scratch.
    #[arg(short, long)]
    skip_regeneration: bool,
    /// Path to the directory containing the genesis signatures
    #[arg(short, long)]
    genesis_signatures_dir: Option<PathBuf>,
}

impl BuildSupraCommittee {
    /// Step 2 of the Genesis Ceremony. Once all the Validators have uploaded
    /// their public keys and public identities to the designated GitHub repository,
    /// `BuildSupraCommittee` command is invoked from the CLI which collects all the fore-mentioned
    /// public details of the Validators to build the `supra_committees.json` file
    pub async fn execute(&self) -> Result<GenesisToolResult, CliError> {
        let smr_settings = SmrSettings::read_from_default_path()?;
        let is_testnet = *smr_settings.instance().is_testnet();
        if let Some(url) = &self.url {
            download_directory(
                url,
                &GenesisConfigConstants::github_repo_current_release_path(is_testnet)
                    .display()
                    .to_string(),
                &GenesisConfigConstants::extract_path()?
                    .display()
                    .to_string(),
            )
            .await?;
        }

        smr_settings.validate().map_err(CliError::GeneralError)?;
        let mut config = CommitteeConfigV2::new(
            SmrDkgCommitteeType::Smr,
            smr_settings.into(),
            BTreeMap::new(),
        );
        let members = config.members_mut();

        match fs::read_dir(GenesisConfigConstants::github_operators_path(is_testnet)?) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let metadata = entry.metadata()?;
                    if metadata.is_dir() {
                        println!("Processing {}", entry.path().display());
                        let active_profile_manager =
                            CliProfileManager::read_from_dir(entry.path())?;
                        let cli_profile = active_profile_manager.active_cli_profile_ref()?;
                        let validator_public_identity =
                            ValidatorNodePublicIdentity::read_from_dir(entry.path())?;

                        let new_validator = SupraValidator::new(
                            *cli_profile.profile_attributes().account_address(),
                            validator_public_identity.clone(),
                        );

                        if members
                            .insert(new_validator.identity(), new_validator)
                            .is_some()
                        {
                            panic!("Duplicate member found while building SupraCommitteesConfig {validator_public_identity}");
                        }
                    }
                }
            }
            Err(err) => println!("Error reading operators directory: {err}"),
        }

        // Validate the CGPublicKeys of all the peers parallely
        let validation_res = config
            .members()
            .into_par_iter()
            .map(|(_, member)| {
                (
                    member.node_public_identity.address.clone(),
                    member.node_public_identity.dkg_cg_public_key.validate(),
                )
            })
            .collect::<Vec<(_, Result<(), DkgError>)>>();

        if validation_res.iter().all(|res| res.1.is_ok()) {
            // Write to disk
            let supra_committee = SupraCommitteesConfigV2::new(vec![config]);
            supra_committee.write_to_default_path()?;
        } else {
            // Print the SocketAddress of the peer that failed CG-DKG key validation
            // SocketAddress is chosen as it is the most human-readable identifier than CG-DKG Key
            for peer in validation_res.iter().filter(|res| res.1.is_err()) {
                eprintln!("Peer {:?} failed CG-DKG key validation", peer.0)
            }

            return Err(CliError::GenesisToolError(GenesisToolError::Key(
                "CG-DKG PublicKey Validation failed".to_string(),
            )));
        }

        Ok(GenesisToolResult::Blank)
    }
}

impl SignSupraCommittee {
    /// Step 3 of Genesis Ceremony. The `supra_committees.json` file built in Step 2 & the Genesis
    /// configurations file that are to be pulled from Github are collectively signed in with the
    /// Consensus Secret Key of the Validator. Every validator will generate a signature file that
    /// will be uploaded to the GitHub repository after completion of Step 3.
    pub fn execute(&self) -> Result<GenesisToolResult, CliError> {
        let (_, password) = ProtectedProfileManager::read_or_create(false, false)?;
        let validator_info = NodeIdentity::from_encrypted_file(&password)?;
        let output_path =
            GenesisConfigConstants::signature_output_path(validator_info.network.address())?;

        // Create and sign the [GenesisConfig].
        let genesis_config = GenesisConfig::try_from_supra_home()?;
        let signature = genesis_config.sign(&validator_info);

        // Attach Node PublicKey with signature
        let flattened_signature = signature.to_bytes().to_vec();
        let mut pk_and_signature = validator_info.consensus.public_key().to_bytes().to_vec();
        pk_and_signature.extend_from_slice(&flattened_signature);

        // Write Signature to a file
        let mut signature_file = File::create(&output_path)?;
        let bytes_written = signature_file.write(&pk_and_signature)?;
        eprintln!("Signature stored to disk: {bytes_written:?} bytes written at {output_path:?}");

        Ok(GenesisToolResult::Blank)
    }
}

impl RefreshRepo {
    /// Utility command. Executing RefreshRepo command pulls the latest state of the
    /// provided GitHub repository. To be used after Step 1 to pull all the public details
    /// and Step 3 to pull all the Validator's signatures
    pub async fn execute(&self) -> Result<GenesisToolResult, CliError> {
        let smr_settings = SmrSettings::read_from_default_path()?;
        let is_testnet = *smr_settings.instance().is_testnet();
        download_directory(
            &self.url,
            &GenesisConfigConstants::github_repo_current_release_path(is_testnet)
                .display()
                .to_string(),
            &GenesisConfigConstants::extract_path()?
                .display()
                .to_string(),
        )
        .await?;

        Ok(GenesisToolResult::Blank)
    }
}

impl GenerateGenesisBlob {
    /// Step 4 of Genesis Ceremony. The following process happens when executing GenerateGenesisBlob command:
    /// 1. The local `supra_committees.json` file is loaded onto memory
    /// 2. The signatures of all the validators present in the [GenesisConfigConstants::github_signatures_path()]
    ///     directory is read and verified against the `supra_committees.json` file. This is a strict
    ///     n-of-n verification, meaning all the signatures submitted by the validators must be successfully
    ///     verified against their public keys. This gives us a guarantee that the validators have all agreed
    ///     to start with the same set of [GenesisParameters] and the same [CommitteeConfigV2].
    /// 3. Load the [GenesisAccounts] files from disk for generating the genesis transactions
    /// 4. Differentiation between Testnet and Mainnet Genesis transactions is picked from the `GenesisParameters`.
    ///     [ChainInstanceParameters::is_testnet()] denotes the operation mode of the network.
    /// 5. Based on the network operation mode, Genesis Transactions are created and written to the
    ///     `genesis.blob` file.
    pub fn execute(&self) -> Result<GenesisToolResult, CliError> {
        let genesis_blob = generate_genesis_blob_from_supra_home(
            &self.genesis_transaction_path,
            &self.genesis_signatures_dir,
        )?;

        // Write the file to disk
        genesis_blob.to_file(None)?;

        eprintln!(
            "'genesis.blob' file written to disk at {:?}",
            GenesisBlobV1::default_path()
        );

        Ok(GenesisToolResult::Blank)
    }
}

impl VerifyGenesisBlob {
    pub fn execute(&self) -> Result<GenesisToolResult, CliError> {
        let input_genesis_blob =
            GenesisBlob::try_from_file(Some(PathBuf::from(&self.input_genesis_blob_path)))?;

        // Read an already existing blob if true else generate a blob from the existing config files
        let local_genesis_blob = if self.skip_regeneration {
            GenesisBlob::try_from_file(None)?
        } else {
            generate_genesis_blob_from_supra_home(&None, &self.genesis_signatures_dir)?
        };

        println!(
            "Expected config digest: {:?}",
            local_genesis_blob.config_digest()
        );
        println!(
            "Input config digest: {:?}",
            input_genesis_blob.config_digest()
        );
        println!(
            "Expected GenesisTransactions digest: {:?}",
            local_genesis_blob.transaction_digest()
        );
        println!(
            "Input GenesisTransactions digest: {:?}",
            input_genesis_blob.transaction_digest()
        );

        // Compare the hashes of the configs from the blob and the VM events from the Genesis
        // Transactions. The Transaction hash itself might not be same because the supra framework
        // in the ChangeSet may be different.
        if local_genesis_blob.config_digest() != input_genesis_blob.config_digest()
            || local_genesis_blob.transaction_digest() != input_genesis_blob.transaction_digest()
        {
            // Write the generated file to disk. Prefixes the hash of transactions as the file name
            if !self.skip_regeneration {
                local_genesis_blob
                    .clone()
                    .to_file(Some(PathBuf::from(format!(
                        "{}_genesis.blob",
                        &local_genesis_blob.config_digest()
                    ))))?;
            }

            return Err(CliError::GenesisToolError(
                GenesisToolError::InvalidInputGenesisBlob,
            ));
        }

        info!("Verification successful: Input Genesis Blob matches the locally generated Genesis Blob");
        eprintln!("Verification successful: Input Genesis Blob matches the locally generated Genesis Blob");

        Ok(GenesisToolResult::Blank)
    }
}

fn generate_genesis_blob_from_supra_home(
    maybe_genesis_transaction_path: &Option<String>,
    maybe_genesis_signatures_path: &Option<PathBuf>,
) -> Result<GenesisBlob, CliError> {
    // Load the [GenesisConfig].
    println!("Loading GenesisConfig...");
    let config = GenesisConfig::try_from_supra_home()?;
    let genesis_config_digest = config.digest();
    // Panic if there is no SMR committee. It must exist because the SMR is the fundamental
    // service in our system.

    let committee_config = config.smr_committee_config()?;
    let members = committee_config.members();
    let is_testnet = *committee_config
        .genesis_parameters()
        .instance()
        .is_testnet();

    // Collect target PublicKey (Consensus PublicKey)
    let mut all_consensus_pks = members
        .values()
        .map(|v| v.node_public_identity.consensus_public_key)
        .collect::<HashSet<PublicKey>>();

    let signatures_path = maybe_genesis_signatures_path
        .clone()
        .unwrap_or(GenesisConfigConstants::github_signatures_path(is_testnet)?);

    // Collect signatures to be included in the genesis.blob file
    let mut all_signatures_and_signers = HashSet::new();
    let mut signers_with_invalid_signatures = Vec::new();

    println!("Verifying Signatures from path {signatures_path:?} ...");
    match fs::read_dir(signatures_path) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let metadata = entry.metadata()?;
                if metadata.is_file()
                    && entry
                        .file_name()
                        .to_string_lossy()
                        .contains(GenesisConfigConstants::GENESIS_SIGNATURE_FILE_NAME_SUFFIX)
                {
                    println!(
                        "Verifying Signature from path {} ...",
                        entry.path().display()
                    );
                    // Load PublicKey and Signature from the signature file
                    let pk_and_signature = fs::read(entry.path())?;

                    // Split the Consensus PublicKey and the Signature
                    let (pub_key_bytes, signature_bytes) =
                        pk_and_signature.split_at(PUBLIC_KEY_LENGTH);

                    // Deserialize the components
                    let operator_consensus_pub_key = PublicKey::try_from(pub_key_bytes)?;
                    let signature = Signature::try_from(signature_bytes)?;

                    // Verify the signature and record the signer if it is invalid.
                    if signature
                        .verify(&genesis_config_digest, &operator_consensus_pub_key)
                        .is_err()
                    {
                        signers_with_invalid_signatures.push(operator_consensus_pub_key);
                    }

                    // Check if the operator is one of us
                    if all_consensus_pks.contains(&operator_consensus_pub_key) {
                        // Pop the PublicKey once detected
                        all_consensus_pks.remove(&operator_consensus_pub_key);

                        if !all_signatures_and_signers
                            .insert((operator_consensus_pub_key, signature))
                        {
                            panic!("Duplicate signature entry encountered!");
                        }
                    } else {
                        panic!(
                            "The node operator in directory {:?} has presented unknown PublicKeys",
                            entry.path().display()
                        )
                    }
                }
            }
        }
        Err(err) => return Err(CliError::GenesisToolError(GenesisToolError::IO(err))),
    }

    if !all_consensus_pks.is_empty() {
        return Err(CliError::GenesisToolError(
            GenesisToolError::SignatureError(
                "Not all validators have signed the committee file".to_string(),
            ),
        ));
    }

    if !signers_with_invalid_signatures.is_empty() {
        return Err(CliError::GenesisToolError(
            GenesisToolError::SignatureError(format!(
                "Found invalid signatures from: {signers_with_invalid_signatures:?}"
            )),
        ));
    }

    let blob = if let Some(path) = &maybe_genesis_transaction_path {
        let transaction_bytes = read(path)?;
        let unverified_tx = match config {
            GenesisConfig::V0(_) => {
                let Ok(unverified_transaction) =
                    aptos_types_testnet::transaction::Transaction::try_from_bytes(
                        &transaction_bytes,
                    )
                else {
                    return Err(CliError::GenesisToolError(
                        GenesisToolError::DeserializationFailed(
                            "a genesis transaction".to_string(),
                            path.to_string(),
                        ),
                    ));
                };
                GenesisTransaction::V0(unverified_transaction)
            }
            GenesisConfig::V1(_) | GenesisConfig::V2(_) => {
                let Ok(unverified_transaction) = Transaction::try_from_bytes(&transaction_bytes)
                else {
                    return Err(CliError::GenesisToolError(
                        GenesisToolError::DeserializationFailed(
                            "a genesis transaction".to_string(),
                            path.to_string(),
                        ),
                    ));
                };
                GenesisTransaction::V1(unverified_transaction)
            }
        };

        let blob = GenesisBlob::new_with_unverified_transaction(
            config,
            all_signatures_and_signers,
            unverified_tx,
        )?;
        // Print a warning to notify the user that the transaction was not verified. This should
        // help to protect against accidental invocations.
        eprintln!(
            "Warning: The correlation between the transaction in this genesis blob and the \
            genesis config has not been verified. You can verify this by regenerating the \
            genesis blob from the same config without providing a transaction input file, then \
            confirming that the hash digests of both blobs match."
        );
        blob
    } else {
        println!("Warning: This operation may take several hours to complete if the genesis config is large (e.g., like the mainnet config).");
        println!("Beginning Genesis Blob creation...");
        // Generate the [GenesisBlob] and write it to disk.
        GenesisBlob::new(config, all_signatures_and_signers)?
    };

    Ok(blob)
}

async fn download_directory(
    repo_url: &str,
    target_directory: &str,
    download_dir: &str,
) -> Result<(), GenesisToolError> {
    println!("Pulling the repository and writing to disk...");
    // Build the download URL based on the repository URL format (assuming zip archive)
    let url = reqwest::Url::parse(repo_url).map_err(|e| GenesisToolError::URL(e.to_string()))?;

    // Download the archive
    let client = reqwest::Client::new();
    let mut response = client.get(url.clone()).send().await?;
    let mut retry_count = RETRY_DOWNLOAD_COUNT;

    while retry_count > 0 && !response.status().is_success() {
        // Retry
        response = client.get(url.clone()).send().await?;
        retry_count -= 1;
    }

    // Early return
    if !response.status().is_success() {
        return Err(GenesisToolError::IO(io::Error::other(format!(
            "Failed to download: {}",
            response.status()
        ))));
    }

    // Create a temporary directory
    let temp_dir = tempfile::TempDir::new()?;
    let extract_path = temp_dir.path().join("archive.zip");

    // Write the downloaded data to a temporary file
    let mut file = File::create(&extract_path)?;
    let data = response.bytes().await?;
    file.write_all(&data)?;

    // Extract the archive
    let archive = File::open(&extract_path)?;

    println!("Unzipping the repository...");
    zip::read::ZipArchive::new(archive)
        .expect("failed to create zip archive")
        .extract(temp_dir.path().join("extracted"))
        .expect("failed to"); // Panic if we are unable to unzip the file

    // Copy the desired directory to the final destination
    let source_dir = temp_dir.path().join("extracted").join(target_directory);

    if !source_dir.exists() {
        return Err(GenesisToolError::IO(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Keys directory not found in archive: {}", target_directory),
        )));
    }

    // Creating and writing to destination directory
    match fs::create_dir(download_dir).map_err(|e| e.kind()) {
        Ok(()) | Err(ErrorKind::AlreadyExists) => (),
        Err(e) => {
            return Err(GenesisToolError::Download(format!(
                "Error refreshing repository {e:?}"
            )))
        }
    }
    println!("Moving files from temporary dir {source_dir:?} to {download_dir:?}....");
    move_items_with_progress(
        &[source_dir],
        download_dir,
        &dir::CopyOptions::new().overwrite(true), // Overwrite to update old files
        |_process_info: fs_extra::TransitProcess| dir::TransitProcessResult::ContinueOrAbort,
    )
    .map_err(|e| GenesisToolError::IO(io::Error::other(e)))?;

    Ok(())
}

impl Display for GenesisTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl GenesisTool {
    pub async fn execute(self) -> Result<GenesisToolResult, CliError> {
        match self {
            GenesisTool::RefreshRepo(args) => args.execute().await,
            GenesisTool::BuildSupraCommittee(args) => args.execute().await,
            GenesisTool::SignSupraCommittee(args) => args.execute(),
            GenesisTool::GenerateGenesisBlob(args) => args.execute(),
            GenesisTool::VerifyGenesisBlob(args) => args.execute(),
            GenesisTool::GenerateMainnetAccountsConfig(args) => args.execute(),
            GenesisTool::GenerateGenesisTransaction(args) => args.execute(),
        }
    }
}
