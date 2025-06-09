use crate::common::error::CliError;
use crate::rest_utils::tool_option::GasParameters;
use aptos_crypto::ed25519::Ed25519PublicKey;
use aptos_types::chain_id::ChainId;
use aptos_types::transaction::{RawTransaction, SignedTransaction, TransactionPayload};
use std::ops::Add;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use types::cli_profile::profile_management::{SignerKeyType, SignerProfileHelper};
use types::impl_struct_create_and_read_methods;
use types::transactions::transaction::AccountAddress;
use url::Url;

#[derive(Clone)]
pub struct SignedTransactionHelper {
    txn_sender_address: AccountAddress,
    txn_sender_pub_key: Ed25519PublicKey,
    signer_key_type: SignerKeyType,
    max_gas_amount: u64,
    gas_unit_price: u64,
    /// This value indicate after which number of seconds any associated transaction will expire.
    expiration_period_secs: u64,
    url: Url,
    chain_id: ChainId,
}

impl_struct_create_and_read_methods!(SignedTransactionHelper {
    txn_sender_address: AccountAddress,
    txn_sender_pub_key: Ed25519PublicKey,
    signer_key_type: SignerKeyType,
    max_gas_amount: u64,
    gas_unit_price: u64,
    expiration_period_secs: u64,
    url: Url,
    chain_id: ChainId,
});

impl PartialEq for SignedTransactionHelper {
    fn eq(&self, other: &Self) -> bool {
        self.txn_sender_address == other.txn_sender_address
    }
}

impl SignedTransactionHelper {
    pub fn new_user_object(
        cli_profile_info: SignerProfileHelper,
        gas_option: GasParameters,
    ) -> Self {
        Self {
            txn_sender_address: *cli_profile_info.account_address(),
            txn_sender_pub_key: cli_profile_info.public_key().clone(),
            signer_key_type: cli_profile_info.signer_key_type().clone(),
            max_gas_amount: gas_option.max_gas,
            gas_unit_price: gas_option.gas_unit_price,
            expiration_period_secs: gas_option.expiration_secs,
            url: cli_profile_info.rpc_url().clone(),
            chain_id: *cli_profile_info.chain_id(),
        }
    }

    pub async fn create_transaction(
        &self,
        transaction_payload: TransactionPayload,
        chain_id: u8,
        sequence_number: u64,
    ) -> Result<SignedTransaction, CliError> {
        let raw_txn = self
            .create_raw_user_transaction(transaction_payload, chain_id, sequence_number)
            .await?;
        Ok(self
            .signer_key_type
            .sign(raw_txn, self.txn_sender_pub_key.clone())?)
    }

    pub async fn create_raw_user_transaction(
        &self,
        transaction_payload: TransactionPayload,
        chain_id: u8,
        sequence_number: u64,
    ) -> Result<RawTransaction, CliError> {
        let expiration_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .add(Duration::from_secs(self.expiration_period_secs))
            .as_secs();
        Ok(RawTransaction::new(
            self.txn_sender_address,
            sequence_number,
            transaction_payload,
            self.max_gas_amount,
            self.gas_unit_price,
            expiration_timestamp,
            ChainId::new(chain_id),
        ))
    }
}
