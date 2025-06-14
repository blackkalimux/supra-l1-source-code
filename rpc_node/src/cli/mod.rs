use crate::cli::migrate_db::Migrate;
use crate::cli::start_server::start_rpc_server;
use crate::error;
use clap::Parser;
use core::fmt::{Display, Formatter};
use migrations::{DbMigrationError, MigrationResult};
use serde::Serialize;
use thiserror::Error;

pub mod migrate_db;
pub mod start_server;

#[derive(Serialize)]
pub enum RpcToolResult {
    Blank(()),
    DatabaseToolResult(MigrationResult),
}

impl Display for RpcToolResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Blank(()) => Ok(()),
            Self::DatabaseToolResult(result) => write!(f, "{}", result),
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("RpcNode error: {0}")]
    RpcNodeError(#[from] error::Error),
    #[error("Db Tool error: {0}")]
    DbToolError(#[from] DbMigrationError),
}

#[derive(Parser)]
pub enum SupraRpcTool {
    #[clap(about = "Start Rpc Node.", long_about = "Start Rpc Node.")]
    Start,
    #[clap(about = "Migrate rpc database to latest version.")]
    MigrateDB(Migrate),
}

impl SupraRpcTool {
    pub async fn execute(self) -> Result<RpcToolResult, Error> {
        match self {
            Self::Start => Ok(start_rpc_server().await.map(RpcToolResult::Blank)?),
            Self::MigrateDB(tool) => Ok(tool.execute().map(RpcToolResult::DatabaseToolResult)?),
        }
    }
}
