use crate::common::error::CliError;
use aptos_types::account_address::AccountAddress;

pub fn parse_pool_address(address: Option<&str>) -> Result<AccountAddress, CliError> {
    let pool_address = match address {
        Some(addr) => AccountAddress::from_str_strict(addr).map_err(|_| {
            CliError::Aborted(
                format!("{} is not a valid account address", addr),
                "The account address must be a valid 0x + 64 hex characters".to_owned(),
            )
        })?,
        None => {
            return Err(CliError::Aborted(
                "Delegation pool address is required".to_owned(),
                "Please provide the delegation pool address".to_owned(),
            ));
        }
    };
    Ok(pool_address)
}
