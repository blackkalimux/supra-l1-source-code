#![allow(deprecated)]

use super::api_server_error::{ApiServerError, PublicApiServerError};
use crate::gas::AggregatedGasPriceReader;
use crate::rest::faucet::Faucet;
use crate::rest::faucet_call_limiter::FaucetCallLimiter;
use crate::rest::move_list::v3::{MoveListBuilder, MoveListV3};
use crate::rest::rest_call_limiter::RestCallLimiter;
use crate::rest::Converter;
use crate::transactions::dispatcher::TransactionDispatcher;
use crate::RpcStorage;
use anyhow::anyhow;
use aptos_api_types::{Address, MoveResource, ResourceGroup};
use aptos_types::account_address::AccountAddress;
use aptos_types::state_store::state_key::StateKey;
use aptos_types::state_store::TStateView;
use aptos_types::transaction::SignedTransaction;
use aptos_types::transaction::TransactionPayload as MoveTransactionPayload;
use aptos_vm::AptosSimulationVM;
use archive::schemas::coin_transactions::TaggedTransactionHash;
use committees::CommitteeAuthorization;
use committees::{CertifiedBlock, Height, TBlockHeader, TSmrBlock};
use execution::MoveStore;
use lifecycle::{ChainId, Epoch};
use mempool::MoveSequenceNumberProvider;
use move_core_types::language_storage::StructTag;
use rocksdb::{IteratorMode, ReadOptions};
use rocksstore::chain_storage::block_store::{ChainStore, StoreError};
use socrypto::{Digest, Hash};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::RwLock as AsyncRwLock;
use traits::Storable;
use transactions::{
    SequenceNumber, SmrTransactionHeader, SmrTransactionPayload, TSignerData, TTransactionHeader,
    TxExecutionStatus,
};
use types::api::v1::MoveListQuery;
use types::api::v2::BlockHeaderInfo;
use types::api::v3::events::EventWithContext;
use types::api::v3::execution_statistics::ExecutedBlockStats;
use types::api::v3::{BlockMetadataInfo, EventsV3, Transaction};
use types::api::{
    v1::{ConsensusBlock, Events, TransactionAuthenticator, TransactionOutput, TransactionPayload},
    v2::{Block, TransactionInfoV2},
    TransactionType,
};
use types::automation::{AutomatedTransactionMeta, AutomationTask};
use types::batches::batch::{PackedBatchData, PackedSmrBatch};
use types::transactions::smr_transaction::{SmrMoveTransaction, SmrTransaction};
use types::transactions::transaction_output::TransactionOutput as SerializedOutput;
use types::transactions::MoveTransactionOutput;
use types::TypeTag;

#[derive(Clone)]
/// State shared across web server worker threads in the regular mode.
pub struct ApiServerState {
    pub(crate) storage: RpcStorage<MoveSequenceNumberProvider>,
    pub(crate) move_store: MoveStore,
    pub(crate) store: ChainStore,
    pub(crate) tx_sender: TransactionDispatcher,
    pub(crate) gas_price_reader: AggregatedGasPriceReader,
    pub(crate) rest_call_limiter: Option<Arc<RwLock<RestCallLimiter>>>,
    pub(crate) faucet_state: Option<FaucetState>,
    /// Chain id retrieved  from `Self::move_store` and cached.
    /// Chain id is not expected to be changed throughout lifetime of a chain/network instance.
    pub(crate) chain_id: ChainId,
}

#[derive(Clone)]
/// Convenience struct to address [Faucet].
pub struct FaucetState {
    pub(crate) faucet: Arc<AsyncRwLock<Faucet>>,
    pub(crate) faucet_call_limiter: Arc<AsyncRwLock<FaucetCallLimiter>>,
}

impl FaucetState {
    pub fn new(faucet: Faucet, faucet_call_limiter: FaucetCallLimiter) -> Self {
        Self {
            faucet: Arc::new(AsyncRwLock::new(faucet)),
            faucet_call_limiter: Arc::new(AsyncRwLock::new(faucet_call_limiter)),
        }
    }

    pub fn empty(faucet_per_request: u64, faucet_call_limiter: FaucetCallLimiter) -> Self {
        Self {
            faucet: Arc::new(AsyncRwLock::new(Faucet::new_with_no_delegates(
                faucet_per_request,
            ))),
            faucet_call_limiter: Arc::new(AsyncRwLock::new(faucet_call_limiter)),
        }
    }

    /// Set faucet.
    pub async fn set_faucet(&self, faucet: Faucet) {
        *self.faucet.write().await = faucet;
    }
}

impl ApiServerState {
    pub fn chain_id_header_value(&self) -> String {
        self.chain_id.to_string()
    }

    pub fn converter(&self) -> Converter {
        Converter::new(&self.move_store)
    }

    pub fn set_faucet_state(&mut self, faucet_state: &FaucetState) {
        self.faucet_state = Some(faucet_state.clone());
    }

    /// Check if this [ApiServerStateCommon] offers [Faucet] services.
    pub(crate) fn is_faucet_enabled(&self) -> bool {
        self.faucet_state.is_some()
    }

