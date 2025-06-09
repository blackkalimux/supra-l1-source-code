use super::bcs_decode::BcsJsonDecode;
use super::bcs_encode::BcsJsonEncode;
use crate::common::error::CliError;
use crate::move_tool::error::MoveToolError;
use crate::move_tool::MoveToolResult;
use crate::rest_utils::action::{RpcAction, RpcResponse};
use crate::rest_utils::client::supra_rpc_client::RpcClientApi;
use crate::rest_utils::client::RPCClientIfc;
use crate::rest_utils::rpc_request_kind::public_query::PublicQueryHelper;
use crate::rest_utils::rpc_request_kind::RpcRequestKind;
use crate::rest_utils::tool_option::{
    MoveTransactionOptions, ResolveTransactionHash, SupraTransactionOptions,
};
use aptos::common::types::{CliCommand, EntryFunctionArguments};
use aptos::move_tool::{
    coverage, BuildPublishPayload, CleanPackage, CompilePackage, CompileScript, DocumentPackage,
    InitPackage, ProvePackage, PublishPackage, RunFunction, RunScript, TestPackage,
};
use aptos_api_types::ViewRequest;
use clap::Parser;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use supra_aptos::SupraCommand;
use types::api::v1::{MoveListQuery, MoveShowQuery};

/// Command-line interface for MoveTool operations.
#[derive(Parser)]
#[clap(
    about = "Tool for Move-related operations",
    long_about = "Provides means to compile and publish Move code."
)]
pub enum MoveToolCommand {
    BcsJsonDecode(BcsJsonDecode),
    BcsJsonEncode(BcsJsonEncode),
    BuildPublishPayload(BuildPublishPayload),
    Clean(CleanPackage),
    Compile(CompilePackage),
    CompileScript(CompileScript),
    #[clap(subcommand)]
    Coverage(coverage::CoveragePackage),
    Document(DocumentPackage),
    Init(InitPackage),
    List(ListTool),
    Show(ShowTool),
    Prove(ProvePackage),
    #[clap(about = "Publish a smart contract to Supra blockchain.")]
    Publish(PublishPackage),
    Run(RunFunction),
    #[clap(about = "Run a Move script.")]
    RunScript(RunScript),
    Test(TestPackage),
    View(ViewFunction),
}

impl Display for MoveToolCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveToolCommand::BcsJsonDecode(_) => write!(f, "BcsJsonDecode"),
            MoveToolCommand::BcsJsonEncode(_) => write!(f, "BcsJsonEncode"),
            MoveToolCommand::BuildPublishPayload(_) => write!(f, "BuildPublishPayload"),
            MoveToolCommand::Clean(_) => write!(f, "Clean"),
            MoveToolCommand::Compile(_) => write!(f, "Compile"),
            MoveToolCommand::CompileScript(_) => write!(f, "CompileScript"),
            MoveToolCommand::Coverage(_) => write!(f, "Coverage"),
            MoveToolCommand::Document(_) => write!(f, "Document"),
            MoveToolCommand::Init(_) => write!(f, "Init"),
            MoveToolCommand::List(_) => write!(f, "List"),
            MoveToolCommand::Show(_) => write!(f, "Show"),
            MoveToolCommand::Prove(_) => write!(f, "Prove"),
            MoveToolCommand::Publish(_) => write!(f, "Publish"),
            MoveToolCommand::Run(_) => write!(f, "Run"),
            MoveToolCommand::RunScript(_) => write!(f, "RunScript"),
            MoveToolCommand::Test(_) => write!(f, "Test"),
            MoveToolCommand::View(_) => write!(f, "View"),
        }
    }
}

/// Run a view function
#[derive(Parser)]
pub struct ViewFunction {
    #[clap(flatten)]
    pub(crate) entry_function_args: EntryFunctionArguments,
    #[clap(flatten)]
    pub(crate) txn_options: SupraTransactionOptions,
}

/// Lists of on-chain resources for an account
#[derive(Parser)]
pub struct ListTool {
    /// Most recent items published by account address.
    #[clap(long)]
    pub query: MoveListQuery,
    #[clap(flatten)]
    pub txn_options: SupraTransactionOptions,
}

/// Show on-chain module abi or inspect on-chain resource
#[derive(Parser)]
pub struct ShowTool {
    /// Query move module or resource.
    #[clap(long)]
    pub query: MoveShowQuery,
    /// Name of module or resource.
    #[clap(long)]
    pub name: String,
    #[clap(flatten)]
    pub txn_options: SupraTransactionOptions,
}

