use crate::common::error::CliError;
use crate::node::delegation_pool::multisig_execute::MultisigExecuteStoredPayload;
use crate::node::delegation_pool::multisig_propose_set_operator::ProposeSetOperator;
use crate::node::delegation_pool::multisig_vote::MultisigVote;
use crate::rest_utils::action::CliTransactionInfo;
use clap::Subcommand;
use derive_pool_address::DerivePoolAddress;
use leave_validator_set::LeaveValidatorSet;
use multisig_propose_unlock_stake::ProposeUnlockStake;
use multisig_propose_withdraw_stake::ProposeWithdrawStake;
use serde::Serialize;
use set_operator::SetOperator;

pub mod derive_pool_address;
mod leave_validator_set;
mod multisig;
mod multisig_execute;
mod multisig_propose_set_operator;
mod multisig_propose_unlock_stake;
mod multisig_propose_withdraw_stake;
mod multisig_vote;
pub mod set_operator;

/// Delegation pool operations.
#[derive(Subcommand, Debug)]
pub enum DelegationPoolTool {
    /// Derive delegation pool address from owner address and creation seed.
    DerivePoolAddress(DerivePoolAddress),
    /// Removes the validator associated with the delegation pool managed by the current profile,
    /// from the active validator set. This change will take effect when the next epoch begins.
    LeaveValidatorSet(LeaveValidatorSet),
    /// Execute transaction payload for the given MultiSig address. Takes effect immediately
    /// if the payload successfully validated and executed.
    // TODO: MultiSig commands need to be placed into their own module
    MultisigExecuteStoredPayload(MultisigExecuteStoredPayload),
    /// Vote for a Set new operator transaction via MultiSig for pool managed by current profile.
    MultisigVote(MultisigVote),
    /// Set new operator for pool owned by current profile. Take effect immediately.
    SetOperator(SetOperator),
    /// Proposes a Set New Operator transaction via MultiSig for pool managed by the current profile.
    ProposeSetOperator(ProposeSetOperator),
    /// Proposes a multisig transaction to unlock stake in the delegation pool managed by the
    /// current profile.
    ProposeUnlockStake(ProposeUnlockStake),
    /// Proposes a multisig transaction to withdraw stake from the delegation pool managed by the
    /// current profile.
    ProposeWithdrawStake(ProposeWithdrawStake),
}

impl DelegationPoolTool {
    pub async fn execute(self) -> Result<DelegationPoolResult, CliError> {
        match self {
            DelegationPoolTool::DerivePoolAddress(args) => args.execute(),
            DelegationPoolTool::LeaveValidatorSet(args) => args.execute().await,
            DelegationPoolTool::MultisigExecuteStoredPayload(args) => args.execute().await,
            DelegationPoolTool::MultisigVote(args) => args.execute().await,
            DelegationPoolTool::ProposeSetOperator(args) => args.execute().await,
            DelegationPoolTool::ProposeUnlockStake(args) => args.execute().await,
            DelegationPoolTool::ProposeWithdrawStake(args) => args.execute().await,
            DelegationPoolTool::SetOperator(args) => args.execute().await,
        }
    }
}

// TODO: This should be refactored. A simple `Transaction` variant can cover all
// `Hash` return values.
#[derive(Serialize)]
pub enum DelegationPoolResult {
    DerivePoolAddress(String),
    TransactionInfo(Box<CliTransactionInfo>),
}

impl std::fmt::Display for DelegationPoolResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DelegationPoolResult::DerivePoolAddress(res) => write!(f, "{}", res),
            DelegationPoolResult::TransactionInfo(res) => write!(f, "{:?}", res),
        }
    }
}
