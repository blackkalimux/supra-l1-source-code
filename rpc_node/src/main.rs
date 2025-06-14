use clap::Parser;
use supra_build::build;

use rpc_node::cli::SupraRpcTool;

#[derive(Parser)]
#[clap(
    version = build::COMMIT_HASH,
    name = "Supra RPC node",
    about = "Supra RPC node RESTful API server for client applications to interact with the Supra blockchain",
    long_about = build::CLAP_LONG_VERSION
)]
pub struct SupraRPC {
    #[clap(subcommand)]
    cli: SupraRpcTool,
}

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    let matches = SupraRPC::parse();
    let cli_output = matches.cli.execute().await?;
    println!("{}", cli_output);
    Ok(())
}
