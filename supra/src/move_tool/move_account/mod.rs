use crate::common::error::CliError;
use crate::move_tool::MoveToolResult;
use aptos::account::derive_resource_account;
use aptos::common::types::CliCommand;
use aptos_types::SUPRA_COIN_TYPE;
use clap::Subcommand;
use move_core_types::language_storage::TypeTag;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use types::cli_utils::constants::STRUCT_TAG_COIN_STORE;

pub mod balance;
pub mod create;
pub mod create_resource_account;
pub mod derive_multisig_addr;
pub mod fund;
pub mod transfer;

#[derive(Subcommand, Debug)]
#[clap(
    about = "Tool for interacting with accounts",
    long_about = "This tool is used to create accounts, get information about the \
    account's coin balance, and transfer coin between accounts."
)]
pub enum AccountTool {
    Create(create::CreateAccount),
    CreateResourceAccount(create_resource_account::CreateResourceAccount),
    DeriveResourceAccountAddress(derive_resource_account::DeriveResourceAccount),
    FundWithFaucet(fund::FundWithFaucet),
    Balance(balance::Balance),
    Transfer(transfer::TransferCoins),
    DeriveMultiSigAddress(derive_multisig_addr::DeriveMultiSigAddress),
}

impl Display for AccountTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountTool::Create(_) => write!(f, "AccountTool::Create"),
            AccountTool::CreateResourceAccount(_) => {
                write!(f, "AccountTool::CreateResourceAccount")
            }
            AccountTool::DeriveResourceAccountAddress(_) => {
                write!(f, "AccountTool::CreateResourceAccount")
            }

            AccountTool::FundWithFaucet(_) => write!(f, "AccountTool::FundWithFaucet"),
            AccountTool::Balance(_) => write!(f, "AccountTool::Balance"),
            AccountTool::Transfer(_) => write!(f, "AccountTool::Transfer"),
            AccountTool::DeriveMultiSigAddress(_) => {
                write!(f, "AccountTool::DeriveMultiSigAddress")
            }
        }
    }
}

impl AccountTool {
    pub async fn execute(self) -> Result<MoveToolResult, CliError> {
        match self {
            AccountTool::Create(tool) => tool.execute().await,
            AccountTool::CreateResourceAccount(tool) => tool.execute().await,
            AccountTool::DeriveResourceAccountAddress(tool) => Ok(tool
                .execute()
                .await
                .map(MoveToolResult::DeriveResourceAccountAddress)?),
            AccountTool::FundWithFaucet(tool) => tool.execute().await,
            AccountTool::Balance(tool) => tool.execute().await,
            AccountTool::Transfer(tool) => tool.execute().await,
            AccountTool::DeriveMultiSigAddress(tool) => tool.execute(),
        }
    }
}

/// Resolve the type of coin used by cli
pub enum SupraCoinTypeResolver {
    CoinResource,
    TransferCoins,
}

impl SupraCoinTypeResolver {
    pub fn resolve(&self, coin_type: &Option<String>) -> Result<TypeTag, anyhow::Error> {
        match coin_type {
            None => {
                let resolved_coin_type = match self {
                    SupraCoinTypeResolver::CoinResource => TypeTag::from_str(&format!(
                        "{}<{}>",
                        STRUCT_TAG_COIN_STORE, *SUPRA_COIN_TYPE
                    ))?,
                    SupraCoinTypeResolver::TransferCoins => SUPRA_COIN_TYPE.clone(),
                };
                Ok(resolved_coin_type)
            }
            Some(user_input) => TypeTag::from_str(user_input.as_str()),
        }
    }
}
