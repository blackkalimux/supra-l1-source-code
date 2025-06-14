use crate::rest::faucet::minter_account::MinterAccount;
use crate::transactions::dispatcher::{TransactionDispatchError, TransactionDispatcher};
use crate::transactions::tx_execution_notification::TxExecutionNotification;
use archive::reader::ArchiveReader;
use execution::MoveStore;
use move_core_types::account_address::AccountAddress;
use socrypto::Hash;
use std::fmt::{Debug, Display, Formatter};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use task_manager::async_task_tracker;
use tracing::{debug, warn};

/// Generalized faucet acting as an umbrella for many [AccountAddress] with minting capabilities.
pub struct Faucet {
    delegates: Vec<DelegateState>,
    quants_granted_per_request: u64,
}

impl Faucet {
    /// [Faucet] will be issuing sequences of txs which need to be executed in order.
    /// To issue a next tx, [Faucet] needs to verify that the previous tx has gone through.
    /// To achieve that goal, it will poll the passed [ArchiveReader] with [Self::ARCHIVE_POLL_PERIODICITY].
    const ARCHIVE_POLL_PERIODICITY: Duration = Duration::from_secs(1);

    /// Creates a requested `num_minters` of [MinterAccount]'s.
    /// Each [MinterAccount] has minting capabilities.
    pub async fn prepare(
        num_minters: usize,
        tx_sender: TransactionDispatcher,
        archive_reader: ArchiveReader,
        move_store: MoveStore,
    ) -> Vec<MinterAccount> {
        debug!("Preparing minter accounts");
        let tx_inclusion_notification =
            TxExecutionNotification::new(Self::ARCHIVE_POLL_PERIODICITY, archive_reader);
        let mut addresses = Vec::new();

        for i in 0..num_minters {
            let address =
                match MinterAccount::generate(&tx_sender, &move_store, &tx_inclusion_notification)
                    .await
                {
                    Ok(address) => {
                        debug!("Minter {i} is ready at {address}");
                        address
                    }
                    Err(err) => {
                        // If generation of an address failed for some reason,
                        // log the error and optimistically proceed.
                        warn!("Minter {i} cannot be generated: {err:?}");
                        continue;
                    }
                };
            addresses.push(address);
        }
        debug!(
            "Prepared {} of requested {num_minters} minters.\nAccounts: {addresses:#?}",
            addresses.len()
        );

        addresses
    }

    pub fn new_with_no_delegates(coins_granted_per_request: u64) -> Self {
        Self {
            delegates: Vec::new(),
            quants_granted_per_request: coins_granted_per_request,
        }
    }

    /// Creates a [Faucet] acting as an umbrella for multiple [MinterAccount]'s.
    /// `minter_accounts`  can be created using [Self::prepare].
    pub fn new(
        minter_accounts: &[MinterAccount],
        tx_sender: TransactionDispatcher,
        archive_reader: ArchiveReader,
        move_store: MoveStore,
        quants_granted_per_request: u64,
    ) -> Self {
        debug!(
            "Initializing a faucet with {} delegates",
            minter_accounts.len()
        );

        let tx_execution_notification =
            TxExecutionNotification::new(Self::ARCHIVE_POLL_PERIODICITY, archive_reader);

        Self {
            delegates: minter_accounts
                .iter()
                .cloned()
                .map(|account| {
                    DelegateState::new(
                        account,
                        tx_sender.clone(),
                        move_store.clone(),
                        tx_execution_notification.clone(),
                    )
                })
                .collect(),
            quants_granted_per_request,
        }
    }

    /// Try delegating minting to a first available delegate.
    /// If a free delegate is found, try submitting a funding transaction.
    /// if submission is successful, return Some(Ok(hash_of_funding_transaction)).
    /// if submission failed, return Some(Err).
    /// If no one can fund, return None.
    pub async fn try_fund(&self, account: AccountAddress) -> Option<Hash> {
        let free_delegates = self.delegates.iter().filter(|d| d.is_free()).count();
        debug!(
            "[Assessing] Funding request for {account}. Free delegates {free_delegates}/{}",
            self.delegates.len()
        );

        if free_delegates == 0 {
            debug!("[Rejecting] Funding request for {account}. No free delegates");
            return None;
        }

        for delegate in &self.delegates {
            let funding_attempt = delegate
                .try_fund(account, self.quants_granted_per_request)
                .await;

            match funding_attempt {
                Some(Ok(tx_hash)) => {
                    let free_delegates = self.delegates.iter().filter(|d| d.is_free()).count();
                    debug!(
                        "[Processing] Funding request for {account}. Leftover free delegates {free_delegates}"
                    );
                    return Some(tx_hash);
                }
                Some(Err(err)) => {
                    warn!("[Processing] Funding attempt for {account} failed: {err:?}. Trying next minter");
                    continue;
                }
                None => {
                    // This delegate is busy and can't process the request.
                    continue;
                }
            }
        }

        // No delegate picked up the request.
        None
    }
}

/// [DelegateState] describes either a [Delegate::Free] or [Delegate::Busy] minter.
/// Free minter will be able to pick up funding requests.
struct DelegateState {
    state: Arc<Mutex<Option<Delegate>>>,
    tx_sender: TransactionDispatcher,
    move_store: MoveStore,
    tx_execution_notification: TxExecutionNotification,
}