    /// Constructs the API [Block] for the corresponding height from the data available in the
    /// archive if a block has been committed for the given [Height]. This block will contain
    /// the transactions that were finalized when the corresponding [SmrBlock] was committed along
    /// with the automated transactions which where valid by the time of the block execution.
    pub(crate) fn get_block_by_height(
        &self,
        height: Height,
        with_finalized_transactions: bool,
        txn_type: Option<TransactionType>,
    ) -> Result<Option<Block>, PublicApiServerError> {
        let block =
            self.get_block_by_height_internal(height, with_finalized_transactions, txn_type)?;
        Ok(block)
    }

    /// Returns the [TransactionInfoV2]s for all finalized (i.e., consensus-processed) [SignedTransaction]s
    /// associated with the given [AccountAddress].
    ///
    /// If `start` is specified, the result set will only include [TransactionInfoV2]s starting from it inclusive.
    pub(crate) fn get_account_user_transactions(
        &self,
        account: AccountAddress,
        start_sequence_number: Option<SequenceNumber>,
        max: usize,
    ) -> Result<Vec<TransactionInfoV2>, PublicApiServerError> {
        // Get the hashes of the transactions associated with the given account.
        let hashes = self
            .storage
            .archive()
            .reader()
            .get_transaction_hashes_by_account(account, start_sequence_number, max)
            .map_err(ApiServerError::from)?;

        let transactions =
            self.get_account_executed_transactions_by_hashes_internal(account, hashes)?;
        Ok(transactions)
    }

    /// Returns the API [TransactionInfoV2]s for all [SignedTransaction]s associated with the given
    /// [AccountAddress].
    ///
    /// If `start` is specified, the result set will only include [TransactionInfoV2]s starting from it inclusive.
    pub(crate) fn get_account_coin_transaction_hashes(
        &self,
        account: AccountAddress,
        start: Option<u64>,
        max: usize,
    ) -> Result<(Vec<TaggedTransactionHash>, u64), PublicApiServerError> {
        let result = self
            .storage
            .archive()
            .reader()
            .get_coin_transaction_hashes_by_account(account, start, max)
            .map_err(ApiServerError::from)?;
        Ok(result)
    }

    /// Returns the [`TransactionInfoV2`]s for all automated transactions
    /// associated with the given [`AccountAddress`] which where valid for it during the specified
    /// range of the blocks.
    ///
    /// If `cursor` is specified, the result set will include [`TransactionInfoV2`]s starting from it exclusive,
    /// and start block height will be ignored.
    /// Else if `start_block_height` is specified, the result set will include [`TransactionInfoV2`]s starting the specified block inclusive.
    /// If no starting point is specified the lookup will be started from the latest available entry for the account.
    pub(crate) fn get_account_automated_transactions(
        &self,
        account: AccountAddress,
        start_block_height: Option<Height>,
        cursor: Option<Vec<u8>>,
        ascending: bool,
        max: usize,
    ) -> Result<(Vec<TransactionInfoV2>, Option<Vec<u8>>), PublicApiServerError> {
        // Get the hashes of the transactions associated with the given account.
        let (hashes, cursor) = self
            .storage
            .archive()
            .reader()
            .get_automated_transaction_hashes_by_account(
                account,
                start_block_height,
                cursor,
                ascending,
                max,
            )
            .map_err(ApiServerError::from)?;

        let transactions = self.get_account_automated_transactions_by_hashes(account, hashes)?;
        Ok((transactions, cursor))
    }

