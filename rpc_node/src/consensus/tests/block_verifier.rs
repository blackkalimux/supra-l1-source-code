#![allow(clippy::unwrap_used)]

use crate::consensus::block_store::block_store_tests::prepare_full_block;
use crate::consensus::{BlockVerifier, TBlockVerifier, VerificationResult};
use crate::RpcEpochChangeNotificationSchedule;
use async_trait::async_trait;
use certifier::test_utils::epoch_state::epoch_state;
use committees::{
    CertifiedBlock, GenesisStateProvider, Height, TBlockHeader, TCommittee, TSmrBlock,
};
use epoch_manager::{EpochNotificationAndAckSender, EpochStatesProvider};
use execution::test_utils::executor::ExecutorTestDataProvider;
use lifecycle::ChainId;
use lifecycle_types::{EpochState, EpochStates, TEpochStates};
use std::ops::{Deref, DerefMut};
use test_utils_2::certificate::ed25519_qc_from_keys;
use tokio::sync::mpsc::Receiver;
use types::settings::committee::extended_genesis_state_provider::ExtendedGenesisStateProvider;
use votes::VoteType;

pub(crate) struct TestDataProvider {
    provider: ExecutorTestDataProvider,
}

impl TestDataProvider {
    pub(crate) fn current_epoch_state(&self) -> EpochState {
        epoch_state(
            self.provider.validator_committee().clone(),
            None,
            None,
            Some(self.genesis_state_provider().genesis_parameters().into()),
        )
    }

    pub(crate) fn get_next_epoch_state(&self, height: Height) -> EpochState {
        epoch_state(
            self.provider.get_next_epoch_validator_set(height),
            None,
            None,
            Some(self.genesis_state_provider().genesis_parameters().into()),
        )
    }
}

#[async_trait(?Send)]
impl EpochStatesProvider<RpcEpochChangeNotificationSchedule> for TestDataProvider {
    fn get_chain_id(&self) -> ChainId {
        self.provider.genesis_state_provider().chain_id()
    }

    async fn get_epoch_states(&self, max_depth: usize) -> EpochStates {
        let mut states =
            EpochStates::new(max_depth).expect("Successful epoch-states initialization");
        states.insert(self.current_epoch_state());
        states
    }

    fn subscribe(
        &mut self,
        _id: RpcEpochChangeNotificationSchedule,
    ) -> Receiver<EpochNotificationAndAckSender> {
        unimplemented!("TestDataProvider does not provide subscription options yet.")
    }
}

impl Deref for TestDataProvider {
    type Target = ExecutorTestDataProvider;

    fn deref(&self) -> &Self::Target {
        &self.provider
    }
}

impl DerefMut for TestDataProvider {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.provider
    }
}

impl From<ExecutorTestDataProvider> for TestDataProvider {
    fn from(provider: ExecutorTestDataProvider) -> Self {
        Self { provider }
    }
}

