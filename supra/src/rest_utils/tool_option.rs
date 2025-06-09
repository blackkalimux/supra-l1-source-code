use crate::common::error::CliError;
use crate::move_tool::error::MoveToolError;
use crate::move_tool::move_account::SupraCoinTypeResolver;
use crate::rest_utils::action::{CliEstimatedGasPrice, CliTransactionInfo, RpcAction, RpcResponse};
use crate::rest_utils::client::supra_rpc_client::RpcClientApi;
use crate::rest_utils::client::RPCClientIfc;
use crate::rest_utils::rpc_request_kind::public_query::PublicQueryHelper;
use crate::rest_utils::rpc_request_kind::signed_transaction::SignedTransactionHelper;
use crate::rest_utils::rpc_request_kind::RpcRequestKind;
use aptos_crypto::ed25519::Ed25519PrivateKey;
use aptos_types::account_address::AccountAddress;
use aptos_types::transaction::TransactionPayload;
use clap::Parser;
use inquire::Text;
use socrypto::Hash;
use std::cmp::min;
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::time::Duration;
use supra_aptos::SupraCommandArguments;
use tracing::debug;
use transactions::{TTransactionHeaderProperties, TxExecutionStatus};
use types::api::{v1::TransactionOutput, ApiVersion};
use types::cli_profile::profile_management::{
    CliProfileManager, ProfileAttributeOptions, ProtectedProfileManager, SignerKeyType,
    SignerProfileHelper, TransactionType,
};
use types::cli_profile::protected_profile::{ProfileAttributes, ProtectedProfile};
use types::cli_profile::socrypto_secret_key_wrapper::SoCryptoSecretKey;
use types::cli_utils::constants::{
    DEFAULT_CONNECTION_TIMEOUT_SECS, DEFAULT_MAX_GAS_AMOUNT_FOR_SIMULATION,
};
use types::impl_struct_create_and_read_methods;
use types::move_types::coin_parser;
use types::move_types::coin_parser::CoinResourceData;
use types::settings::backlog;
use types::settings::constants::REDACTED;
use url::Url;

#[derive(Default, Parser, Clone, Eq, PartialEq)]
pub struct PrivateKeyInputOptions {
    /// Signing Ed25519 private key file path
    #[clap(long, group = "private_key_options")]
    private_key_file: Option<PathBuf>,
    /// Signing Ed25519 private key
    #[clap(long, group = "private_key_options")]
    private_key: Option<SoCryptoSecretKey>,
}

impl Debug for PrivateKeyInputOptions {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrivateKeyInputOptions")
            .field("private_key_file", &self.private_key_file)
            .field("private_key", &REDACTED)
            .finish()
    }
}

#[derive(Clone, Debug, Parser)]
pub struct SupraTransactionOptions {
    #[clap(flatten)]
    pub(crate) profile_options: SupraProfileOptions,
    #[clap(flatten)]
    pub(crate) rest_options: RestOptions,
    #[clap(flatten)]
    pub(crate) gas_options: GasOptions,
    #[clap(long, help = "Delegation pool address")]
    pub delegation_pool_address: Option<String>,
}

#[derive(Debug, Default, Parser, Clone, Eq, PartialEq)]
pub struct SupraProfileOptions {
    /// Profile to use from the CLI config
    ///
    /// This will be used to override associated settings such as
    /// the REST URL, the Faucet URL, and the private key arguments.
    ///
    /// Defaults to active profile
    #[clap(long)]
    pub(crate) profile: Option<String>,
    #[clap(flatten)]
    pub(crate) private_key_options: PrivateKeyInputOptions,
    #[clap(flatten)]
    pub(crate) profile_attribute_options: ProfileAttributeOptions,
    /// Sender account address
    ///
    /// This allows you to override the account address from the derived account address
    /// in the event that the authentication key was rotated or for a resource account
    #[clap(long)]
    pub(crate) sender_account: Option<AccountAddress>,
}

impl_struct_create_and_read_methods!(
    SupraProfileOptions {
    /// Profile to use from the CLI config
    ///
    /// This will be used to override associated settings such as
    /// the REST URL, the Faucet URL, and the private key arguments.
    ///
    /// Defaults to active profile
    profile: Option<String>,
    private_key_options: PrivateKeyInputOptions,
    profile_attribute_options: ProfileAttributeOptions,
    /// Sender account address
    ///
    /// This allows you to override the account address from the derived account address
    /// in the event that the authentication key was rotated or for a resource account
    sender_account: Option<AccountAddress>,
});

