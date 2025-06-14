use crate::consensus::{
    AssemblerError, BlockStore, BlockVerifier, ChainStateAssembler, ChainStateAssemblerDataInputs,
    TBlockStore,
};
use crate::error::Error;
use crate::gas::gas_monitor;
use crate::rest::api::v1::transactions::__path_submit_txn;
use crate::rest::api::v2::block::__path_latest_block;
use crate::rest::api::v2::consensus::{
    __path_committee_authorization, __path_consensus_block, __path_latest_consensus_block,
};
use crate::rest::api_server_state::FaucetState;
use crate::rest::api_server_state_builder::ApiServerStateBuilder;
use crate::rest::faucet::Faucet;
use crate::rest::faucet_call_limiter::FaucetCallLimiter;
use crate::rest::router::{default_route, rest_root_route};
use crate::rpc_epoch_manager_middleware::RpcEpochManagerMiddleware;
use crate::transactions::dispatcher::TransactionDispatcher;
use crate::{error, rest, RpcStorage};
use crate::{RpcEpochChangeNotificationSchedule, RpcEpochManager};
use anyhow::anyhow;
use archive::builder::ArchiveDbBuilder;
use archive::db::ArchiveDb;
use committees::CommitteeAuthorization;
use configurations::databases::StorageInstance;
use configurations::rpc::node::{RpcConfig, RPC_CONFIG_PATH};
use configurations::rpc::sync_config::{
    RpcRestSync, RpcSync, RpcWsSync, RPC_TRANSACTION_FORWARD_CHANNEL_SIZE,
};
use epoch_manager::EpochStatesProvider;
use execution::{Executor, Ledger, MoveExecutor, MoveStore};
use mempool::MoveSequenceNumberProvider;
use network_tls::tls_connector;
use node::utils::reset_db_committee;
use ntex::web;
use ntex::web::middleware;
use ntex_files::Files;
use pruner::PrunerTask;
use rocksstore::chain_storage::block_store::ChainStore;
use rocksstore::chain_storage::ChainStoreBuilder;
use rpc::clients::{RestConnectionSetupConfiguration, RpcClient, WsConnectionSetupConfiguration};
use rpc::messages::RpcBlockData;
use rpc::RPC_INTERNAL_CHANNEL_SIZE;
use serde::{Deserialize, Serialize};
use smrcommon::{get_random_port, remove_and_backup_directory};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use supra_logger::{init_runtime_logger, setup_panic_hook};
use task_manager::{async_task_tracker, notify_shutdown, wait_for_shutdown};
use tcp_console as console;
use tokio::sync::mpsc::{channel, Receiver};
use tracing::{debug, info, warn};
use transactions::SmrDkgCommitteeType;
use types::settings::committee::committee_config::config_v2::SupraCommitteesConfigV2;
use types::settings::committee::genesis_blob::GenesisBlob;
use types::settings::node_identity::ConstIdentityProvider;
use utoipa::Path as UtoipaPath;