#[tokio::test]
async fn check_block_verification() {
    let mut test_data_provider = TestDataProvider::from(ExecutorTestDataProvider::default());
    // Data from epoch 1
    let (epoch_1_certified_blocks, epoch_1_batches) = test_data_provider.generate_block_chain();
    // Prepare data with no-commit qc.
    let epoch_1_block_1 = epoch_1_certified_blocks[0].clone();
    let prepare_qc = ed25519_qc_from_keys(
        epoch_1_block_1.block(),
        test_data_provider.validator_committee().committee(),
        VoteType::PrepareNormal,
        test_data_provider.validator_consensus_keys(),
    )
    .expect("Successful QC creation");
    let block_wo_commit_qc = prepare_full_block(
        &CertifiedBlock::new(epoch_1_block_1.into_block(), prepare_qc),
        &epoch_1_batches[0],
    );

    // Move to epoch 2
    test_data_provider.move_to_next_epoch(
        epoch_1_certified_blocks
            .last()
            .expect("Non-empty blocks")
            .height(),
    );
    // Data from epoch 2
    let (epoch_2_certified_blocks, epoch_2_batches) = test_data_provider.generate_block_chain();

    // BlockVerifier is in epoch 2 and QC verification is skipped.
    let mut block_verifier = BlockVerifier::new(&test_data_provider, true).await;

    // Move to epoch 3
    test_data_provider.move_to_next_epoch(
        epoch_2_certified_blocks
            .last()
            .expect("Non-empty blocks")
            .height(),
    );
    // Data from epoch 3
    let (epoch_3_certified_blocks, epoch_3_batches) = test_data_provider.generate_block_chain();

    // Verification of the genesis block should fail.
    let genesis_block = test_data_provider
        .genesis_state_provider()
        .certified_block();
    let genesis_block = prepare_full_block(&genesis_block, &[]);
    assert!(matches!(
        block_verifier.verify(&genesis_block),
        VerificationResult::InvalidBlock(_)
    ));

    // Verification of the block-wo-commit-qc should fail
    assert!(matches!(
        block_verifier.verify(&block_wo_commit_qc),
        VerificationResult::InvalidBlock(_)
    ));

    // Verification of blocks from previous epoch should fail.
    assert!(matches!(
        block_verifier.verify(&prepare_full_block(
            &epoch_1_certified_blocks[0],
            &epoch_1_batches[0]
        )),
        VerificationResult::OldEpochBlock
    ));

    // Verification of new blocks from previous epoch should fail.
    assert!(matches!(
        block_verifier.verify(&prepare_full_block(
            &epoch_3_certified_blocks[0],
            &epoch_3_batches[0]
        )),
        VerificationResult::FutureEpochBlock
    ));

    let invalid_block_qc = CertifiedBlock::new(
        epoch_2_certified_blocks[1].block().clone(),
        epoch_1_certified_blocks[1].qc().clone(),
    );
    let invalid_block_qc = prepare_full_block(&invalid_block_qc, &epoch_2_batches[1]);
    // When qc verification is skipped, Block has invalid qc, but block is from current epoch and has commit-qc, verification is successful :(
    assert!(matches!(
        block_verifier.verify(&invalid_block_qc),
        VerificationResult::Success
    ));
    // It is the same for the valid block as well
    let epoch_2_block_1 = prepare_full_block(&epoch_2_certified_blocks[1], &epoch_2_batches[1]);
    assert!(matches!(
        block_verifier.verify(&epoch_2_block_1),
        VerificationResult::Success
    ));

    // Enable qc verification and check invalid_block_qc verification, now it should fail.
    block_verifier.skip_qc_verification = false;
    // When qc verification is skipped, Block has invalid qc, but block is from current epoch and has commit-qc, verification is successful :(
    assert!(matches!(
        block_verifier.verify(&invalid_block_qc),
        VerificationResult::InvalidBlock(_)
    ));
    // But valid block verification will succeed.
    assert!(matches!(
        block_verifier.verify(&epoch_2_block_1),
        VerificationResult::Success
    ));

    // Check end_epoch api
    assert!(!block_verifier.current_epoch_state().is_epoch_over());
    block_verifier.end_current_epoch();
    assert!(block_verifier.current_epoch_state().is_epoch_over());

    let epoch_3_state = test_data_provider.current_epoch_state();
    block_verifier.update(epoch_3_state);

    // Now epoch 3 data verification should succeed, and epoch 2 data verification will fail.
    assert!(matches!(
        block_verifier.verify(&prepare_full_block(
            &epoch_3_certified_blocks[0],
            &epoch_3_batches[0]
        )),
        VerificationResult::Success
    ));
    assert!(matches!(
        block_verifier.verify(&epoch_2_block_1),
        VerificationResult::OldEpochBlock
    ));

    // Now full block with inconsistent data
    assert!(matches!(
        block_verifier.verify(&prepare_full_block(
            &epoch_3_certified_blocks[0],
            &epoch_3_batches[1]
        )),
        VerificationResult::InvalidBlock(_)
    ));
    // Now full block with inconsistent data
    assert!(matches!(
        block_verifier.verify(&prepare_full_block(
            &epoch_3_certified_blocks[0],
            &epoch_3_batches[0][1..]
        )),
        VerificationResult::InvalidBlock(_)
    ));
}
