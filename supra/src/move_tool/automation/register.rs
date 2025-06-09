use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use aptos::common::types::EntryFunctionArguments;
use aptos_types::transaction::{automation::RegistrationParams, TransactionPayload};
use clap::Parser;

#[derive(Debug, Parser)]
#[clap(about = "Register a new automation task")]
pub struct RegistrationCommand {
    #[clap(
        long,
        help = "Simulate automation task registration.",
        default_value_t = false
    )]
    simulate: bool,
    #[clap(flatten)]
    task_payload: EntryFunctionArguments,
    #[clap(
        long,
        help = "Maximum gas amount to be paid when registered task is executed."
    )]
    task_max_gas_amount: u64,
    #[clap(long, help = "Maximum gas price user is willing to pay for the task.")]
    task_gas_price_cap: u64,
    #[clap(long, help = "Task expire time in seconds since EPOCH.")]
    task_expiry_time_secs: u64,
    #[clap(
        long,
        help = "The maximum automation fee per epoch user is willing to pay for the task."
    )]
    task_automation_fee_cap: u64,
    #[clap(flatten)]
    txn_options: SupraTransactionOptions,
}

impl RegistrationCommand {
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        let RegistrationCommand {
            simulate,
            task_payload,
            task_max_gas_amount,
            task_gas_price_cap,
            task_expiry_time_secs,
            task_automation_fee_cap,
            txn_options,
        } = self;
        let payload = task_payload.try_into()?;
        let registration_params = RegistrationParams::new_v1(
            payload,
            task_expiry_time_secs,
            task_max_gas_amount,
            task_gas_price_cap,
            task_automation_fee_cap,
            vec![], // Auxiliary data, not supported yet.
        );
        let txn_payload = TransactionPayload::AutomationRegistration(registration_params);
        let mut runner = MoveTransactionOptions::new(txn_payload, txn_options);
        if simulate {
            runner
                .simulate_transaction()
                .await
                .map(MoveToolResult::SimulationInfo)
        } else {
            runner
                .send_transaction(ResolveTransactionHash::UntilSuccess)
                .await
                .map(MoveToolResult::TransactionInfo)
        }
    }
}