/// Options specific to using the Rest endpoint
#[derive(Clone, Debug, Default, Parser)]
pub struct RestOptions {
    /// Connection timeout in seconds, used for the REST endpoint of the full node
    #[clap(long, default_value_t = DEFAULT_CONNECTION_TIMEOUT_SECS, alias = "connection-timeout-s")]
    pub(crate) connection_timeout_secs: u64,

    /// Key to use for ratelimiting purposes with the node API. This value will be used
    /// as `Authorization: Bearer <key>`. You may also set this with the NODE_API_KEY
    /// environment variable.
    #[clap(long, env)]
    pub(crate) node_api_key: Option<String>,
    #[clap(long, default_value_t = ApiVersion::V3)]
    pub(crate) api_version: ApiVersion,
}

/// Gas price options for manipulating how to prioritize transactions
#[derive(Clone, Debug, Eq, Parser, PartialEq)]
pub struct GasOptions {
    /// Gas multiplier per unit of gas
    ///
    /// The amount of Quants (10^-8 SUPRA) used for a transaction is equal
    /// to (gas unit price * gas used).  The gas_unit_price can
    /// be used as a multiplier for the amount of Quants willing
    /// to be paid for a transaction.  This will prioritize the
    /// transaction with a higher gas unit price.
    ///
    /// Without a value, it will determine the price based on the current estimated price
    #[clap(long)]
    pub(crate) gas_unit_price: Option<u64>,
    /// Maximum amount of gas units to be used to send this transaction
    ///
    /// The maximum amount of gas units willing to pay for the transaction.
    /// This is the (max gas in Quants / gas unit price).
    ///
    /// For example if I wanted to pay a maximum of 100 Quants, I may have the
    /// max gas set to 100 if the gas unit price is 1.  If I want it to have a
    /// gas unit price of 2, the max gas would need to be 50 to still only have
    /// a maximum price of 100 Quants.
    ///
    /// Without a value, it will determine the price based on simulating the current transaction
    #[clap(long)]
    pub(crate) max_gas: Option<u64>,
    /// Number of seconds to expire the transaction
    ///
    /// This is the number of seconds from the current local computer time.
    #[clap(long, default_value_t = safe_default_tx_ttl())]
    pub(crate) expiration_secs: u64,
}

pub struct GasParameters {
    pub(crate) gas_unit_price: u64,
    pub(crate) max_gas: u64,
    pub(crate) expiration_secs: u64,
}

impl_struct_create_and_read_methods!(GasParameters {
    gas_unit_price: u64,
    max_gas: u64,
    expiration_secs: u64,
});

impl SupraProfileOptions {
    /// Profile attribute based on profile name or active profile.
    pub fn profile_attribute(&self) -> Result<ProfileAttributes, CliError> {
        let profile_attributes = match self.profile {
            None => CliProfileManager::active_cli_profile()?
                .profile_attributes()
                .clone(),
            Some(ref profile) => CliProfileManager::cli_profile(profile)?
                .profile_attributes()
                .clone(),
        };
        Ok(profile_attributes)
    }

    /// Precedence: active profile > profile name > private key.
    /// Does not attempt to read the password for the related profile from
    /// `NODE_OPERATOR_KEY_PASSWORD` if `ignore_env` is `true`.
    pub fn protected_profile(
        &self,
        ignore_env: bool,
    ) -> Result<(ProtectedProfile, String), CliError> {
        let (protected_profile_manager, password) = match self
            .private_key_options
            .local_protected_profile_manager(self.profile_attribute_options.clone())?
        {
            None => ProtectedProfileManager::read_or_create(ignore_env, false)?,
            Some(temp_profile) => temp_profile,
        };

        Ok(match self.profile {
            None => (
                protected_profile_manager
                    .get_active_protected_profile()?
                    .clone(),
                password,
            ),
            Some(ref txn_sender_profile) => (
                protected_profile_manager
                    .try_smr_private_key_ref(txn_sender_profile)?
                    .clone(),
                password,
            ),
        })
    }
}

impl SupraTransactionOptions {
    pub fn api_version(&self) -> ApiVersion {
        self.rest_options.api_version
    }

