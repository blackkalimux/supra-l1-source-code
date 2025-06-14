use crate::rest::faucet::{ensure_tx, issue_tx, IssuingError};
use crate::transactions::dispatcher::{TransactionDispatchError, TransactionDispatcher};
use crate::transactions::tx_execution_notification::TxExecutionNotification;
use aptos_cached_packages::aptos_stdlib;
use aptos_crypto::ed25519::{
    Ed25519PrivateKey as AptosPrivateKey, Ed25519PublicKey as AptosPublicKey,
};
use aptos_crypto::PrivateKey;
use aptos_types::account_config;
use aptos_types::transaction::{Script, TransactionPayload};
use aptos_vm_genesis::GENESIS_KEYPAIR;
use execution::MoveStore;
use move_core_types::account_address::AccountAddress;
use move_core_types::transaction_argument::TransactionArgument;
use socrypto::Hash;
use std::fmt::{Debug, Display, Formatter};
use std::time::Duration;
use thiserror::Error;
use tokio::task::{JoinError, JoinHandle};
use tokio::time;
use tracing::debug;
use types::cli_profile::profile_management::ProfileAttributeOptions;
use types::cli_profile::protected_profile::ProtectedProfile;

#[derive(Clone)]
pub struct MinterAccount {
    protected_profile: ProtectedProfile,
    aptos_secret_key: AptosPrivateKey,
}

impl Debug for MinterAccount {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MinterAccount")
            .field(
                "Supra pub key",
                self.protected_profile.cli_profile().ed25519_public_key(),
            )
            .field("Aptos pub key", &self.aptos_secret_key.public_key())
            .finish()
    }
}

impl Display for MinterAccount {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl MinterAccount {
    const TX_MAX_WAIT: Duration = Duration::from_secs(2 * 60);

    const MINTER_SCRIPT: &'static [u8] =
        include_bytes!("../../../../crates/scripts/minter/build/Minter/bytecode_scripts/main.mv");

    pub fn aptos_private_key(&self) -> &AptosPrivateKey {
        &self.aptos_secret_key
    }

    pub fn aptos_public_key(&self) -> AptosPublicKey {
        self.aptos_secret_key.public_key()
    }
    pub fn public_key(&self) -> socrypto::PublicKey {
        *self.protected_profile.cli_profile().ed25519_public_key()
    }

    pub fn account_address(&self) -> AccountAddress {
        *self
            .protected_profile
            .cli_profile()
            .profile_attributes()
            .account_address()
    }

    /// [Self::INITIAL_BALANCE] must be sufficient to do minting.
    /// TODO Minter balance must always be sufficient to do actual minting.
    const INITIAL_BALANCE: u64 = 10 * 1_000_000_000;

    /// This function will create an account capable of minting new coins.
    ///
    /// This will be achieved by:
    /// - Creating a random account by depositing [Self::INITIAL_BALANCE] into that account.
    /// - Delegating minting capabilities to that account.
    /// - Claiming minting capabilities by that account.
    pub async fn generate(
        tx_sender: &TransactionDispatcher,
        move_store: &MoveStore,
        tx_execution_notification: &TxExecutionNotification,
    ) -> Result<Self, MinterAccountError> {
        debug!("Generating a minter account");
        // Fetch the root account data.
        let root_account = account_config::aptos_test_root_address();
        let root_aptos_private_key = GENESIS_KEYPAIR.0.clone();
        let root_aptos_public_key = GENESIS_KEYPAIR.1.clone();

        // Generate SMR secret key material.
        let minter_profile_secret =
            ProtectedProfile::generate(ProfileAttributeOptions::default(), false)?;

        let minter_aptos_private_key =
            AptosPrivateKey::try_from(minter_profile_secret.ed25519_secret_key())
                .expect("Must convert");
        let minter_aptos_public_key = minter_aptos_private_key.public_key();
        let minter_account = minter_profile_secret
            .cli_profile()
            .profile_attributes()
            .account_address();

        // Send some coins to a perspective minter.
        // This creates its account if it was not created and funds it.
        let tx_payload =
            aptos_stdlib::supra_account_transfer(*minter_account, Self::INITIAL_BALANCE);

        ensure_tx(
            "[Create/Fund minter account]",
            Self::TX_MAX_WAIT,
            tx_payload,
            root_account,
            &root_aptos_private_key,
            &root_aptos_public_key,
            tx_sender,
            move_store,
            tx_execution_notification,
        )
        .await?;

        // Delegate minting capabilities to the delegate account.
        let tx_payload = aptos_stdlib::supra_coin_delegate_mint_capability(*minter_account);
        ensure_tx(
            "[Delegate minting capabilities to minter account]",
            Self::TX_MAX_WAIT,
            tx_payload,
            root_account,
            &root_aptos_private_key,
            &root_aptos_public_key,
            tx_sender,
            move_store,
            tx_execution_notification,
        )
        .await?;

        // Claim minting capabilities.
        let tx_payload = aptos_stdlib::supra_coin_claim_mint_capability();
        ensure_tx(
            "[Claiming minting capabilities by minter account]",
            Self::TX_MAX_WAIT,
            tx_payload,
            *minter_account,
            &minter_aptos_private_key,
            &minter_aptos_public_key,
            tx_sender,
            move_store,
            tx_execution_notification,
        )
        .await?;

        let minter_account = Self {
            protected_profile: minter_profile_secret,
            aptos_secret_key: minter_aptos_private_key,
        };

        debug!("Minter account generated: {minter_account:#?}");

        Ok(minter_account)
    }