impl MoveToolCommand {
    /// Executes the MoveTool command.
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        match self {
            MoveToolCommand::BcsJsonDecode(tool) => tool.execute(),
            MoveToolCommand::BcsJsonEncode(tool) => tool.execute(),
            MoveToolCommand::BuildPublishPayload(tool) => {
                let tool_res = tool.execute_serialized().await;
                MoveToolResult::try_as_move_tool_result(tool_res)
            }
            MoveToolCommand::Clean(tool) => {
                let tool_res = tool.execute_serialized().await;
                MoveToolResult::try_as_move_tool_result(tool_res)
            }
            MoveToolCommand::Compile(tool) => {
                let tool_res = tool.execute_serialized().await;
                MoveToolResult::try_as_move_tool_result(tool_res)
            }
            MoveToolCommand::CompileScript(tool) => {
                let tool_res = tool.execute_serialized().await;
                MoveToolResult::try_as_move_tool_result(tool_res)
            }
            MoveToolCommand::Coverage(tool) => {
                let tool_res = tool.execute().await;
                MoveToolResult::try_as_move_tool_result(tool_res)
            }
            MoveToolCommand::Document(tool) => {
                let tool_res = tool.execute_serialized().await;
                MoveToolResult::try_as_move_tool_result(tool_res)
            }
            MoveToolCommand::Init(tool) => {
                let tool_res = tool.execute_serialized().await;
                MoveToolResult::try_as_move_tool_result(tool_res)
            }
            MoveToolCommand::List(tool) => {
                match RpcClientApi::new_with_action(
                    RpcAction::ListAccountAssets(
                        tool.txn_options.api_version(),
                        *tool.txn_options.profile_attribute()?.account_address(),
                        tool.query,
                    ),
                    RpcRequestKind::PublicQuery(PublicQueryHelper::new(
                        tool.txn_options.profile_attribute()?.rpc_url().clone(),
                    )),
                )?
                .execute_action()
                .await?
                {
                    RpcResponse::ListAccountAssets(response) => {
                        Ok(MoveToolResult::ListAccountPackage(response))
                    }
                    _ => Err(CliError::RESTClient),
                }
            }
            MoveToolCommand::Show(tool) => {
                let account_address = *tool.txn_options.profile_attribute()?.account_address();
                let req_kind = RpcRequestKind::PublicQuery(PublicQueryHelper::new(
                    tool.txn_options.profile_attribute()?.rpc_url().clone(),
                ));
                let resp = match tool.query {
                    MoveShowQuery::Module => RpcClientApi::new_with_action(
                        RpcAction::AccountModule(
                            tool.txn_options.api_version(),
                            account_address,
                            Identifier::from_str(tool.name.as_str())?,
                        ),
                        req_kind,
                    )?,
                    MoveShowQuery::Resource => RpcClientApi::new_with_action(
                        RpcAction::AccountResource(
                            tool.txn_options.api_version(),
                            account_address,
                            TypeTag::from(StructTag::from_str(tool.name.as_str())?),
                        ),
                        req_kind,
                    )?,
                }
                .execute_action()
                .await?;
                match resp {
                    RpcResponse::AccountResource(response)
                    | RpcResponse::AccountModule(response) => {
                        let body = response.text().await.map_err(MoveToolError::ReqwestError)?;
                        Ok(MoveToolResult::MoveCli(serde_json::from_str(&body)?))
                    }
                    _ => Err(CliError::RESTClient),
                }
            }
            MoveToolCommand::Prove(tool) => {
                let tool_res = tool.execute_serialized().await;
                MoveToolResult::try_as_move_tool_result(tool_res)
            }
            MoveToolCommand::Publish(tool) => {
                let txn_argument = tool
                    .supra_command_arguments()
                    .await
                    .map_err(|e| CliError::GeneralError(e.to_string()))?;
                MoveTransactionOptions::from(txn_argument)
                    .send_transaction(ResolveTransactionHash::UntilSuccess)
                    .await
                    .map(MoveToolResult::TransactionInfo)
            }
            MoveToolCommand::Run(tool) => {
                let txn_argument = tool
                    .supra_command_arguments()
                    .await
                    .map_err(|e| CliError::GeneralError(e.to_string()))?;
                MoveTransactionOptions::from(txn_argument)
                    .send_transaction(ResolveTransactionHash::UntilSuccess)
                    .await
                    .map(MoveToolResult::TransactionInfo)
            }
            MoveToolCommand::RunScript(tool) => {
                let txn_argument = tool
                    .supra_command_arguments()
                    .await
                    .map_err(|e| CliError::GeneralError(e.to_string()))?;
                MoveTransactionOptions::from(txn_argument)
                    .send_transaction(ResolveTransactionHash::UntilSuccess)
                    .await
                    .map(MoveToolResult::TransactionInfo)
            }
            MoveToolCommand::Test(tool) => {
                let tool_res = tool.execute_serialized().await;
                MoveToolResult::try_as_move_tool_result(tool_res)
            }
            MoveToolCommand::View(tool) => {
                let view_req: ViewRequest = tool.entry_function_args.try_into()?;
                match RpcClientApi::new_with_action(
                    RpcAction::RunViewFunction(tool.txn_options.api_version(), view_req),
                    RpcRequestKind::PublicQuery(PublicQueryHelper::new(
                        tool.txn_options.profile_attribute()?.rpc_url().clone(),
                    )),
                )?
                .execute_action()
                .await?
                {
                    RpcResponse::RunViewFunction(response) => {
                        let body = response.text().await.map_err(MoveToolError::ReqwestError)?;
                        Ok(MoveToolResult::MoveCli(serde_json::from_str(&body)?))
                    }
                    _ => Err(CliError::RESTClient),
                }
            }
        }
    }
}