    /// Cli Profile attribute.
    pub fn profile_attribute(&self) -> Result<ProfileAttributes, CliError> {
        let mut profile_attributes = self.profile_options.profile_attribute()?;
        if let Some(address) = &self
            .profile_options
            .profile_attribute_options
            .account_address()
        {
            *profile_attributes.account_address_mut() = *address
        }
        if let Some(rpc_url) = &self.profile_options.profile_attribute_options.rpc_url() {
            *profile_attributes.rpc_url_mut() = rpc_url.clone();
            *profile_attributes.faucet_url_mut() = rpc_url.clone();
            // TODO: Fix this for CI only [#1453](https://github.com/Entropy-Foundation/smr-moonshot/issues/1453) in case of unit test
            *profile_attributes.chain_id_mut() = ProfileAttributes::fetch_and_confirm_chain_id(
                rpc_url.clone(),
                *profile_attributes.chain_id(),
            )?
        }
        Ok(profile_attributes)
    }

    /// Does not attempt to read the password for the related profile from
    /// `NODE_OPERATOR_KEY_PASSWORD` if `ignore_env` is `true`.
    pub fn signer_profile_helper(
        &self,
        txn_type: TransactionType,
        ignore_env: bool,
    ) -> Result<SignerProfileHelper, CliError> {
        let (transaction_sender_protected_profile, password) =
            self.profile_options.protected_profile(ignore_env)?;

        let public_key = transaction_sender_protected_profile
            .cli_profile()
            .aptos_signer_public_key()
            .map_err(MoveToolError::ExecutorCryptoError)?;

        let signer_key_type = {
            match txn_type {
                TransactionType::Simulate => SignerKeyType::Simulate,
                TransactionType::Execute => SignerKeyType::Execute(
                    Ed25519PrivateKey::try_from(
                        transaction_sender_protected_profile.ed25519_secret_key(),
                    )
                    .map_err(MoveToolError::ExecutorCryptoError)?,
                ),
            }
        };
        let sender_attributes = self.profile_attribute()?;
        Ok(SignerProfileHelper::new(
            *sender_attributes.account_address(),
            public_key,
            signer_key_type,
            sender_attributes.rpc_url().clone(),
            aptos_types::chain_id::ChainId::new(*sender_attributes.chain_id()),
            password,
        ))
    }
}

impl PrivateKeyInputOptions {
    /// Create a temporary cli profile to reuse logics made for [ProtectedProfileManager] without
    /// importing into current cli profile. The temporary profile can be made using only
    /// a private key or another supra cli profile file.
    pub fn local_protected_profile_manager(
        &self,
        profile_attribute_options: ProfileAttributeOptions,
    ) -> Result<Option<(ProtectedProfileManager, String)>, CliError> {
        match (self.private_key_file.as_ref(), self.private_key.as_ref()) {
            (Some(file_path), None) => Ok(Some(ProtectedProfileManager::load_private_profile(
                file_path, None,
            )?)),
            (None, Some(private_key)) => Ok(Some((
                ProtectedProfileManager::local_protected_profile_manager(
                    private_key.clone(),
                    profile_attribute_options,
                )?,
                "temp_password".to_string(),
            ))),
            (None, None) => Ok(None),
            (Some(_), Some(_)) => Err(CliError::Aborted(
                "Either CLI profile file path or private key should be provided.".to_string(),
                "Please Try again.".to_string(),
            )),
        }
    }
}

#[derive(Debug)]
pub struct MoveTransactionOptions {
    pub payload: TransactionPayload,
    pub transaction_option: SupraTransactionOptions,
}

pub enum ResolveTransactionHash {
    DoNotResolve,
    UntilSuccess,
}

impl From<SupraCommandArguments> for MoveTransactionOptions {
    fn from(value: SupraCommandArguments) -> Self {
        let profile_attribute_option =
            ProfileAttributeOptions::new(None, value.rest_options.rpc_url.clone(), None, None);
        let supra_profile_option = SupraProfileOptions::new(
            value.profile_options.profile,
            PrivateKeyInputOptions::default(),
            profile_attribute_option,
            value.sender_account,
        );
        Self {
            payload: value.payload,
            transaction_option: SupraTransactionOptions {
                profile_options: supra_profile_option,
                rest_options: RestOptions::from(value.rest_options),
                gas_options: GasOptions::from(value.gas_options),
                delegation_pool_address: None,
            },
        }
    }
}

impl From<supra_aptos::GasOptions> for GasOptions {
    fn from(value: supra_aptos::GasOptions) -> Self {
        Self {
            gas_unit_price: value.gas_unit_price,
            max_gas: value.max_gas,
            expiration_secs: value.expiration_secs,
        }
    }
}

