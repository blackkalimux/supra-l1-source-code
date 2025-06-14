use committees::TBlockHeader;
use epoch_manager::EpochStatesProvider;
use lifecycle::TEpochId;
use lifecycle_types::{EpochState, EpochStates, TEpochStates, TEpochStatesWrite};
use rpc::FullBlock;
use std::cmp::Ordering;
use std::error::Error;
use traits::Verifier;

/// Verification result of block by block verifier.
#[derive(Clone)]
pub enum VerificationResult {
    /// Verification of the block is successful.
    Success,
    /// Block is identified to be from old epoch compared with available epoch data.
    OldEpochBlock,
    /// Block is identified to be from future epoch compared with available epoch data.
    FutureEpochBlock,
    /// Block verification failed.
    InvalidBlock(String),
}

impl VerificationResult {
    fn report_byzantine_behaviour(
        msg: &str,
        full_block: &FullBlock,
        error: Option<&dyn Error>,
    ) -> VerificationResult {
        VerificationResult::InvalidBlock(format!(
            "Potential Byzantine Block-Provider: {msg}.\n\
                     Block: {full_block:?}\n\
                     Cause: {error:?} \n\
                     PLEASE REPORT THIS ERROR TO THE SUPRA SUPPORT TEAM IMMEDIATELY!!!.\n\
                     Try to connect to a new provider to ensure that your node remains safe."
        ))
    }
}

/// Epoch data aware block verifier interface for incoming blocks verification utilized in scope
/// of [ChainStateAssembler].
pub trait TBlockVerifier: TEpochStates + TEpochStatesWrite {
    /// Verifies input block.
    fn verify(&self, block: &FullBlock) -> VerificationResult;
    /// Update verifier with new epoch data.
    fn update(&mut self, new_epoch_state: EpochState) {
        self.epoch_states_mut().insert(new_epoch_state);
    }

    fn end_current_epoch(&mut self) {
        self.epoch_states_mut().current_mut().end_epoch()
    }
}

/// Verifies block against current epoch.
/// Currently only single epoch verification is supported.
/// Verification fails if:
///     - block is from lower or higher epoch than current
///     - block is not consistent, i.e. provided batch info does not match the block-batch references.
///     - block is not commit-certified
///     - block certification verification fails
///
/// Block certificate verification is optional and is done only if it is enabled.
pub struct BlockVerifier {
    /// Current epoch state(s) known by verifier.
    epoch_states: EpochStates,
    /// Flag indicating whether the block certification is enabled or not.
    skip_qc_verification: bool,
}

impl BlockVerifier {
    pub const EPOCHS_TO_RETAIN: usize = 1;

    /// Creates [BlockVerifier] instance.
    pub async fn new<SubscriptionId>(
        epoch_state_provider: &impl EpochStatesProvider<SubscriptionId>,
        skip_block_verification: bool,
    ) -> Self {
        Self {
            epoch_states: epoch_state_provider
                .get_epoch_states(Self::EPOCHS_TO_RETAIN)
                .await,
            skip_qc_verification: skip_block_verification,
        }
    }
}

impl TBlockVerifier for BlockVerifier {
    fn verify(&self, full_block: &FullBlock) -> VerificationResult {
        // The RPC server should never send its clients the root block, since it should be generated
        // by the node from genesis-blob. Ensure that Byzantine servers cannot cause clients
        // to crash via a subtraction from 0 by doing this.
        if full_block.height() == 0 {
            return VerificationResult::report_byzantine_behaviour(
                "Received root block",
                full_block,
                None,
            );
        }
        // The block provider is not trusted. Ensure that the block is valid under the current
        // [Committee] and report an error if it is not, since this would indicate either that a critical
        // bug has been introduced into the consensus or that the Byzantine threshold has been
        // exceeded.
        if !full_block.base.is_commit_certified() {
            return VerificationResult::report_byzantine_behaviour(
                "The block provider sent a block that is not commit-certified",
                full_block,
                None,
            );
        }

        let current_epoch = self.current_committee().epoch();
        match full_block.epoch().cmp(&current_epoch) {
            Ordering::Less => return VerificationResult::OldEpochBlock,
            Ordering::Greater => return VerificationResult::FutureEpochBlock,
            Ordering::Equal => {
                if let Err(msg) = full_block.check_consistency() {
                    return VerificationResult::report_byzantine_behaviour(
                        msg.as_str(),
                        full_block,
                        None,
                    );
                }
                if self.skip_qc_verification {
                    return VerificationResult::Success;
                }
            }
        }

        if let Err(e) = self.current_committee().verify(&full_block.base) {
            return VerificationResult::report_byzantine_behaviour(
                "Safety Failure: Received a block with an invalid Commit Certificate.\n\
                      WARNING: Your block provider may be malicious! ",
                full_block,
                Some(&e),
            );
        }

        VerificationResult::Success
    }
}

impl TEpochStates for BlockVerifier {
    fn epoch_states(&self) -> &EpochStates {
        &self.epoch_states
    }
}

impl TEpochStatesWrite for BlockVerifier {
    fn epoch_states_mut(&mut self) -> &mut EpochStates {
        &mut self.epoch_states
    }
}

#[cfg(test)]
#[path = "tests/block_verifier.rs"]
pub(crate) mod block_verifier_tests;