/// NOTE: the `ntex` http server spawn its own tokio runtime and worker threads.
/// In current implementation, we use `tokio::spawn` to spawn tasks, which will be scheduled
/// in the same runtime as the http server.
/// TODO: Consider use separate tokio runtime of http server from other tasks. When we have high
///   load on the http server, it could block other tasks especially the block listener.
pub(crate) async fn start_rpc_server() -> error::Result<()> {
    info!("Starting RPC node ...");
    let mut config = RpcConfig::read_from_file(RPC_CONFIG_PATH)?;
    config.is_well_formed().map_err(Error::SetupError)?;

    let committee_config = config.supra_committee_config()?.into_v2();

    let port = get_random_port()?;
    let tokio_console_port: u16 = get_random_port()?;

    // Start logger.
    let (_guard, log_filter_reload) =
        init_runtime_logger(|| Box::new(std::io::sink()), tokio_console_port)?;

    setup_panic_hook();

    let mut console = console::Builder::new()
        .bind_address((Ipv4Addr::LOCALHOST, port))
        .subscribe(ConsoleServices::Log, log_filter_reload)?
        .accept_only_localhost()
        .build()?;

    console.spawn().await?;

    let (store, archive_db, ledger) = setup_databases(&config, &committee_config).await?;

    // Load and verify the [GenesisBlob].
    let mut genesis_blob = GenesisBlob::try_from_file(None)?;

    // Load or create the MoveStore with the genesis committee.
    let move_store = MoveStore::load_or_create_genesis(&mut genesis_blob, &store, None)?;

    let chain_id = move_store
        .chain_id()
        .ok_or(Error::SetupError(
            "Chain id not found in onchain configuration".to_string(),
        ))?
        .chain_id();
    // Write chain id to global cache.
    chainspec::set_chain_spec(chain_id.id() as u64);

    let move_exec = MoveExecutor::new();
    let rpc_storage = RpcStorage::new(
        archive_db.clone(),
        MoveSequenceNumberProvider::new(move_store.clone()),
        &config.backlog_parameters,
    );

    let (gas_price_writer, gas_price_reader) = gas_monitor();

    // Create the [EpochManager].
    let sync_only = true;
    let (
        mut epoch_manager,
        _network_subscription,
        tx_committee_certifier_message,
        _tx_epoch_manager_message,
        tx_new_epoch_notification,
    ) = RpcEpochManager::new(
        store.clone(),
        &genesis_blob,
        ConstIdentityProvider::rpc(),
        move_store.clone(),
        sync_only,
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to create the EpochManager due to {e}"));

    let exec_ctx = Executor::new(
        rpc_storage.clone(),
        committee_config,
        tx_new_epoch_notification,
        gas_price_writer,
        ledger,
        move_exec,
        store.clone(),
        None,
    );

    let (rpc_client, block_data_rx, rx_committee_auth) = setup_client(&config).await?;

    // Create the middleware for routing messages to and from the [CommitteeCertifier].
    let committee_certifier_sender = RpcEpochManagerMiddleware::init(
        rpc_client.clone(),
        rx_committee_auth,
        tx_committee_certifier_message,
    )
    .await;

    let mut chain_state_assembler_store = BlockStore::new(
        store.clone(),
        config
            .chain_state_assembler()
            .certified_block_cache_bucket_size(),
    );
    chain_state_assembler_store
        .initialize(&genesis_blob)
        .map_err(AssemblerError::from)?;
    let block_verifier =
        BlockVerifier::new(&epoch_manager, *config.block_provider_is_trusted()).await;

    let chain_state_assembler_inputs = ChainStateAssemblerDataInputs::new(
        block_data_rx,
        epoch_manager.subscribe(RpcEpochChangeNotificationSchedule::ChainStateAssembler),
    );
    let chain_state_assembler = ChainStateAssembler::new(
        chain_state_assembler_inputs,
        chain_state_assembler_store,
        block_verifier,
        exec_ctx,
        rpc_client.clone(),
        config.chain_state_assembler().sync_retry_interval_in_secs(),
    )
    .await?;

    setup_prune_scheduler(&config, store.clone(), archive_db, &mut epoch_manager)?;

    // Spawn the [EpochManager].
    epoch_manager
        .spawn_no_admin(committee_certifier_sender)
        .map_err(|e| Error::SetupError(format!("{e:?}")))?;
    // Spawn the [ChainStateAssembler].
    chain_state_assembler.spawn();

    // Order of consumers is important, because they are called in order they were added.
    // It is important to first try adding the tx to the local storage,
    // and if successful, send it further either to a validator node or a validator-connected RPC node.
    let txn_sender = TransactionDispatcher::default()
        .register_consumer(rpc_storage.clone())
        .register_consumer(rpc_client.clone());

    // State for http server must be created externally to server creation itself.
    // Otherwise, the state will be recreated for every http worker thread.
    // See, https://ntex.rs/docs/application#shared-mutable-state,
    // specifically look for "Key takeaways".
    let api_server_state = {
        let mut state_builder = ApiServerStateBuilder::new()
            .storage(rpc_storage.clone())
            .tx_sender(txn_sender.clone())
            .move_store(move_store.clone())
            .store(store)
            .gas_price_reader(gas_price_reader);

        if let Some(strategy) = config.limiting_strategy.take() {
            state_builder = state_builder.rest_call_limiter(strategy);
        }

        // Initially create a state without a faucet.
        let mut state = state_builder.build();

        match (!chainspec::get_chain_spec().is_mainnet(), config.faucet_settings()) {
            (true, Some(faucet_settings)) => {
                let minter_count = faucet_settings.minter_count();
                let quants_granted_per_request = faucet_settings.quants_granted_per_request();

                info!("Starting RPC node in Testnet configuration with faucet services with expected {} minters", minter_count);

                let faucet_call_limiter = FaucetCallLimiter::new(
                    faucet_settings.max_daily_requests(),
                    faucet_settings.cooldown_period_in_seconds(),
                );

                let faucet_state = FaucetState::empty(quants_granted_per_request, faucet_call_limiter);
                state.set_faucet_state(&faucet_state);

                // Spawn a task
                // - creating minting accounts,
                // - creating a respective faucet,
                // - setting that faucet to the previous state with no minters.
                async_task_tracker().spawn(async move {
                    let shutdown_token = task_manager::async_shutdown_token();

                    tokio::select! {
                        minter_accounts = Faucet::prepare(
                            minter_count,
                            txn_sender.clone(),
                            rpc_storage.archive().reader(),
                            move_store.clone(),
                        ) => {
                            faucet_state.set_faucet(Faucet::new(
                                &minter_accounts,
                                txn_sender,
                                rpc_storage.archive().reader(),
                                move_store,
                                quants_granted_per_request
                            )).await;
                        }
                        _ = shutdown_token.cancelled() => {
                            debug!("Received shutdown notification. Stopping minter addresses generation");
                        }
                    }
                });
            }
            (true, None) => info!("Starting RPC node in Testnet configuration with no faucet services"),
            (false, Some(_)) => warn!("Starting RPC node in Mainnet configuration. Faucet services configuration is ignored"),
            (false, None) => info!("Starting RPC node in Mainnet configuration"),
        };

        state
    };

    let swagger_docs_server_list = config.server_list()?;
    let server = web::HttpServer::new({
        let config = config.clone();
        move || {
            // Panic when invalid url present in config file
            let cors = config
                .server_cors()
                .unwrap_or_else(|e| panic!("Failed to configure CORS: {:?}", e));
            let mut app = web::App::new()
                .wrap(middleware::Logger::default())
                .wrap(cors);
            // Check if the guide folder exists
            if Path::new("/guide").exists() {
                // serve the folder `guide` which is at the same level of rpc_node executable
                // the guide folder is copied from docs/guide/html
                app = app.service(Files::new("/guide", "html_guide/"));
            }
            #[cfg(feature = "openapi")]
            let app = app.service(rest::docs::oai_route(swagger_docs_server_list.clone()));

            let root_rt =
                rest_root_route(api_server_state.clone(), config.consensus_access_tokens());
            app.service(root_rt)
                .default_service(web::route().to(default_route))
        }
    })
    .bind(*config.bind_addr())?
    .workers(config.http_server().workers() as usize)
    .run();

    info!("RPC service is ready at {}", config.bind_addr());
    let server_exit_result = server.await;

    // NOTE: Ntex server handle `SIGINT`, `SIGTERM`, `SIGQUIT` signals and gracefully stop ntex system
    // If hit here, it means the server is gracefully shutdown.
    notify_shutdown();
    wait_for_shutdown().await;

    // Shut down the console;
    console.stop();

    // No matter server exit OK or not, we should always notify graceful shutdown to our tasks.
    // So, defer the server exit result to the end.
    server_exit_result?;
    Ok(())
}

async fn setup_client(
    config: &RpcConfig,
) -> error::Result<(
    RpcClient,
    Receiver<RpcBlockData>,
    Receiver<CommitteeAuthorization>,
)> {
    let (block_data_tx, block_data_rx) = channel(RPC_INTERNAL_CHANNEL_SIZE);
    let (tx_committee_auth, rx_committee_auth) = channel(RPC_INTERNAL_CHANNEL_SIZE);
    // Build an RPC client node.
    let rpc_client = match config.synchronization() {
        RpcSync::Ws(RpcWsSync {
            certificates,
            consensus_rpc,
        }) => {
            info!("Establishing WS connection with endpoint: {consensus_rpc}");

            // TLS setup.
            info!("Running with the provided mTLS certificates");
            let tls_connector = tls_connector(
                certificates.root_ca_cert_path().clone().map(PathBuf::from),
                PathBuf::from(certificates.cert_path()).as_path(),
                PathBuf::from(certificates.private_key_path()).as_path(),
                // Panic when there is something wrong with the certificates.
                // Otherwise, the RPC node will keep trying to connect pointlessly with wrong certificates.
            )
            .expect("Valid mTLS certificates should be present at the specified paths");
            let config = WsConnectionSetupConfiguration::new_with(
                consensus_rpc.to_owned(),
                tls_connector,
                block_data_tx,
                tx_committee_auth,
            )
            .validate()
            .map_err(|e| Error::SetupError(format!("{e:?}")))?;
            RpcClient::connect_ws(config).await?
        }
        RpcSync::Rest(RpcRestSync {
            endpoint,
            request_latest_height,
            poll_interval_milliseconds,
            access_token,
            capacity,
        }) => {
            info!("Establishing REST sync-poll from endpoint: {endpoint}");
            let latest_block_endpoint = if *request_latest_height {
                Some(__path_latest_block::path())
            } else {
                None
            };

            let config = RestConnectionSetupConfiguration::new(
                endpoint.to_owned(),
                access_token.to_owned(),
                __path_submit_txn::path(),
                latest_block_endpoint,
                __path_latest_consensus_block::path(),
                __path_consensus_block::path(),
                __path_committee_authorization::path(),
                *poll_interval_milliseconds,
                block_data_tx,
                tx_committee_auth,
                capacity.unwrap_or(RPC_TRANSACTION_FORWARD_CHANNEL_SIZE),
            );
            RpcClient::connect_rest(config).await?
        }
    };
    Ok((rpc_client, block_data_rx, rx_committee_auth))
}

fn setup_prune_scheduler(
    settings: &RpcConfig,
    store: ChainStore,
    archive_db: ArchiveDb,
    epoch_manager: &mut RpcEpochManager,
) -> anyhow::Result<()> {
    let database_setup = settings.database_setup();
    let store_db_pruning_enabled = database_setup
        .storage_config_unchecked(StorageInstance::ChainStore)
        .is_pruning_enabled();
    let archive_db_pruning_enabled = database_setup
        .storage_config_unchecked(StorageInstance::Archive)
        .is_pruning_enabled();
    if !store_db_pruning_enabled && !archive_db_pruning_enabled {
        info!("Pruning is not enabled for any database");
        return Ok(());
    }
    let maybe_pruning_config = database_setup.prune_config();
    let Some(pruning_config) = maybe_pruning_config else {
        return Err(anyhow!(
            "Invalid database configuration, pruning for configured db(s) is(are)\
             enabled but no pruning config is specified: {database_setup:?}"
        ));
    };
    let epochs_to_retain = pruning_config.epochs_to_retain();
    let epoch_change_receiver = epoch_manager.subscribe(RpcEpochChangeNotificationSchedule::Pruner);
    let mut prune_scheduler_builder = pruner::Builder::<ChainStore, PrunerTask>::default()
        .with_epochs_to_retain(epochs_to_retain)
        .with_epoch_info_provider(store.clone())
        .with_new_epoch_state_provider(epoch_change_receiver);
    info!("Pruner is configured with {epochs_to_retain} epochs to retain.");
    if store_db_pruning_enabled {
        info!("ChainStore pruning is added as pruner task.");
        prune_scheduler_builder =
            prune_scheduler_builder.with_task(Arc::new(PrunerTask::new(Box::new(store))));
    }
    if archive_db_pruning_enabled {
        info!("ArchiveDb pruning is added as pruner task.");
        prune_scheduler_builder =
            prune_scheduler_builder.with_task(Arc::new(PrunerTask::new(Box::new(archive_db))));
    }
    let prune_scheduler = prune_scheduler_builder.build()?;
    prune_scheduler.spawn();
    Ok(())
}

async fn setup_databases(
    config: &RpcConfig,
    committee_config: &SupraCommitteesConfigV2,
) -> error::Result<(ChainStore, ArchiveDb, Ledger)> {
    info!("Setup Databases");
    let database_setup = config.database_setup();
    let store_config = database_setup.storage_config_unchecked(StorageInstance::ChainStore);
    let ledger_config = database_setup.storage_config_unchecked(StorageInstance::Ledger);
    let archive_config = database_setup.storage_config_unchecked(StorageInstance::Archive);
    // Ensures that configured storages are of will form rocks-db type, i.e. path is not empty and present.
    store_config
        .is_well_formed_rocks_db()
        .map_err(|e| Error::SetupError(format!("ChainStore: {e}")))?;
    ledger_config
        .is_well_formed_rocks_db()
        .map_err(|e| Error::SetupError(format!("Ledger: {e}")))?;
    archive_config
        .is_well_formed_rocks_db()
        .map_err(|e| Error::SetupError(format!("Archive: {e}")))?;
    // if resume is false then start the RPC node with clean state
    if !config.resume() {
        remove_and_backup_directory(store_config.path().expect("missing store_config path"))?;
        remove_and_backup_directory(ledger_config.path().expect("missing ledger_config path"))?;
        remove_and_backup_directory(archive_config.path().expect("missing archive_config path"))?;
    }

    let store = ChainStoreBuilder::new()
        .with_path(PathBuf::from(
            store_config.path().expect("Valid store path"),
        ))
        // Safe to unwrap as above it was checked that storage instance is of RocksDb type.
        .with_table_configs(
            store_config
                .tables()
                .expect("missing store_config tables")
                .clone(),
        )
        .build()
        .map_err(error::Error::from)?;

    let mut ledger = Ledger::new(Path::new(ledger_config.path().expect("Valid ledger path")))?;
    clean_dkg_data_from_ledger(&mut ledger, committee_config)?;

    let archive_db = ArchiveDbBuilder::new()
        .with_path(
            PathBuf::from_str(archive_config.path().expect("Must have archive db path"))
                .expect("Must valid archive db path"),
        )
        .with_table_configs(
            archive_config
                .tables()
                .expect("missing archive_config tables")
                .clone(),
        )
        .build(Some(&store))?;

    node::components::snapshot::setup_database_snapshot_scheduler(
        database_setup,
        &[
            (StorageInstance::ChainStore, store.db()),
            (StorageInstance::Archive, archive_db.db()),
            (StorageInstance::Ledger, ledger.db()),
        ],
    )?;

    Ok((store, archive_db, ledger))
}

// Temporary solution to have DKG data reset from ledger, until CG-DKG implementation is properly finalized.
fn clean_dkg_data_from_ledger(
    ledger: &mut Ledger,
    committee_config: &SupraCommitteesConfigV2,
) -> error::Result<()> {
    let committee_members = committee_config
        .committees()
        .iter()
        .filter(|item| item.name() == &SmrDkgCommitteeType::Smr)
        .flat_map(|item| item.members().values())
        .map(|member| {
            (
                member.identity(),
                member.node_public_identity.dkg_cg_public_key.clone(),
            )
        })
        .collect();
    reset_db_committee(ledger, committee_members).map_err(error::Error::ExecutionRetrieval)
}

#[derive(Serialize, Deserialize, Debug, Eq, Hash, PartialEq)]
/// List of services that TCP [console] supports.
enum ConsoleServices {
    Log,
}