impl From<supra_aptos::RestOptions> for RestOptions {
    fn from(value: supra_aptos::RestOptions) -> Self {
        Self {
            connection_timeout_secs: value.connection_timeout_secs,
            node_api_key: value.node_api_key,
            api_version: match value.api_version {
                supra_aptos::ApiVersion::V1 => ApiVersion::V1,
                supra_aptos::ApiVersion::V2 => ApiVersion::V2,
                supra_aptos::ApiVersion::V3 => ApiVersion::V3,
            },
        }
    }
}

impl MoveTransactionOptions {
    /// Retry after every [RETRY_DELAY]
    const RETRY_DELAY: u64 = 1;

    pub fn new(payload: TransactionPayload, transaction_option: SupraTransactionOptions) -> Self {
        Self {
            payload,
            transaction_option,
        }
    }

    fn api_version(&self) -> ApiVersion {
        self.transaction_option.rest_options.api_version
    }

    async fn confirm_or_modify_gas_parameters(
        &mut self,
        signer_profile_helper: SignerProfileHelper,
    ) -> Result<GasParameters, CliError> {
        let user_supplied_max_gas = self.transaction_option.gas_options.max_gas;
        let user_supplied_gas_unit_price = self.transaction_option.gas_options.gas_unit_price;
        let user_supplied_expiration_secs = self.transaction_option.gas_options.expiration_secs;

        match (user_supplied_max_gas, user_supplied_gas_unit_price) {
            // If both options are supplied, use them as they are.
            (Some(max_gas), Some(gas_unit_price)) => Ok(GasParameters::new(
                gas_unit_price,
                max_gas,
                user_supplied_expiration_secs,
            )),
            // If gas_unit_price is not supplied, fetch the estimate from an RPC node and prompt the user of the value
            (Some(max_gas), None) => {
                let gas_price_estimates = Self::fetch_gas_estimate(
                    self.api_version(),
                    self.transaction_option
                        .profile_attribute()?
                        .rpc_url()
                        .clone(),
                )
                .await?;

                let final_gas_unit_price = Self::confirm_gas_parameter_u64(
                    format!(
                        "Proceed with a Gas Unit Price of {} Quants? Press return to accept or enter a different value.",
                        gas_price_estimates
                ),
                    gas_price_estimates
                )?;

                Ok(GasParameters::new(
                    final_gas_unit_price,
                    max_gas,
                    user_supplied_expiration_secs,
                ))
            }
            // If max_gas is not supplied, simulate the transaction and prompt the user of the value
            (None, Some(gas_unit_price)) => {
                let (simulated_max_gas_fee, _) =
                    self.simulate_gas_fee(signer_profile_helper).await?;

                let final_max_gas = Self::confirm_gas_parameter_u64(
                    format!("Proceed with a Max Gas Amount of {}? Press return to accept or enter a different value.", simulated_max_gas_fee),
                    simulated_max_gas_fee
                )?;

                Ok(GasParameters::new(
                    gas_unit_price,
                    final_max_gas,
                    user_supplied_expiration_secs,
                ))
            }
            // If none of the options are supplied:
            // 1. Simulate the transaction for max_gas value
            // 2. Fetch the Gas price estimates from an RPC node for gas_unit_price value
            // 3. Prompt the User for both the values
            (None, None) => {
                let (simulated_max_gas_fee, _) =
                    self.simulate_gas_fee(signer_profile_helper).await?;

                let gas_price_estimates = Self::fetch_gas_estimate(
                    self.api_version(),
                    self.transaction_option
                        .profile_attribute()?
                        .rpc_url()
                        .clone(),
                )
                .await?;

                let final_max_gas = Self::confirm_gas_parameter_u64(
                    format!("Proceed with a Max Gas Amount of {}? Press return to accept or enter a different value.", simulated_max_gas_fee),
                    simulated_max_gas_fee,
                )?;

                let final_gas_unit_price = Self::confirm_gas_parameter_u64(
                    format!(
                        "Proceed with a Gas Unit Price of {} Quants? Press return to accept or enter a different value.",
                        gas_price_estimates
                    ),
                    gas_price_estimates
                )?;

                Ok(GasParameters::new(
                    final_gas_unit_price,
                    final_max_gas,
                    user_supplied_expiration_secs,
                ))
            }
        }
    }

