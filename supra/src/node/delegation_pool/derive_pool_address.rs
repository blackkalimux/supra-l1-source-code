use super::DelegationPoolResult;
use crate::common::error::CliError;
use aptos_types::account_address::AccountAddress;
use clap::Args;
use types::genesis::delegation_pool_config::delegation_pool_config_v1::DelegationPoolConfigV1;

#[derive(Args, Debug)]
pub struct DerivePoolAddress {
    /// Owner address of the delegation pool.
    #[clap(short, long)]
    pub owner: String,

    /// Creation seed of the delegation pool.
    #[clap(short, long)]
    pub seed: String,
}

impl DerivePoolAddress {
    pub fn execute(self) -> Result<DelegationPoolResult, CliError> {
        let owner_account_address = AccountAddress::from_str_strict(&self.owner).map_err(|e| {
            CliError::Aborted(
                format!("{} is not a valid account address. {e:?}", self.owner),
                "The account address must be a valid 0x + 64 hex characters".to_owned(),
            )
        })?;
        let pool_address =
            DelegationPoolConfigV1::derive_pbo_pool_address_from(owner_account_address, &self.seed);

        // Do not change the format of the string below because end-to-end tests depend on it to verify the output.
        Ok(DelegationPoolResult::DerivePoolAddress(format!(
            "Derived pool address: 0x{}",
            pool_address.to_canonical_string()
        )))
    }
}
