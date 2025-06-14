use crate::rest::api_server_error::ApiServerError;
use aptos_api_types::AsConverter;
use aptos_api_types::MoveConverter;
use aptos_api_types::MoveValue;
use aptos_types::transaction::TransactionStatus;
use execution::MoveStore;
use move_core_types::language_storage::TypeTag;
use std::ops::Deref;
use std::sync::Arc;
use types::api::v1::MoveTransactionOutput as ApiMoveTransactionOutput;
use types::transactions::MoveTransactionOutput;

pub struct Converter<'a> {
    inner: MoveConverter<'a, MoveStore>,
}

impl<'a> Deref for Converter<'a> {
    type Target = MoveConverter<'a, MoveStore>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> Converter<'a> {
    pub fn new(move_store: &'a MoveStore) -> Self {
        let converter = move_store.as_converter(Arc::new(move_store.clone()), None);
        Self { inner: converter }
    }

    pub(crate) fn convert_to_move_value(
        &self,
        value_type: TypeTag,
        bytes: &[u8],
    ) -> Result<MoveValue, ApiServerError> {
        self.inner
            .try_into_move_value(&value_type, bytes.as_ref())
            .map_err(ApiServerError::from)
    }

    pub(crate) fn convert_to_api_move_transaction_output(
        &self,
        move_output: MoveTransactionOutput,
    ) -> Result<ApiMoveTransactionOutput, ApiServerError> {
        // Deserialize the events.
        let deserialized_events = self.try_into_events(&move_output.events)?;

        // Convert the status into a human-readable summary message.
        let status_message = match move_output.vm_status.transaction_status {
            // The Move write-set produced when executing the transaction was a no-op. See
            // [MoveTransactionOutput::execution_status] for more details.
            TransactionStatus::Discard(error_code) => {
                format!("Discarded with error code: {error_code:?}")
            }
            // Gas was charged and the sender's sequence number was incremented.
            TransactionStatus::Keep(kept_status) => {
                self.explain_vm_status(&kept_status, Some(move_output.vm_status.auxiliary_data))
            }
            // The transaction was included in an epoch boundary block.
            TransactionStatus::Retry => "Retry".to_string(),
        };

        let deserialized_output = ApiMoveTransactionOutput {
            events: deserialized_events,
            gas_used: move_output.gas_used,
            vm_status: status_message,
        };
        Ok(deserialized_output)
    }
}
