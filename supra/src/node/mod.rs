use crate::common::error::CliError;
use crate::node::identity_rotation::{IdentityRotationResult, IdentityRotationTool};
use crate::node::smr_node::{SmrTool, SmrToolResult};
use clap::Parser;
use delegation_pool::{DelegationPoolResult, DelegationPoolTool};
use serde::Serialize;
use std::fmt::{Display, Formatter};
use supra_logger::LogFilterReload;

pub mod delegation_pool;
pub mod identity_rotation;
pub mod smr_node;

#[derive(Serialize)]
pub enum NodeToolResult {
    SmrToolResult(SmrToolResult),
    IdentityRotationResult(Box<IdentityRotationResult>),
    DelegationPoolResult(DelegationPoolResult),
}

impl Display for NodeToolResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeToolResult::SmrToolResult(res) => {
                writeln!(f, "{}", res)
            }
            NodeToolResult::IdentityRotationResult(res) => {
                writeln!(f, "{}", res)
            }
            NodeToolResult::DelegationPoolResult(res) => {
                writeln!(f, "{}", res)
            }
        }
    }
}

#[derive(Parser, Debug)]
#[clap(
    alias = "n",
    about = "Operations for Supra Nodes.",
    long_about = "Provides means to manage Supra's SMR or RPC node."
)]
pub enum NodeTool {
    #[clap(subcommand)]
    Smr(SmrTool),
    #[clap(flatten)]
    Identity(IdentityRotationTool),
    #[clap(subcommand)]
    DelegationPool(DelegationPoolTool),
}

impl Display for NodeTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl NodeTool {
    pub async fn execute(
        self,
        log_filter_reload: Option<LogFilterReload>,
    ) -> Result<NodeToolResult, CliError> {
        match self {
            NodeTool::Smr(tool) => tool
                .execute(log_filter_reload)
                .await
                .map(NodeToolResult::SmrToolResult),
            NodeTool::Identity(cmd) => cmd
                .execute()
                .await
                .map(Box::new)
                .map(NodeToolResult::IdentityRotationResult),
            NodeTool::DelegationPool(tool) => tool
                .execute()
                .await
                .map(NodeToolResult::DelegationPoolResult),
        }
    }
}