impl DelegateState {
    /// Creates a new [DelegateState].
    fn new(
        minter: MinterAccount,
        tx_sender: TransactionDispatcher,
        move_store: MoveStore,
        tx_execution_notification: TxExecutionNotification,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(Some(Delegate::Free { minter }))),
            move_store,
            tx_sender,
            tx_execution_notification,
        }
    }

    /// If this delegate is [Delegate::Free] it will pick up the funding request
    /// submit a funding transaction and return Some(transaction submission result).
    /// If this delegate is [Delegate::Busy] it will return None.
    async fn try_fund(
        &self,
        account_address: AccountAddress,
        amount: u64,
    ) -> Option<Result<Hash, TransactionDispatchError>> {
        let minter = {
            // Scope the state guard manipulations to avoid manual lock dropping before `await`-ing.

            let mut guard = self.state.lock().expect("Delegate must be lockable");
            {
                // Debug scope.
                let delegate = guard
                    .as_ref()
                    .expect("[BUG] DelegateState must always contain Delegate");
                let delegate_action = if delegate.is_free() {
                    "Requesting"
                } else {
                    "Skipping"
                };
                debug!("{delegate_action} {delegate}");
            }

            if let Some(Delegate::Busy { .. }) = *guard {
                // Busy delegates cannot fund, duh.
                return None;
            }

            // Set own state to [Delegate::Busy] ASAP.
            let Delegate::Free { minter } =
                guard.take().expect("[BUG] Delegate state may not be empty")
            else {
                unreachable!("DelegateState has been verified to be Free");
            };

            *guard = Some(Delegate::Busy {
                minter: minter.clone(),
                target_account: account_address,
            });

            minter
        };

        // This [MinterAccount] is free.
        let funding_attempt = minter
            .try_fund(
                account_address,
                amount,
                &self.tx_sender,
                &self.move_store,
                &self.tx_execution_notification,
            )
            .await;

        let (tx_hash, tx_handle) = match funding_attempt {
            Ok(ok) => ok,
            Err(err) => {
                // Set own state to [Delegate::Free].
                *self.state.lock().expect("Delegate must be lockable") =
                    Some(Delegate::Free { minter });
                // Return the error.
                return Some(Err(err));
            }
        };

        {
            let minter = minter.clone();
            let state = self.state.clone();

            // Wait the funding tx and an eventual (time-bound) return to a free state.
            async_task_tracker().spawn(async move {
                debug!(
                        "[Minter {}] funding {account_address} with {amount}",
                        minter.account_address()
                    );

                let shutdown_token = task_manager::async_shutdown_token();

                tokio::select! {
                        tx_result = tx_handle => match tx_result {
                            Ok(Ok(tx_hash)) => debug!(
                                "[Minter {}][Tx {tx_hash}][Success] Funding `{account_address}`",
                                minter.account_address()
                            ),
                            Ok(Err(err)) => warn!(
                                "[Minter {}][Tx {tx_hash}][Failure] Funding `{account_address}`: {err}",
                                minter.account_address()
                            ),
                            Err(err) => warn!("{err}"),
                        },
                        _ = shutdown_token.cancelled() => {
                            debug!("Received shutdown notification. Minter {} stops funding account `{account_address}`", minter.account_address());
                            // Drop everything and complete the task.
                            return;
                        }
                    }

                // Return to [Delegate::Free] state.
                let mut guard = state.lock().expect("Delegate state must be lockable");
                {
                    // Debug scope.

                    // This [Delegate] must be in [Delegate::Busy] state.
                    let delegate = guard
                        .as_ref()
                        .expect("[BUG] Delegate state may not be empty");

                    debug_assert!(!delegate.is_free(), "Delegate with {minter} must be Busy");
                    debug!("[Minter {}] Getting Free", minter.account_address());
                }

                *guard = Some(Delegate::Free { minter });
            });
        }

        Some(Ok(tx_hash))
    }

    /// Check whether this [DelegateState] wraps a [Delegate::Free].
    fn is_free(&self) -> bool {
        self.state
            .lock()
            .expect("Delegate must be lockable")
            .as_ref()
            .expect("[BUG] DelegateState must always contain Delegate")
            .is_free()
    }
}

impl Debug for DelegateState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateState")
            .field("state", &format!("{:?}", self.state))
            .finish()
    }
}

/// [Delegate] can be either [Delegate::Free] or [Delegate::Busy].
/// Free delegates can pick up funding requests.
enum Delegate {
    /// [MinterAccount] mints to [AccountAddress]
    Busy {
        minter: MinterAccount,
        target_account: AccountAddress,
    },
    Free {
        minter: MinterAccount,
    },
}

impl Delegate {
    /// Check whether this instance is [Delegate::Free].
    fn is_free(&self) -> bool {
        match self {
            Delegate::Free { .. } => true,
            Self::Busy { .. } => false,
        }
    }
}

impl Debug for Delegate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy {
                minter,
                target_account,
            } => f
                .debug_struct("Delegate::Busy")
                .field("minter", minter)
                .field("target_account", target_account)
                .finish(),
            Self::Free { minter } => f
                .debug_struct("Delegate::Free")
                .field("minter", minter)
                .finish(),
        }
    }
}

impl Display for Delegate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