    // Simulates the transaction and returns the gas fee and the Gas Unit Price consumed.
    pub async fn simulate_gas_fee(
        &mut self,
        signer_profile_helper: SignerProfileHelper,
    ) -> Result<(u64, u64), CliError> {
        let simulated_txn_info = self
            .simulate_signer_profile_transaction(signer_profile_helper)
            .await?;

        // Early return if there are any errors in the simulation
        if TxExecutionStatus::Success.ne(simulated_txn_info.status()) {
            return Err(CliError::Aborted(
                format!("{:?}", simulated_txn_info.output()),
                "Please try again.".to_string(),
            ));
        }

        // Process the simulated gas values
        if let Some(TransactionOutput::Move(move_txn_output)) = simulated_txn_info.output() {
            Ok((
                move_txn_output.gas_used,
                simulated_txn_info.gas_unit_price(),
            ))
        } else {
            Ok((
                simulated_txn_info.max_gas_amount(),
                simulated_txn_info.gas_unit_price(),
            ))
        }
    }

    pub async fn fetch_account_balance(
        api_version: ApiVersion,
        rpc_url: Url,
        account_address: AccountAddress,
    ) -> Result<CoinResourceData, CliError> {
        match RpcClientApi::new_with_action(
            RpcAction::AccountResource(
                api_version,
                account_address,
                SupraCoinTypeResolver::CoinResource.resolve(&None)?,
            ),
            RpcRequestKind::PublicQuery(PublicQueryHelper::new(rpc_url.clone())),
        )?
        .execute_action()
        .await
        {
            Ok(RpcResponse::AccountResource(response)) if response.status().is_success() => {
                match api_version {
                    ApiVersion::V1 => Ok(response
                        .json::<coin_parser::v1::ApiResponse>()
                        .await?
                        .coin_resource_data()),
                    ApiVersion::V2 | ApiVersion::V3 => Ok(response
                        .json::<coin_parser::v2::ApiResponse>()
                        .await?
                        .coin_resource_data()),
                }
            }

            Ok(res) => Err(CliError::GeneralError(format!(
                "Received an unexpected result {:?} when fetching account balance from RPC {:?}",
                res, rpc_url
            ))),
            Err(e) => Err(e),
        }
    }

    pub async fn fetch_gas_estimate(
        api_version: ApiVersion,
        rpc_url: Url,
    ) -> Result<u64, CliError> {
        match RpcClientApi::new_with_action(
            RpcAction::EstimatedGasPrice(api_version),
            RpcRequestKind::PublicQuery(PublicQueryHelper::new(rpc_url.clone())),
        )?
        .execute_action()
        .await
        {
            Ok(RpcResponse::EstimatedGasPrice(gas_price_res)) => match gas_price_res {
                CliEstimatedGasPrice::V1(r) => Ok(r.mean_gas_price),
                CliEstimatedGasPrice::V2(r) => Ok(r.median_gas_price),
            },
            Ok(res) => Err(CliError::GeneralError(format!(
                "Received an unexpected result {:?} when fetching gas estimates from RPC {:?}",
                res, rpc_url
            ))),
            Err(e) => Err(e),
        }
    }

