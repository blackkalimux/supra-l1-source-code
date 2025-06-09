use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use aptos_cached_packages::aptos_stdlib::automation_registry_cancel_task;
use clap::Parser;

#[derive(Debug, Parser)]
#[clap(about = "Cancel registered automation task by id")]
pub struct CancellationCommand {
    #[clap(
        long,
        help = "Simulate automation task cancellation request.",
        default_value_t = false
    )]
    simulate: bool,
    #[clap(long)]
    task_index: u64,
    #[clap(flatten)]
    txn_options: SupraTransactionOptions,
}

impl CancellationCommand {
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        let mut runner = MoveTransactionOptions::new(
            automation_registry_cancel_task(self.task_index),
            self.txn_options,
        );
        if self.simulate {
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