    /// To fund an account, the minter will invoke a Move script, which
    /// - mints the requested amount to itself,
    /// - transfer the requested amount to `account_address`.
    ///
    /// Returns
    /// - the tx [Hash] and
    /// - [JoinHandle] for a time-bound (see, [Self::TX_MAX_WAIT]) task
    ///   waiting for the tx to be executed (tx status is different from [TxExecutionStatus::Unexecuted]).
    pub async fn try_fund(
        &self,
        account_address: AccountAddress,
        amount: u64,
        tx_sender: &TransactionDispatcher,
        move_store: &MoveStore,
        tx_execution_notification: &TxExecutionNotification,
    ) -> Result<(Hash, JoinHandle<Result<Hash, MinterAccountError>>), TransactionDispatchError>
    {
        let local_account_address = self.account_address();

        let tx_descriptor =
            format!("Funding `{account_address}` with {amount} (by MinterAccount::MINTER_SCRIPT)");

        let minter_script = Script::new(
            Self::MINTER_SCRIPT.to_vec(),
            vec![],
            vec![
                TransactionArgument::Address(account_address),
                TransactionArgument::U64(amount),
            ],
        );

        let tx_payload = TransactionPayload::Script(minter_script);

        let (_, tx_hash, tx_handle) = issue_tx(
            &tx_descriptor,
            tx_payload,
            local_account_address,
            self.aptos_private_key(),
            &self.aptos_public_key(),
            tx_sender,
            move_store,
            tx_execution_notification,
        )
        .await?;

        let tx_handle = tokio::spawn(async move {
            let time_budget = time::sleep(Self::TX_MAX_WAIT);

            tokio::select! {
                _ = time_budget =>
                    Err(MinterAccountError::WaitBudgetExhausted {
                        tx_descriptor,
                        budget: Self::TX_MAX_WAIT,
                    }),
                tx_result = tx_handle =>
                    tx_result.map_err(MinterAccountError::Join).and_then(|r| r.map_err(|err| MinterAccountError::Issuing {
                inner: err, account: local_account_address, stage: "MINTER_SCRIPT".to_string(),})),
            }
        });

        Ok((tx_hash, tx_handle))
    }
}

#[derive(Debug, Error)]
pub enum MinterAccountError {
    #[error("Minter {account} failed at `{stage}` due to {inner}")]
    Issuing {
        inner: IssuingError,
        account: AccountAddress,
        stage: String,
    },
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error("Tx `{tx_descriptor}` stayed unexecuted for {budget:?}.")]
    WaitBudgetExhausted {
        tx_descriptor: String,
        budget: Duration,
    },
    #[error(transparent)]
    TxStatusError(#[from] TransactionDispatchError),
    #[error(transparent)]
    AnyHow(#[from] anyhow::Error),
}