    /// Returns the API [Transaction]s for all type of transactions associated with the given
    /// [AccountAddress].
    ///
    /// If `start` is specified, the result set will only include [Transaction]s starting from it inclusive.
    pub(crate) fn get_account_coin_transactions(
        &self,
        account: AccountAddress,
        start: Option<u64>,
        max: usize,
    ) -> Result<(Vec<Transaction>, u64), PublicApiServerError> {
        let (tagged_hashes, cursor) =
            self.get_account_coin_transaction_hashes(account, start, max)?;
        let transactions = tagged_hashes
            .into_iter()
            .map(|tagged_txn| match tagged_txn {
                TaggedTransactionHash::BlockMetadata(meta) => self
                    .get_account_block_metadata_info_by_hash(account, meta)
                    .map(Transaction::from),
                TaggedTransactionHash::User(user) => self
                    .get_account_executed_transaction_by_hash_internal(account, user)
                    .map(Transaction::from),
                TaggedTransactionHash::Automated(auto) => self
                    .get_account_automated_transaction_by_hash(account, auto)
                    .map(Transaction::from),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((transactions, cursor))
    }

    /// Returns the [MoveResource] with the given [StructTag], if the given [AccountAddress]
    /// owns a resource of that type.
    pub fn get_move_resource(
        &self,
        account_address: AccountAddress,
        struct_tag: &StructTag,
    ) -> Result<Option<MoveResource>, PublicApiServerError> {
        let converter = self.converter();
        let address = Address::from(account_address);

        let Some(bytes) = converter
            .find_resource(&self.move_store, address, struct_tag)
            .map_err(ApiServerError::from)?
        else {
            return Ok(None);
        };

        let resource = converter
            .try_into_resource(struct_tag, &bytes)
            .map_err(ApiServerError::from)?;
        Ok(Some(resource))
    }

    /// Return [StructTag] under a resource group
    pub fn read_resource_group(
        &self,
        address: &AccountAddress,
        struct_tag: &StructTag,
    ) -> Option<Vec<(StructTag, Vec<u8>)>> {
        let state_key = StateKey::resource_group(address, struct_tag);
        self.move_store
            .get_state_value_bytes(&state_key)
            .ok()?
            .map(|bytes| {
                bcs::from_bytes::<ResourceGroup>(&bytes).map(|v| v.into_iter().collect::<Vec<_>>())
            })?
            .ok()
    }

    /// Constructs the API [TransactionInfoV2] for the [SmrTransaction] or [AutomatedTransaction]
    /// with the given [Hash], if the transaction has been stored in the archive.
    /// If the transaction has been finalized, the [TransactionInfoV2] includes the output produced
    /// during the execution that finalized the transaction.
    pub fn get_transaction_by_hash(
        &self,
        hash: Hash,
        ongoing: bool,
    ) -> Result<Option<TransactionInfoV2>, PublicApiServerError> {
        self.get_transaction_by_hash_internal(hash, ongoing)
            .map_err(PublicApiServerError::from)
    }

    /// Constructs the API [TransactionInfo] for the [SmrTransaction] or [AutomatedTransaction]
    /// with the given [Hash], if the transaction has been stored in the archive.
    /// If the transaction has been finalized, the [TransactionInfo] includes the output produced
    /// during the execution that finalized the transaction.
    fn get_transaction_by_hash_internal(
        &self,
        hash: Hash,
        ongoing: bool,
    ) -> Result<Option<TransactionInfoV2>, ApiServerError> {
        let maybe_user_txn = self.get_user_transaction_by_hash_internal(hash, ongoing);
        match maybe_user_txn {
            Ok(Some(txn)) => Ok(Some(txn)),
            Ok(None) => Err(ApiServerError::Other(anyhow!("Invalid internal state"))),
            Err(ApiServerError::TransactionNotFound(_)) => {
                self.get_automated_transaction_by_hash_internal(hash, ongoing)
            }
            Err(err) => Err(err),
        }
    }

    /// Constructs the API [TransactionInfoV2] for the [SmrTransaction] with the given [Hash], if
    /// the transaction has been stored in the archive. If the transaction has been finalized,
    /// the [TransactionInfoV2] includes the output produced during the execution that finalized
    /// the transaction.
    pub(crate) fn get_user_transaction_by_hash(
        &self,
        txn_hash: Hash,
        ongoing: bool,
    ) -> Result<Option<TransactionInfoV2>, PublicApiServerError> {
        self.get_user_transaction_by_hash_internal(txn_hash, ongoing)
            .map_err(PublicApiServerError::from)
    }

    /// Constructs the API [TransactionInfoV2] for the [SmrTransaction] with the given [Hash], if
    /// the transaction has been stored in the archive. If the transaction has been finalized,
    /// the [TransactionInfoV2] includes the output produced during the execution that finalized
    /// the transaction.
    fn get_user_transaction_by_hash_internal(
        &self,
        txn_hash: Hash,
        ongoing: bool,
    ) -> Result<Option<TransactionInfoV2>, ApiServerError> {
        let transaction = if ongoing {
            self.get_transient_transaction_by_hash_internal(txn_hash, true)?
        } else {
            self.get_executed_transaction_by_hash_internal(txn_hash, true)?
        };

        Ok(transaction)
    }

    /// Constructs the API [TransactionInfo] for the [AutomatedTransaction] with the given [Hash], if
    /// the transaction has been stored in the archive.
    pub(crate) fn get_automated_transaction_by_hash(
        &self,
        hash: Hash,
    ) -> Result<Option<TransactionInfoV2>, PublicApiServerError> {
        self.get_automated_transaction_by_hash_internal(hash, true)
            .map_err(PublicApiServerError)
    }

    fn get_block_by_height_internal(
        &self,
        height: Height,
        with_finalized_transactions: bool,
        txn_type: Option<TransactionType>,
    ) -> Result<Option<Block>, ApiServerError> {
        // Get the block header.
        let Some(header) = self
            .storage
            .archive()
            .reader()
            .get_block_header_info_by_height(height)?
        else {
            return Ok(None);
        };

        if !with_finalized_transactions {
            return Ok(Some(Block::new(header, None)));
        }

        let transaction_hashes = match &txn_type {
            None | Some(TransactionType::User) => {
                // Get the hashes of the transactions included in the block. Note that this currently only
                // includes the transactions that produced finalized statuses when the block was executed.
                self.storage
                    .archive()
                    .reader()
                    .get_txs_by_block_height(height)?
            }
            _ => vec![],
        };

        // Get the hashes of the automated transactions executed as part of the block.
        // This includes only the transactions that produced finalized execution statuses.
        let automated_txn_hashes = match &txn_type {
            None | Some(TransactionType::Auto) => {
                // Get the hashes of the transactions included in the block. Note that this currently only
                // includes the transactions that produced finalized statuses when the block was executed.
                self.storage
                    .archive()
                    .reader()
                    .get_automated_txs_by_block_height(height)?
            }
            _ => vec![],
        };

        // Get the [SmrTransactions] and their [TransactionOutputs].
        let mut transactions =
            self.get_block_transactions_by_hashes(header.hash, transaction_hashes)?;

        // Get the automated transactions API info.
        let automated_txns =
            self.get_block_automated_transactions_by_hashes(header.hash, automated_txn_hashes)?;
        transactions.extend(automated_txns);

        Ok(Some(Block::new(header, Some(transactions))))
    }

    fn get_block_automated_transactions_by_hashes(
        &self,
        block: Hash,
        hashes: Vec<Hash>,
    ) -> Result<Vec<TransactionInfoV2>, ApiServerError> {
        let mut transactions = Vec::new();
        for h in hashes {
            let Some(info) = self.get_automated_transaction_by_hash_internal(h, false)? else {
                // This branch will only be executed if the code has regressed. The archive should
                // only have the hash of a transaction if it also has the transaction itself.
                return Err(ApiServerError::MissingBlockAutomatedTransaction(h, block));
            };

            Self::check_move_payload_type(&info, || {
                format!("A Move transaction hash {h} of block {block} has a non-Move transaction associated with it.")
            })?;

            transactions.push(info);
        }

        Ok(transactions)
    }

    fn get_block_transactions_by_hashes(
        &self,
        block: Hash,
        hashes: Vec<Hash>,
    ) -> Result<Vec<TransactionInfoV2>, ApiServerError> {
        let mut transactions = Vec::new();
        for h in hashes {
            let Some(info) = self.get_executed_transaction_by_hash_internal(h, false)? else {
                // This branch will only be executed if the code has regressed. The archive should
                // only have the hash of a transaction if it also has the transaction itself.
                return Err(ApiServerError::MissingBlockTransaction(h, block));
            };

            if info.output().is_none() {
                // This branch will only be executed if the code has regressed. The archive should
                // only have the hash of the transaction matched to this [BlockHeaderInfo] if the
                // transaction produced a [TransactionOutput] when the block was executed.
                return Err(ApiServerError::MissingBlockTransactionOutput(h, block));
            }

            transactions.push(info);
        }
        Ok(transactions)
    }

    /// Returns transactions corresponding to the input list of the hashes.
    /// Transactions are looked in to the archive index of user-submitted transactions.
    pub(crate) fn get_account_executed_transactions_by_hashes(
        &self,
        account: AccountAddress,
        hashes: Vec<Hash>,
    ) -> Result<Vec<TransactionInfoV2>, PublicApiServerError> {
        self.get_account_executed_transactions_by_hashes_internal(account, hashes)
            .map_err(PublicApiServerError)
    }

    /// Returns transactions corresponding to the input list of the hashes.
    /// Transactions are looked in to the archive index of user-submitted transactions.
    fn get_account_executed_transactions_by_hashes_internal(
        &self,
        account: AccountAddress,
        hashes: Vec<Hash>,
    ) -> Result<Vec<TransactionInfoV2>, ApiServerError> {
        hashes
            .into_iter()
            .map(|txn_hash| {
                self.get_account_executed_transaction_by_hash_internal(account, txn_hash)
            })
            .collect::<Result<Vec<TransactionInfoV2>, ApiServerError>>()
    }

    /// Returns user transaction corresponding to the input hash for the account.
    fn get_account_executed_transaction_by_hash_internal(
        &self,
        account: AccountAddress,
        hash: Hash,
    ) -> Result<TransactionInfoV2, ApiServerError> {
        let Some(info) = self.get_executed_transaction_by_hash_internal(hash, true)? else {
            // This branch will only be executed if the code has regressed. The archive should
            // only have the hash of a transaction if it also has the transaction itself.
            return Err(ApiServerError::MissingAccountTransaction(hash, account));
        };

        Self::check_move_payload_type(&info, || {
            format!("A Move account address {account} has a non-Move transaction with {hash} hash associated with it.")
        })?;
        Ok(info)
    }

    /// Returns automated transactions corresponding to the input list of the hashes.
    /// Transactions are looked in to the archive indexes of automated transactions.
    fn get_account_automated_transactions_by_hashes(
        &self,
        account: AccountAddress,
        hashes: Vec<Hash>,
    ) -> Result<Vec<TransactionInfoV2>, ApiServerError> {
        hashes
            .into_iter()
            .map(|txn_hash| self.get_account_automated_transaction_by_hash(account, txn_hash))
            .collect::<Result<Vec<TransactionInfoV2>, ApiServerError>>()
    }

    /// Returns automated transaction corresponding to the input hash and account address.
    /// Transactions are looked in to the archive indexes of automated transactions.
    fn get_account_automated_transaction_by_hash(
        &self,
        account: AccountAddress,
        hash: Hash,
    ) -> Result<TransactionInfoV2, ApiServerError> {
        let Some(info) = self.get_automated_transaction_by_hash_internal(hash, true)? else {
            // This branch will only be executed if the code has regressed. The archive should
            // only have the hash of a transaction if it also has the transaction itself.
            return Err(ApiServerError::MissingAccountAutomatedTransaction(
                hash, account,
            ));
        };

        Self::check_move_payload_type(&info, || {
            format!("A Move account address {account} has a non-Move transaction with {hash} hash associated with it.")
        })?;
        Ok(info)
    }

    /// Returns block metadata transaction info corresponding to the input hash and account address.
    fn get_account_block_metadata_info_by_hash(
        &self,
        account: AccountAddress,
        hash: Hash,
    ) -> Result<BlockMetadataInfo, ApiServerError> {
        self.get_block_metadata_by_hash_internal(hash, true)
            .and_then(|v| v.ok_or_else(|| ApiServerError::MissingAccountTransaction(hash, account)))
    }

    /// Read from both memory backlog and archive.
    ///
    /// The transaction status can be
    ///  - Not exist: neither in memory backlog nor in archive.
    ///  - Ongoing (`Pending`): found in memory backlog, ignore status in archive.
    ///  - Executed maybe finalized later (`PendingAfterExecution`): NOT in memory backlog,
    ///    and only status saved in archive, but without execution output and block info in archive.
    ///  - Finalized (`Invalid/Fail/Success`): NOT in memory backlog, status/output/blockinfo all saved in archive.
    ///
    /// NOTE: If same transaction hash executed before, and here comes a duplicate transaction in backlog,
    /// user will see the transaction is `Pending` from `transaction_by_hash` API which must be the only one providing transient state.
    ///
    /// To be sure if the same transaction has been executed before, a separate api should be provided to query only from database.
    /// Also the other APIs that return transaction related data should use the function [Self::get_executed_transaction_by_hash_internal]
    /// to avoid returning transient state.
    fn get_transient_transaction_by_hash_internal(
        &self,
        txn_hash: Hash,
        with_block_data: bool,
    ) -> Result<Option<TransactionInfoV2>, ApiServerError> {
        let Some(transaction) = self.storage.get_transaction_by_hash(txn_hash)? else {
            // Neither exist in the memory backlog nor in the archive.
            return Err(ApiServerError::TransactionNotFound(txn_hash));
        };
        let Some(status) = self.storage.get_transaction_status(txn_hash)? else {
            return Err(ApiServerError::MissingTransactionStatus(txn_hash));
        };

        let (maybe_output, maybe_block_info) = if status == TxExecutionStatus::Pending
            || status == TxExecutionStatus::PendingAfterExecution
        {
            // See the docs that explain why we do not assert the absence of output and block info in archive.
            (None, None)
        } else {
            let (output, block_info) =
                self.get_finalized_transaction_output_and_block_info(txn_hash, with_block_data)?;
            (output, block_info)
        };

        let info = self.create_transaction_info(
            txn_hash,
            transaction,
            maybe_output,
            status,
            maybe_block_info,
        )?;

        Ok(Some(info))
    }

    /// Finalized transaction should **Eventually** have output and block info, but due to
    /// [TransactionInfoV2] is constructed from 4 tables while their cache update is not atomic, so
    /// it is possible that the output and block info is not available even though transaction status
    /// return a finalized status.
    ///
    /// Due to the above reason, from user perspective, the transaction is not finalized
    /// until the output and block info is available.
    fn get_finalized_transaction_output_and_block_info(
        &self,
        txn_hash: Hash,
        with_block_data: bool,
    ) -> Result<(Option<SerializedOutput>, Option<BlockHeaderInfo>), ApiServerError> {
        let output = self.storage.archive().reader().get_txn_output(txn_hash)?;
        let block_hash = self
            .storage
            .archive()
            .reader()
            .get_block_hash_by_txn_hash(txn_hash)
            .map_err(ApiServerError::ArchiveError)
            .and_then(|maybe_block_hash| {
                maybe_block_hash.ok_or(ApiServerError::TransactionNotFound(txn_hash))
            })?;
        let block_info = with_block_data
            .then_some(
                self.storage
                    .archive()
                    .reader()
                    .get_block_header_info(block_hash)?,
            )
            .flatten();
        Ok((output, block_info))
    }

    /// Looks for  user-submitted & executed transaction.
    /// Always read from archive, skip reading from the memory backlog.
    fn get_executed_transaction_by_hash_internal(
        &self,
        txn_hash: Hash,
        with_block_data: bool,
    ) -> Result<Option<TransactionInfoV2>, ApiServerError> {
        let Some(transaction) = self.storage.archive().reader().get_transaction(txn_hash)? else {
            return Err(ApiServerError::TransactionNotFound(txn_hash));
        };
        let Some(status) = self
            .storage
            .archive()
            .reader()
            .get_transaction_status(txn_hash)?
        else {
            return Err(ApiServerError::MissingTransactionStatus(txn_hash));
        };

        let (output, block_info) =
            self.get_finalized_transaction_output_and_block_info(txn_hash, with_block_data)?;

        let info =
            self.create_transaction_info(txn_hash, transaction, output, status, block_info)?;

        Ok(Some(info))
    }

    // Always read from archive as automated transactions are internal and never become part of the backlog.
    fn get_automated_transaction_by_hash_internal(
        &self,
        hash: Hash,
        with_block_data: bool,
    ) -> Result<Option<TransactionInfoV2>, ApiServerError> {
        let Some((txn_meta, ref_task)) = self
            .storage
            .archive()
            .reader()
            .get_automated_transaction_data(hash)?
        else {
            return Err(ApiServerError::TransactionNotFound(hash));
        };
        let Some(status) = self
            .storage
            .archive()
            .reader()
            .get_transaction_status(hash)?
        else {
            return Err(ApiServerError::MissingTransactionStatus(hash));
        };

        let (output, block_info) =
            self.get_finalized_transaction_output_and_block_info(hash, with_block_data)?;
        // Automated transaction should always have output if it is present in the index table.
        if output.is_none() {
            return Err(ApiServerError::MissingAutomatedTransactionOutput(hash));
        }

        let info =
            self.create_automated_transaction_info(txn_meta, ref_task, output, status, block_info)?;

        Ok(Some(info))
    }

    /// Executed transaction info must include output, status and block info.
    fn create_automated_transaction_info(
        &self,
        txn_meta: AutomatedTransactionMeta,
        ref_task: AutomationTask,
        output: Option<SerializedOutput>,
        status: TxExecutionStatus,
        maybe_block_data: Option<BlockHeaderInfo>,
    ) -> Result<TransactionInfoV2, ApiServerError> {
        let (task_idx, sender, gas_unit_price, _block_height, hash) = txn_meta.into_inner();
        let header = SmrTransactionHeader::new(
            sender,
            task_idx,
            gas_unit_price,
            *ref_task.max_gas_amount(),
            *ref_task.expiry_time(),
            self.chain_id,
        );
        let converter = self.converter();
        let deserialized_payload = converter.try_into_transaction_payload(
            MoveTransactionPayload::EntryFunction(ref_task.payload().clone()),
        )?;
        let authenticator = TransactionAuthenticator::Automation(*ref_task.tx_hash());
        let api_output = Self::api_move_transaction_output(converter, output)?;
        let info = TransactionInfoV2::new(
            authenticator,
            maybe_block_data,
            hash,
            header,
            TransactionPayload::Move(deserialized_payload),
            api_output,
            status,
        );
        Ok(info)
    }

    /// Executed transaction info must include output, status and block info.
    /// Ongoing transactiono will have no output and block info, but the status should be pending.
    /// If transaction neither exist in the memory backlog nor in the archive, should not call this function.
    fn create_transaction_info(
        &self,
        hash: Hash,
        transaction: SmrTransaction,
        maybe_output: Option<SerializedOutput>,
        status: TxExecutionStatus,
        maybe_block_data: Option<BlockHeaderInfo>,
    ) -> Result<TransactionInfoV2, ApiServerError> {
        let info = match transaction {
            SmrTransaction::Move { inner: t, .. } => Self::move_transaction_info(
                self.converter(),
                hash,
                *t,
                maybe_block_data,
                maybe_output,
                status,
            )?,
            SmrTransaction::Supra { inner: t, .. } => {
                let signature_data = t.signer_data().clone();
                let header = t.header().clone();

                match SmrTransactionPayload::from(*t) {
                    SmrTransactionPayload::Dkg(payload) => TransactionInfoV2::new(
                        TransactionAuthenticator::Dkg(Box::new(signature_data)),
                        maybe_block_data,
                        hash,
                        header,
                        TransactionPayload::Dkg(*payload),
                        maybe_output.map(|o| TransactionOutput::Dkg(o.execution_status())),
                        status,
                    ),
                    SmrTransactionPayload::Oracle(payload) => {
                        TransactionInfoV2::new(
                            TransactionAuthenticator::Oracle(Box::new(signature_data)),
                            maybe_block_data,
                            hash,
                            header,
                            TransactionPayload::Oracle(*payload),
                            // TODO: Not currently processing any Oracle transactions.
                            // The internal [TransactionOutput] type will need to be updated when
                            // execution logic is added if we intend to process them separately.
                            maybe_output.map(|o| TransactionOutput::Oracle(o.execution_status())),
                            status,
                        )
                    }
                }
            }
        };
        Ok(info)
    }

    pub(crate) fn move_transaction_info(
        converter: Converter,
        hash: Hash,
        transaction: SmrMoveTransaction,
        maybe_block_data: Option<BlockHeaderInfo>,
        maybe_transaction_output: Option<SerializedOutput>,
        status: TxExecutionStatus,
    ) -> Result<TransactionInfoV2, ApiServerError> {
        // Make the [MoveConverter] used to convert stored Move types into their API representations.
        // Note that we do not instantiate these when the type is constructed because the resolver loads
        // on-chain config, which may change during the lifetime of the chain.

        let move_transaction = transaction.data();
        let deserialized_payload =
            converter.try_into_transaction_payload(move_transaction.payload().clone())?;

        // Deserialize the output if any was given.
        let maybe_move_output =
            Self::api_move_transaction_output(converter, maybe_transaction_output)?;

        let info = TransactionInfoV2::new(
            TransactionAuthenticator::Move(Box::new(transaction.authenticator())),
            maybe_block_data,
            hash,
            SmrTransactionHeader::from(&transaction),
            TransactionPayload::Move(deserialized_payload),
            maybe_move_output,
            status,
        );
        Ok(info)
    }

    pub(crate) fn api_move_transaction_output(
        converter: Converter,
        maybe_transaction_output: Option<SerializedOutput>,
    ) -> Result<Option<TransactionOutput>, ApiServerError> {
        // Deserialize the output if any was given.
        let maybe_move_output = if let Some(output) = maybe_transaction_output {
            let SerializedOutput::Move(move_output) = output else {
                // This branch will only be executed if the code has regressed. Executed Move
                // transactions should always have Move outputs.
                return Err(ApiServerError::InconsistentArchiveState(
                    "Move transaction has non-move output.".to_string(),
                ));
            };
            let deserialized_output =
                converter.convert_to_api_move_transaction_output(move_output)?;
            Some(TransactionOutput::Move(deserialized_output))
        } else {
            None
        };
        Ok(maybe_move_output)
    }

    /// Get the latest [CertifiedBlock] with possibly [SerializedSmrBatch]'s finalized in that block.
    pub(crate) fn get_latest_certified_block(
        &self,
        with_batches: bool,
    ) -> Result<Option<ConsensusBlock>, ApiServerError> {
        let certified_block = self
            .store
            .certified_block
            .iter_values(ReadOptions::default(), IteratorMode::End)
            .next()
            .transpose()
            .map_err(StoreError::SchemaError)?;

        let Some(certified_block) = certified_block else {
            return Ok(None);
        };

        let certified_block = certified_block
            .hydrate(&self.store)
            .map_err(ApiServerError::from)?;

        let batches = if with_batches {
            Some(self.get_batches_for_certified_block(&certified_block)?)
        } else {
            None
        };

        Ok(Some(ConsensusBlock::new(certified_block, batches)))
    }

    /// Get [CertifiedBlock] with possibly [PackedBatchData]'s finalized in that block.
    pub(crate) fn get_certified_block(
        &self,
        height: Height,
        with_batches: bool,
    ) -> Result<Option<ConsensusBlock>, ApiServerError> {
        let certified_block = self.store.certified_block.read(height, None)?;

        let Some(certified_block) = certified_block else {
            return Ok(None);
        };

        let certified_block = certified_block
            .hydrate(&self.store)
            .map_err(ApiServerError::from)?;

        let batches = if with_batches {
            Some(self.get_batches_for_certified_block(&certified_block)?)
        } else {
            None
        };

        Ok(Some(ConsensusBlock::new(certified_block, batches)))
    }

    /// Fetch [SerializedSmrBatch]'a for a given [CertifiedBlock].
    fn get_batches_for_certified_block(
        &self,
        certified_block: &CertifiedBlock,
    ) -> Result<PackedBatchData, ApiServerError> {
        let height = certified_block.height();

        let mut batches = vec![];

        for hash in certified_block
            .payload()
            .items()
            .iter()
            .map(|item| item.store_key())
        {
            let batch = self.store.batch.read_bytes(hash)?
                    .ok_or_else(|| ApiServerError::InconsistentStoreState(
                        format!("Block at height {height} includes batch with hash {hash}, however a corresponding batch not found in Store")
                    ))?;

            batches.push(PackedSmrBatch::new(hash, batch));
        }

        Ok(PackedBatchData::new(batches))
    }

    /// Get the events by type.
    pub(crate) fn get_events_by_type(
        &self,
        event_type: TypeTag,
        start_height: u64,
        end_height: u64,
    ) -> Result<Events, PublicApiServerError> {
        let contract_events = self
            .storage
            .archive()
            .reader()
            .get_events_by_type(event_type, start_height, end_height)
            .map_err(ApiServerError::from)?;
        let converter = self.converter();

        Ok(Events {
            data: converter
                .try_into_events(&contract_events)
                .map_err(ApiServerError::from)?,
        })
    }

    /// Get events by type with a limit on the number of events returned.
    pub(crate) fn get_events_by_type_with_limit(
        &self,
        event_type: TypeTag,
        start_height: u64,
        end_height: u64,
        limit: usize,
        cursor: Option<Vec<u8>>,
    ) -> Result<(EventsV3, Option<Vec<u8>>), PublicApiServerError> {
        // Get the events from the archive reader, with the limit and the cursor.
        let (events_with_context, cursor) = self
            .storage
            .archive()
            .reader()
            .get_events_by_type_with_limit_and_context(
                event_type,
                start_height,
                end_height,
                limit,
                cursor,
            )
            .map_err(ApiServerError::from)?;

        let (contract_events, contexts): (Vec<_>, Vec<_>) = events_with_context.into_iter().unzip();

        // Convert the events to API format
        let converter = self.converter();
        let event_data = converter
            .try_into_events(&contract_events)
            .map_err(ApiServerError::from)?;

        // Create the final events list.
        let data = event_data
            .into_iter()
            .zip(contexts)
            .map(|(event, (block_height, transaction_hash))| {
                (event, block_height, transaction_hash)
            })
            .map(EventWithContext::from)
            .collect();

        Ok((EventsV3 { data }, cursor))
    }

    /// Get [CommitteeAuthorization] for [Epoch].
    pub(crate) fn get_authorization_for_epoch(
        &self,
        epoch: Epoch,
    ) -> Result<Option<CommitteeAuthorization>, ApiServerError> {
        Ok(self
            .store
            .epoch_authorized_committee
            .read(epoch, None)?
            .map(|authorized_committee| authorized_committee.authorization().clone()))
    }

    /// The transaction MUST NOT have a valid signature, to prevent replay or Man-in-the-middle attacks.
    /// The simulation VM will reject any transaction with a valid signature.
    pub async fn run_move_transaction_simulation(
        &self,
        txn: SignedTransaction,
    ) -> Result<TransactionInfoV2, PublicApiServerError> {
        // Limit the number of concurrent simulations to prevent resource exhaustion.
        // Total blocking tasks in tokio runtime is limited to 512 by default.
        const MAX_CONCURRENCY: u16 = 512; // TODO: make it configurable.
        static CURRENT_NUM_OF_CONCURRENT_SIMULATION: AtomicU16 = AtomicU16::new(0);

        // Back pressure if too many concurrent simulations are running.
        if CURRENT_NUM_OF_CONCURRENT_SIMULATION.load(Ordering::Relaxed) >= MAX_CONCURRENCY {
            return Err(ApiServerError::TooManyConcurrentSimulations.into());
        }

        CURRENT_NUM_OF_CONCURRENT_SIMULATION.fetch_add(1, Ordering::Relaxed);

        let state_view_store = self.move_store.clone();

        // TODO: Check all REST API endpoints that need to be put in blocking context.
        let transaction_info = tokio::task::spawn_blocking(move || {
            if txn.verify_signature().is_ok() {
                return Err(ApiServerError::InvalidSimulationTransaction.into());
            }
            // As transaction output already contains vm_status info included, it is discarded for now.
            let (_, transaction_output) =
                AptosSimulationVM::create_vm_and_simulate_signed_transaction(
                    &txn,
                    &state_view_store,
                );
            let simulation_output: SerializedOutput =
                MoveTransactionOutput::from(transaction_output).into();
            let smr_move_txn = SmrMoveTransaction::from(txn);
            let tx_status = simulation_output.execution_status();
            let converter = Converter::new(&state_view_store);
            Self::move_transaction_info(
                converter,
                smr_move_txn.digest(),
                smr_move_txn,
                None,
                Some(simulation_output),
                tx_status,
            )
            .map_err(PublicApiServerError::from)
        })
        .await
        .map_err(|e| ApiServerError::Other(anyhow!("Simulation blocking task failure: {e}")))??;

        CURRENT_NUM_OF_CONCURRENT_SIMULATION.fetch_sub(1, Ordering::Relaxed);
        Ok(transaction_info)
    }

    pub fn get_move_list(
        &self,
        addr: AccountAddress,
        cursor: Option<Vec<u8>>,
        count: usize,
        query_kind: MoveListQuery,
    ) -> Result<(MoveListV3, Option<Vec<u8>>), PublicApiServerError> {
        MoveListBuilder::new(self, addr, query_kind, cursor, count)
            .and_then(|builder| builder.build_move_list())
            .map_err(ApiServerError::from)
            .map_err(PublicApiServerError::from)
    }

    /// Returns the chain-id.
    pub fn chain_id(&self) -> ChainId {
        self.chain_id
    }

    pub fn get_block_metadata_by_block_height(
        &self,
        height: Height,
    ) -> Result<Option<BlockMetadataInfo>, PublicApiServerError> {
        self.get_block_metadata_by_block_height_internal(height)
            .map_err(PublicApiServerError::from)
    }
    pub fn get_block_metadata_by_hash(
        &self,
        hash: Hash,
    ) -> Result<Option<BlockMetadataInfo>, PublicApiServerError> {
        let maybe_block_metadata_hash = self
            .storage
            .archive()
            .reader()
            .is_block_metadata_hash(hash)
            .map_err(ApiServerError::from)?;
        if !maybe_block_metadata_hash {
            return Err(ApiServerError::TransactionNotFound(hash).into());
        }
        self.get_block_metadata_by_hash_internal(hash, true)
            .map_err(PublicApiServerError::from)
    }

    fn get_block_metadata_by_block_height_internal(
        &self,
        height: Height,
    ) -> Result<Option<BlockMetadataInfo>, ApiServerError> {
        let maybe_hash = self
            .storage
            .archive()
            .reader()
            .get_block_metadata_hash_by_block_height(height)?;
        let Some(hash) = maybe_hash else {
            return Ok(None);
        };
        self.get_block_metadata_by_hash_internal(hash, false)
    }

    fn get_block_metadata_by_hash_internal(
        &self,
        hash: Hash,
        with_block_info: bool,
    ) -> Result<Option<BlockMetadataInfo>, ApiServerError> {
        let Some(status) = self
            .storage
            .archive()
            .reader()
            .get_transaction_status(hash)?
        else {
            return Err(ApiServerError::MissingTransactionStatus(hash))?;
        };

        let (output, block_info) =
            self.get_finalized_transaction_output_and_block_info(hash, with_block_info)?;
        // A BlockMetadata transaction should always have output if it is present in the index table.
        if let Some(api_output) = Self::api_move_transaction_output(self.converter(), output)? {
            let block_metadata = BlockMetadataInfo::new(hash, block_info, api_output, status);
            Ok(Some(block_metadata))
        } else {
            Err(ApiServerError::MissingBlockMetadataOutput(hash))?
        }
    }

    fn check_move_payload_type<ErrorMsg>(
        info: &TransactionInfoV2,
        error_msg: ErrorMsg,
    ) -> Result<(), ApiServerError>
    where
        ErrorMsg: Fn() -> String,
    {
        if !info.payload().is_move() {
            // This branch will only be executed if the code has regressed. The archive should
            // only have the hash of the transaction matched to this [BlockHeaderInfo] if the
            // transaction produced a [TransactionOutput] when the block was executed.
            return Err(ApiServerError::InconsistentArchiveState(error_msg()));
        }
        Ok(())
    }

    pub fn get_block_execution_statistics_by_height(
        &self,
        height: Height,
    ) -> Result<Option<ExecutedBlockStats>, PublicApiServerError> {
        let native_stats = self
            .storage
            .archive()
            .reader()
            .get_block_stats_by_block_height(height)
            .map_err(ApiServerError::from)?;
        Ok(native_stats.map(ExecutedBlockStats::from))
    }
}
