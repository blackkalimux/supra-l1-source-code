use anyhow::{Error, Result};
use configurations::supra::SmrSettings;
use encryption::prompt_password;
use node::run_smr::run_validator_node;
use smrcommon::remove_and_backup_directory;
use supra_logger::LogFilterReload;
use tracing::info;
use types::cli_profile::profile_management::ProtectedProfileManager;
use types::settings::committee::genesis_blob::GenesisBlob;
use types::settings::node_identity::{
    ConstIdentityProvider, FileNodeIdentityProvider, NodeIdentityProvider,
};

// Load all external configuration parameters and start the validator node.
pub async fn start_validator_node(
    resume: Option<bool>,
    disable_dynamic_identity_loading: bool,
    log_filter_reload: Option<LogFilterReload>,
) -> Result<()> {
    let mut smr_settings = SmrSettings::read_from_default_path()?;
    smr_settings.validate_no_p2p_auth().map_err(Error::msg)?;

    // with default flow password from env is not ignored here.
    let password = match ProtectedProfileManager::read_password_from_env() {
        None => prompt_password("Enter your password: ")?,
        Some(p) => p,
    };

    // Load genesis.blob
    let genesis_blob = GenesisBlob::try_from_file(None)?;

    if let Some(resume) = resume {
        eprintln!(
            "Overriding resume flag from: {} to: {}",
            smr_settings.node().resume(),
            resume
        );
        *smr_settings.node_mut().resume_mut() = resume;
    }
    info!("Active SMR settings: {:?}", smr_settings);

    #[cfg(all(feature = "memory_profile", target_os = "linux"))]
    if let Some(memory_profile) = &smr_settings.memory_profile() {
        match memory_profile {
            configurations::supra::smr_node::TargetOs::Linux => {
                crate::node::smr_node::memory_profile::linux_memory_profile();
            }
        }
    }

    // if resume is false then start the SMR node with clean state
    if !smr_settings.node().resume() {
        // As long as smr_settings has been validated at the top unchecked operations are expected to be successful.
        remove_and_backup_directory(
            smr_settings
                .node()
                .chain_store_unchecked()
                .path()
                .expect("missing chain_store path"),
        )?;
        remove_and_backup_directory(
            smr_settings
                .node()
                .ledger_unchecked()
                .path()
                .expect("missing ledger path"),
        )?;
    }

    if disable_dynamic_identity_loading {
        run(
            genesis_blob,
            smr_settings,
            ConstIdentityProvider::initialize_from_file(&password)?,
            log_filter_reload,
        )
        .await
    } else {
        run(
            genesis_blob,
            smr_settings,
            FileNodeIdentityProvider::new(&password)?,
            log_filter_reload,
        )
        .await
    }
}

pub async fn run<IP>(
    genesis_blob: GenesisBlob,
    settings: SmrSettings,
    validator_identity_provider: IP,
    log_filter_reload: Option<LogFilterReload>,
) -> Result<()>
where
    IP: NodeIdentityProvider + Send + 'static,
{
    let validator_identity = validator_identity_provider.load()?;

    info!(
        "Validator node public identity: {}",
        validator_identity.to_public_identity()
    );

    // Already verified the blob so will never panic.
    let genesis_supra_committee = genesis_blob.config().expect_smr_committee_config();
    info!(
        "Supra committee configuration: {:?}",
        genesis_supra_committee
    );

    run_validator_node(
        genesis_blob,
        settings,
        validator_identity_provider,
        log_filter_reload,
    )
    .await
}