    fn confirm_gas_parameter_u64(message: String, default: u64) -> Result<u64, CliError> {
        let help = "Leave blank and press Return to accept the value";

        loop {
            let status = Text::new(&message).with_help_message(help).prompt();

            match status {
                Ok(input) => {
                    if input.trim().is_empty() {
                        return Ok(default);
                    }
                    match input.parse::<u64>() {
                        Ok(gas_unit_price) => return Ok(gas_unit_price),
                        Err(e) => {
                            eprintln!("Invalid input {}. Please enter a valid u64 value.", e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    return Err(CliError::Aborted(
                        e.to_string(),
                        "Please try again.".to_string(),
                    ));
                }
            }
        }
    }

    /// Simulate the transaction based on Default gas option if option not provided by the user
    /// max_gas = [DEFAULT_MAX_GAS_AMOUNT]
    /// gas_unit_price = [DEFAULT_GAS_UNIT_PRICE]
    pub async fn simulate_transaction(&mut self) -> Result<CliTransactionInfo, CliError> {
        let signer_profile_helper = self
            .transaction_option
            .signer_profile_helper(TransactionType::Simulate, false)?;
        self.simulate_signer_profile_transaction(signer_profile_helper)
            .await
    }

    async fn simulate_signer_profile_transaction(
        &mut self,
        signer_profile_helper: SignerProfileHelper,
    ) -> Result<CliTransactionInfo, CliError> {
        let gas_unit_price = self
            .transaction_option
            .gas_options
            .gas_unit_price
            .unwrap_or(
                Self::fetch_gas_estimate(
                    self.api_version(),
                    signer_profile_helper.rpc_url().clone(),
                )
                .await?,
            );
        let max_gas = self.transaction_option.gas_options.max_gas.unwrap_or({
            let acc_bal = Self::fetch_account_balance(
                self.api_version(),
                signer_profile_helper.rpc_url().clone(),
                *signer_profile_helper.account_address(),
            )
            .await?;
            if acc_bal.is_frozen() {
                return Err(CliError::Aborted(
                    "Transaction failed: SupraCoin balance is frozen.".to_string(),
                    "Please unfreeze your SupraCoin balance before proceeding.".to_string(),
                ));
            }
            min(acc_bal.balance(), DEFAULT_MAX_GAS_AMOUNT_FOR_SIMULATION)
        });
        match RpcClientApi::new_with_action(
            RpcAction::SimulateTransaction(self.api_version(), self.payload.clone()),
            RpcRequestKind::SignedTransaction(SignedTransactionHelper::new_user_object(
                signer_profile_helper,
                GasParameters::new(
                    gas_unit_price,
                    max_gas,
                    self.transaction_option.gas_options.expiration_secs,
                ),
            )),
        )?
        .execute_action()
        .await?
        {
            RpcResponse::SimulateTransaction { info, .. } => Ok(info),
            _ => Err(CliError::RESTClient),
        }
    }

    pub async fn send_transaction(
        mut self,
        resolve_txn_hash: ResolveTransactionHash,
    ) -> Result<CliTransactionInfo, CliError> {
        let payload_debug = format!("{:?}", self.payload);
        let signer_profile_helper = self
            .transaction_option
            .signer_profile_helper(TransactionType::Execute, false)?;
        let signer_profile_simulation_helper = signer_profile_helper.simulation_helper();
        let rpc_url = signer_profile_helper.rpc_url().clone();
        let txn_hash = match RpcClientApi::new_with_action(
            RpcAction::SendTransaction(self.api_version(), self.payload.clone()),
            RpcRequestKind::SignedTransaction(SignedTransactionHelper::new_user_object(
                signer_profile_helper,
                self.confirm_or_modify_gas_parameters(signer_profile_simulation_helper)
                    .await?,
            )),
        )?
        .execute_action()
        .await?
        {
            RpcResponse::SendTransaction {
                hash,
                expiration_timestamp_secs,
            } => {
                match resolve_txn_hash {
                    ResolveTransactionHash::DoNotResolve => {}
                    ResolveTransactionHash::UntilSuccess => {
                        debug!("Waiting for tx {hash} to get finalized");
                        RpcClientApi::await_finalized_with_success(
                            self.api_version(),
                            hash,
                            expiration_timestamp_secs,
                            RpcRequestKind::PublicQuery(PublicQueryHelper::new(rpc_url)),
                            Duration::from_secs(Self::RETRY_DELAY),
                        )
                        .await?;
                    }
                }
                Ok::<Hash, CliError>(hash)
            }
            unexpected_response => Err(CliError::UnexpectedResponse(
                payload_debug,
                unexpected_response,
            )),
        }?;
        self.check_transaction_info(txn_hash).await
    }

    async fn check_transaction_info(&self, txn_hash: Hash) -> Result<CliTransactionInfo, CliError> {
        match RpcClientApi::new_with_action(
            RpcAction::TransactionInfo(self.api_version(), txn_hash),
            RpcRequestKind::PublicQuery(PublicQueryHelper::new(
                self.transaction_option
                    .profile_attribute()?
                    .rpc_url()
                    .clone(),
            )),
        )?
        .execute_action()
        .await?
        {
            RpcResponse::TransactionInfo(info) => Ok(info),
            _ => Err(CliError::RESTClient),
        }
    }
}

/// This function assigns a default value for the transaction TTL during command-line parsing.
///
/// The backlog checks that the transaction TTL does not exceed the configured maximum allowed TTL
/// by calculating the difference between the transaction expiration timestamp and the transaction
/// reception timestamp. Since the transaction may not arrive immediately, some additional time is
/// allowed to prevent inadvertently violating the backlog transaction expiration condition.
fn safe_default_tx_ttl() -> u64 {
    backlog::Parameters::percentage_of_default_tx_ttl(0.75)
}
