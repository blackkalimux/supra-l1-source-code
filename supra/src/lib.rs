use clap::Parser;
use serde::Serialize;
use std::fmt::{Display, Formatter};
use supra_logger::LogFilterReload;

pub mod common;
pub mod config;
pub mod database;
pub mod genesis;
pub mod governance;
pub mod move_tool;
pub mod nidkg;
pub mod node;
pub mod profile;
pub mod rest_utils;
pub(crate) mod utils;

#[cfg(test)]
mod tests;

#[derive(Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum SupraToolResult {
    ProfileToolResult(profile::ProfileToolResult),
    DKGToolResult(nidkg::DkgToolResult),
    NodeToolResult(node::NodeToolResult),
    ConfigToolResult(config::ConfigToolResult),
    MoveToolResult(move_tool::MoveToolResult),
    DataToolResult(database::DatabaseToolResult),
    GenesisToolResult(genesis::GenesisToolResult),
}

impl Display for SupraToolResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SupraToolResult::ProfileToolResult(res) => {
                writeln!(f, "{}", res)
            }
            SupraToolResult::DKGToolResult(res) => {
                writeln!(f, "{}", res)
            }
            SupraToolResult::NodeToolResult(res) => {
                writeln!(f, "{}", res)
            }
            SupraToolResult::ConfigToolResult(res) => {
                writeln!(f, "{}", res)
            }
            SupraToolResult::MoveToolResult(res) => writeln!(f, "{}", res),
            SupraToolResult::DataToolResult(res) => writeln!(f, "{}", res),
            SupraToolResult::GenesisToolResult(res) => writeln!(f, "{}", res),
        }
    }
}

#[derive(Parser)]
pub enum SupraTool {
    #[clap(subcommand)]
    Profile(profile::ProfileTool),
    #[clap(subcommand)]
    Nidkg(nidkg::DKGTool),
    #[clap(subcommand)]
    Node(node::NodeTool),
    #[clap(subcommand)]
    Config(config::ConfigTool),
    #[clap(subcommand)]
    Move(move_tool::MoveTool),
    #[clap(subcommand)]
    Data(database::DatabaseTool),
    #[clap(subcommand)]
    Genesis(genesis::GenesisTool),
    #[clap(subcommand)]
    Governance(governance::GovernanceTool),
}

impl Display for SupraTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SupraTool::Profile(tool) => write!(f, "{tool:?}"),
            SupraTool::Nidkg(tool) => write!(f, "{tool:?}"),
            SupraTool::Node(tool) => write!(f, "{tool:?}"),
            SupraTool::Config(tool) => write!(f, "{tool:?}"),
            SupraTool::Move(tool) => write!(f, "{tool}"),
            SupraTool::Data(tool) => write!(f, "{tool:?}"),
            SupraTool::Genesis(tool) => write!(f, "{tool:?}"),
            SupraTool::Governance(tool) => write!(f, "{tool}"),
        }
    }
}

impl SupraTool {
    pub async fn execute(
        self,
        log_filter_reload: Option<LogFilterReload>,
    ) -> Result<SupraToolResult, common::error::CliError> {
        match self {
            SupraTool::Profile(tool) => {
                tool.execute().await.map(SupraToolResult::ProfileToolResult)
            }
            SupraTool::Nidkg(tool) => tool.execute().await.map(SupraToolResult::DKGToolResult),
            SupraTool::Node(tool) => tool
                .execute(log_filter_reload)
                .await
                .map(SupraToolResult::NodeToolResult),
            SupraTool::Config(tool) => tool.execute().await.map(SupraToolResult::ConfigToolResult),
            SupraTool::Move(tool) => tool.execute().await.map(SupraToolResult::MoveToolResult),
            SupraTool::Data(tool) => tool.execute().await.map(SupraToolResult::DataToolResult),
            SupraTool::Genesis(tool) => {
                tool.execute().await.map(SupraToolResult::GenesisToolResult)
            }
            SupraTool::Governance(tool) => {
                tool.execute().await.map(SupraToolResult::MoveToolResult)
            }
        }
    }
}
