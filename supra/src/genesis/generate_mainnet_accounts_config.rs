use super::{error::GenesisToolError, utils::from_json_file_path};
use crate::{common::error::CliError, genesis::GenesisToolResult};
use aptos_types::account_address::{create_multisig_account_address, AccountAddress};
use aptos_vm_genesis::{MultiSigAccountSchema, MultiSigAccountWithBalance};
use clap::Args;
use serde::{Deserialize, Serialize};
use socrypto::PublicKey;
use soserde::SmrSerialize;
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap, HashSet},
    fs::write,
    hash::{Hash, Hasher},
    str::FromStr,
};
use types::{
    api::v1::SUPRA_COINS_BASE_WITH_DECIMALS,
    genesis::{
        delegation_pool_config::delegation_pool_config_v1::{
            DelegationPoolConfigV1, DelegatorAccountV1,
        },
        genesis_accounts::genesis_accounts_v1::GenesisAccountsV1,
        vesting::{VestingAccount, VestingPoolConfig},
    },
    settings::{
        committee::committee_config::config_v1::{CommitteeConfigV1, SupraCommitteesConfigV1},
        constants::GenesisConfigConstants,
    },
};

const DELEGATION_POOL_SEED_PREFIX: &str = "Delegation Pool";

/// The total number of Supra Foundation-operated delegation pools to receive stake from PBO
/// participants. This stake is assigned to Foundation-operated nodes as they are the most
/// reliable. We limit the number so that we can reduce the number of Foundation-operated
/// nodes in the future if need be. We have to commit to running at least this many nodes
/// for the duration of the PBO lockup period.
const DELEGATION_POOLS_WITH_PBO_STAKE: usize = 10;

const PBO_OWNER_NAME_PREFIX: &str = "Project Blast Off Owner ";

/// The total amount of SUPRA that may be lost when converting floats to integers.
const ERROR_MARGIN: u64 = 10;

/// The total number of SUPRA expected to be minted at genesis, assuming a 100B total supply and
/// 21% reserved for block rewards.
const SUPRA_MINTED_AT_GENESIS: u64 = 79_000_000_000;

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
enum FoundationMultisigAccountType {
    /// A multisig account controlled by The Supra Foundation. Controls the stake delegated
    /// to a particular validator at genesis.
    DelegationPoolOwner,
    /// Other multisig accounts managed by The Supra Foundation. Holds the funds allocated to
    /// The Foundation at genesis excluding the amount required to fund the stakes of the
    /// validators.
    StandaloneAccount,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct FoundationMultisigAccount {
    /// The [AccountAddress] of the multisig account.
    address: AccountAddress,
    /// The type of the multisig account, determined by its purpose.
    kind: FoundationMultisigAccountType,
    /// The name of the account. Identifies its purpose.
    name: String,
    /// The [AccountAddress] used to derive `address`.
    seed_owner_address: AccountAddress,
    /// The sequence number of `seed_owner_address` used to derive `address`.
    seed_sequence_number: u64,
}

#[derive(Debug, Serialize)]
struct FoundationMultisigAccounts {
    /// Unique multisig accounts managed by The Supra Foundation created to serve as the owners of
    /// the PBO Delegation Pools associated with the validators of the genesis [Committee].
    delegation_pool_owners: Vec<FoundationMultisigAccount>,
    /// All other multisig accounts managed by The Supra Foundation, such as the Treasury account.
    standalone: Vec<FoundationMultisigAccount>,
}

impl FoundationMultisigAccounts {
    const START_SEED_SEQUENCE_NUMBER: u64 = 0;

    /// Generates the [FoundationMultisigAccounts] corresponding to the given
    /// [FoundationMultiSigAccountsConfig] and `genesis_committee_size`. The result is
    /// guaranteed to be well-formed.
    fn new(
        config: &FoundationMultiSigAccountsConfig,
        genesis_committee_size: usize,
    ) -> Result<Self, CliError> {
        let seed_account = config.first_owner();
        let mut delegation_pool_owners = Vec::new();
        let mut start = Self::START_SEED_SEQUENCE_NUMBER;
        let mut end = start + genesis_committee_size as u64;

        // We generate one account for each validator, since each validator should have an associated
        // PBO Delegation Pool, and each PBO Delegation Pool must have a unique owner.
        for i in start..end {
            let account = FoundationMultisigAccount {
                address: create_multisig_account_address(seed_account, i),
                kind: FoundationMultisigAccountType::DelegationPoolOwner,
                name: format!("{PBO_OWNER_NAME_PREFIX} {i}"),
                seed_owner_address: seed_account,
                seed_sequence_number: i,
            };
            delegation_pool_owners.push(account);
        }

        let mut standalone = Vec::new();
        start = end;
        end = start + config.standalone_accounts.len() as u64;

        // Our fork of `aptos-core` creates independent multisig accounts after schema-based accounts,
        // so we must create all standalone accounts after the pool owner accounts.
        for i in start..end {
            let config_index = (i - start) as usize;
            let account_config = &config.standalone_accounts[config_index];
            let account = FoundationMultisigAccount {
                address: create_multisig_account_address(seed_account, i),
                kind: FoundationMultisigAccountType::StandaloneAccount,
                name: account_config.name()?,
                seed_owner_address: seed_account,
                seed_sequence_number: i,
            };
            standalone.push(account);
        }

        let accounts = Self {
            delegation_pool_owners,
            standalone,
        };

        Ok(accounts)
    }

    fn with_name_prefix(&self, prefix: &String) -> Result<&FoundationMultisigAccount, CliError> {
        let maybe_name = self
            .standalone
            .iter()
            .find(|a| a.name.starts_with(prefix))
            .or(self
                .delegation_pool_owners
                .iter()
                .find(|a| a.name.starts_with(prefix)));

        match maybe_name {
            Some(name) => Ok(name),
            None => Err(CliError::GeneralError(format!(
                "Missing account for name prefix: {prefix}"
            ))),
        }
    }

    fn write_to_file(&self) -> Result<(), CliError> {
        let json = serde_json::to_string_pretty(&self)
            // Serialization should never fail.
            .expect("Must be able to serialize a well-formed FoundationMultisigAccounts");
        write(
            GenesisConfigConstants::FOUNDATION_MULTISIG_ACCOUNTS_JSON,
            json,
        )?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FoundationOwnedMultisigAccount {
    /// The number of SUPRA tokens to be allocated to the account.
    balance: f64,
    /// The keys of the metadata to be associated with the account.
    metadata_keys: Vec<String>,
    /// The values of the metadata to be associated with the account.
    metadata_values: Vec<String>,
}

impl FoundationOwnedMultisigAccount {
    /// Convert the assigned `amount` of SUPRA to the equivalent number of Quants.
    fn balance(&self) -> u64 {
        to_quants(self.balance)
    }

    fn balance_in_supra(&self) -> u64 {
        self.balance as u64
    }

    fn metadata_values_serialized(&self) -> Vec<Vec<u8>> {
        self.metadata_values.iter().map(|v| v.to_bytes()).collect()
    }

    fn name(&self) -> Result<String, CliError> {
        let maybe_name_index = self.metadata_keys.iter().position(|k| k == "name");

        if let Some(i) = maybe_name_index {
            if self.metadata_values.len() > i {
                let name = self.metadata_values[i].clone();
                return Ok(name);
            }
        }

        Err(CliError::GeneralError(format!(
            "FoundationOwnedMultisigAccount is missing a name: {self:?}"
        )))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FoundationMultiSigAccountsConfig {
    /// The single-signer accounts designated to participate in the Supra Foundation Multisig.
    owners: Vec<AccountAddress>,
    num_signatures_required: u64,
    owner_balances_in_supra: u64,
    timeout_duration: u64,
    standalone_accounts: Vec<FoundationOwnedMultisigAccount>,
}

impl FoundationMultiSigAccountsConfig {
    /// The minimum number of individual participants in the Supra Foundation Mainnet Multisig.
    const MIN_PARTICIPANTS: usize = 9;

    fn additional_owners(&self) -> Vec<AccountAddress> {
        self.owners.iter().skip(1).cloned().collect()
    }

    fn allocated_supra(&self) -> u64 {
        // The amount allocated to each of the multisig accounts derived from the `owners`.
        self.standalone_accounts
            .iter()
            .fold(0, |total, account| total + account.balance_in_supra())
            // The amount allocated to the `owners` themselves.
            + self.owner_balances_in_supra * self.owners.len() as u64
    }

    fn first_owner(&self) -> AccountAddress {
        // This will never panic because of the verification performed in `from_file`.
        self.owners[0]
    }

    fn from_file(path: &String) -> Result<Self, CliError> {
        let config: Self = from_json_file_path(path)?;

        if config.owners.len() < Self::MIN_PARTICIPANTS {
            Err(GenesisToolError::NotEnoughFoundationMultisigMembers.into())
        } else {
            Ok(config)
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct FoundationValidatorsConsensusKeys {
    keys: Vec<PublicKey>,
}

impl FoundationValidatorsConsensusKeys {
    fn from_file(path: &String) -> Result<Self, CliError> {
        let keys: Vec<PublicKey> = from_json_file_path(path)?;
        let keys_count = keys.len();

        if keys_count < DELEGATION_POOLS_WITH_PBO_STAKE {
            Err(CliError::GeneralError(format!(
                "Expected at least {DELEGATION_POOLS_WITH_PBO_STAKE} foundation-operated \
                    validator consensus keys but found {keys_count}."
            )))
        } else {
            Ok(Self { keys })
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct DelegationPoolUnlockSchedule {
    /// The path to a file containing the [AccountBalances] of the participants in this delegation
    /// pool in JSON format.
    accounts_file_path: Option<String>,
    /// An identifier for the schedule. Intended to make it easier to understand
    /// what the schedule is for.
    id: String,
    /// The number of delegation pools that should be assigned this schedule.
    pools_with_delegators: usize,
    /// Numerator for unlock fraction
    unlock_schedule_numerators: Vec<u64>,
    /// Denominator for unlock fraction
    unlock_schedule_denominator: u64,
    /// Time from `timestamp::now_seconds()` to start unlocking schedule
    unlock_startup_time_from_now: u64,
    /// Time for each unlock
    unlock_period_duration: u64,
}

impl DelegationPoolUnlockSchedule {
    /// Returns the [AccountBalances] associated with this [DelegationPoolUnlockSchedule] by parsing the
    /// definitions provided in the `accounts_file_path`.
    fn accounts(&self) -> Result<Option<AccountBalances>, CliError> {
        if let Some(accounts_file_path) = &self.accounts_file_path {
            let accounts = AccountBalances::from_file(accounts_file_path)?;
            Ok(Some(accounts))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct DelegationPoolUnlockSchedules {
    /// The `id` of the [DelegationPoolUnlockSchedule] that should be assigned to all
    /// delegation pools that do not have schedules after the `pools_with_delegators`s of all
    /// schedules have been exhausted.
    default: String,
    /// The [DelegationPoolUnlockSchedule]s.
    schedules: Vec<DelegationPoolUnlockSchedule>,
    // Derived fields.
    //
    /// The default [DelegationPoolUnlockSchedule].
    #[serde(skip)]
    default_pool: DelegationPoolUnlockSchedule,
    /// Contains `pools_with_delegators` copies of each `DelegationPoolUnlockSchedule` in `schedules`.
    #[serde(skip)]
    schedules_expanded: Vec<DelegationPoolUnlockSchedule>,
}

impl DelegationPoolUnlockSchedules {
    fn accounts(&self) -> Result<AccountBalances, CliError> {
        let mut balances = Vec::new();
        for s in &self.schedules {
            if let Some(mut a) = s.accounts()? {
                balances.append(&mut a.balances);
            }
        }
        Ok(AccountBalances { balances })
    }

    fn allocated_supra(&self) -> Result<u64, CliError> {
        let sum = self.accounts()?.allocated_supra();
        Ok(sum)
    }

    // This is the only method that should be used to construct a [DelegationPoolUnlockSchedules].
    fn from_file(path: &String) -> Result<Self, CliError> {
        let mut schedules: Self = from_json_file_path(path)?;

        if let Some(default_pool) = schedules
            .schedules
            .iter()
            .find(|s| s.id == schedules.default)
        {
            schedules.default_pool = default_pool.clone();
            let expanded = schedules
                .schedules
                .iter()
                .flat_map(|s| vec![s.clone(); s.pools_with_delegators]);
            schedules.schedules_expanded = expanded.collect();
            Ok(schedules)
        } else {
            Err(GenesisToolError::MissingDefaultDelegationPoolSchedule.into())
        }
    }

    /// Returns the [DelegationPoolUnlockSchedule] for the ith delegation pool. Returns
    /// the `default` schedule if `i` is greater than the sum of the `pools_with_delegators`s of
    /// all [DelegationPoolUnlockSchedule]s. `i` is 0-indexed.
    fn ith_pool_schedule(&self, i: usize) -> &DelegationPoolUnlockSchedule {
        if i < self.schedules_expanded.len() {
            &self.schedules_expanded[i]
        } else {
            &self.default_pool
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct AccountBalance {
    /// The Move [AccountAddress] of the account.
    address: AccountAddress,
    /// The amount to be credited to the account at genesis.
    amount: f64,
}

impl AccountBalance {
    /// Convert the assigned `amount` of SUPRA to the equivalent number of Quants.
    fn balance(&self) -> u64 {
        to_quants(self.amount)
    }
}

impl Eq for AccountBalance {}

impl Hash for AccountBalance {
    /// [AccountBalance]s are unique by their `address`.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.address.hash(state);
    }
}

impl Ord for AccountBalance {
    /// [AccountBalance]s should be ordered by their `amount`.
    fn cmp(&self, other: &Self) -> Ordering {
        if self.amount == other.amount {
            Ordering::Equal
        } else if self.amount < other.amount {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }
}

impl PartialEq for AccountBalance {
    /// [AccountBalance]s are unique by their `address`.
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

impl PartialOrd for AccountBalance {
    /// [AccountBalance]s should be ordered by their `amount`.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct AccountBalances {
    balances: Vec<AccountBalance>,
}

impl AccountBalances {
    fn allocated_supra(&self) -> u64 {
        self.balances
            .iter()
            .fold(0, |total, balance| total + balance.balance())
            / SUPRA_COINS_BASE_WITH_DECIMALS
    }

    fn from_file(path: &String) -> Result<Self, CliError> {
        let balances: Vec<AccountBalance> = from_json_file_path(path)?;

        // Filter out any duplicates. We use an ordered set to ensure that the elements of the vec
        // that we ultimately store always have the same ordering given the same input.
        let unique_balances: BTreeSet<_> = balances.clone().into_iter().collect();

        // The code that converts the config into the genesis transaction does not support the
        // aggregation of multiple balance definitions for a single account, so all must be
        // unique across both the delegation and vesting pools.
        if unique_balances.len() == balances.len() {
            let balances: Vec<_> = unique_balances.into_iter().collect();
            Ok(Self { balances })
        } else {
            let duplicates = find_duplicates(balances);
            let dup_addrs = duplicates.iter().map(|dup| dup.address).collect();
            Err(GenesisToolError::AccountsNotUnique(dup_addrs).into())
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct VestingPoolUnlockSchedule {
    /// The path to a file containing the [VestingBalances] of the participants in this vesting
    /// pool in JSON format.
    accounts_file_path: String,
    vesting_pool_id: String,
    vpool_locking_percentage: u8,
    vesting_numerators: Vec<u64>,
    vesting_denominators: u64,
    cliff_period_in_seconds: u64,
    period_duration_in_seconds: u64,
}

impl VestingPoolUnlockSchedule {
    /// Returns the [VestingBalances] associated with this [VestingPoolUnlockSchedule] by adding the
    /// `vesting_pool_id` of this schedule to the associated [AccountBalance] definitions provided
    /// in the `accounts_file_path`.
    fn accounts(&self) -> Result<VestingBalances, CliError> {
        let accounts = AccountBalances::from_file(&self.accounts_file_path)?;
        let mut accounts_mapped = BTreeSet::new();

        for (i, chunk) in accounts
            .balances
            .chunks(GenesisConfigConstants::MAXIMUM_SHAREHOLDERS_PER_VESTING_POOL)
            .enumerate()
        {
            let contract_id = format!("{} {i}", self.vesting_pool_id.clone());
            let vesting_accounts: BTreeSet<VestingAccount> = chunk
                .iter()
                .map(|acc| VestingAccount::new(acc.address, acc.balance(), contract_id.clone()))
                .collect();
            accounts_mapped.extend(vesting_accounts);
        }

        Ok(VestingBalances {
            balances: accounts_mapped,
        })
    }

    /// Returns the prefix of the `name` metadata property of the corresponding Foundation Multisig
    /// account. This is equivalent to the vesting pool/contract id.
    fn admin_name_prefix(&self) -> &String {
        &self.vesting_pool_id
    }

    fn pool_ids(&self) -> Result<BTreeSet<String>, CliError> {
        // We do this to avoid duplicating the logic used to calculate the pool ids, even though
        // it is less efficient.
        let accounts = self.accounts()?;
        let ids = accounts
            .balances
            .iter()
            .map(|a| a.vesting_pool_id().clone())
            .collect();
        Ok(ids)
    }
}

#[derive(Debug)]
struct VestingPoolUnlockSchedules {
    schedules: Vec<VestingPoolUnlockSchedule>,
}

impl VestingPoolUnlockSchedules {
    fn account_balances(&self) -> Result<VestingBalances, CliError> {
        let mut balances = BTreeSet::new();

        for schedule in &self.schedules {
            let schedule_accounts = schedule.accounts()?;
            let duplicates: Vec<_> = schedule_accounts.balances.intersection(&balances).collect();

            // Ensure that all vesting accounts are unique across all files.
            if !duplicates.is_empty() {
                let addresses = duplicates
                    .iter()
                    .map(|dup| dup.address())
                    .cloned()
                    .collect();
                return Err(GenesisToolError::AccountsNotUnique(addresses).into());
            }

            balances.extend(schedule_accounts.balances);
        }

        Ok(VestingBalances { balances })
    }

    fn from_file(path: &String) -> Result<Self, CliError> {
        let schedules: Vec<VestingPoolUnlockSchedule> = from_json_file_path(path)?;
        Ok(Self { schedules })
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct VestingBalances {
    balances: BTreeSet<VestingAccount>,
}

impl VestingBalances {
    fn allocated_supra(&self) -> u64 {
        self.balances
            .iter()
            .fold(0, |total, balance| total + balance.amount())
            / SUPRA_COINS_BASE_WITH_DECIMALS
    }
}

struct MainnetGenesisAccountsGenerator {
    delegation_pool_schedules: DelegationPoolUnlockSchedules,
    foundation_multisig_accounts: FoundationMultisigAccounts,
    foundation_multisig_accounts_config: FoundationMultiSigAccountsConfig,
    foundation_validators_consensus_public_keys: FoundationValidatorsConsensusKeys,
    smr_committee_config: CommitteeConfigV1,
    vesting_accounts: VestingBalances,
    vesting_pool_schedules: VestingPoolUnlockSchedules,
}

impl MainnetGenesisAccountsGenerator {
    /// Returns the total amount of SUPRA allocated by the genesis
    fn allocated_supra(&self) -> Result<u64, CliError> {
        // The sum of SUPRA delegated by the foundation to the validators.
        let mut total = self.foundation_delegated_supra();
        // The sum of SUPRA allocated to PBO winners that decided to stake their winnings.
        total += self.delegation_pool_schedules.allocated_supra()?;
        // The sum of SUPRA allocated to foundation standalone multisig accounts for escrow etc.
        total += self.foundation_multisig_accounts_config.allocated_supra();
        // The sum of SUPRA allocated to accounts with vesting agreements at genesis, including the
        // PBO winners who chose the vesting option.
        total += self.vesting_accounts.allocated_supra();
        Ok(total)
    }

    /// Assigns the delegators to pools so that each pool ends up with a roughly similar
    /// amount of stake.
    fn assign_delegators_to_delegation_pools(
        &self,
        all_pools: &BTreeSet<DelegationPoolConfigV1>,
    ) -> Result<BTreeSet<DelegatorAccountV1>, CliError> {
        // Ensure that the PBO stake is split roughly evenly between the
        // delegation pools controlled by the foundation.
        let foundation_operated_pools = self.foundation_operated_delegation_pools(all_pools);

        let mut assigned = BTreeSet::new();
        let mut assignment_counts = HashMap::new();
        let mut pool_balances = Vec::new();

        for schedule in &self.delegation_pool_schedules.schedules {
            // The number of pools that have already had delegators assigned to them.
            let assigned_pools_count = pool_balances.len();
            // Take the next set of unassigned pools.
            let assigned_pools: Vec<_> = foundation_operated_pools
                .iter()
                .skip(assigned_pools_count)
                .take(schedule.pools_with_delegators)
                .collect();

            // The configuration must not try to assign delegators to more delegation pools
            // than the foundation has available at genesis.
            if assigned_pools.len() != schedule.pools_with_delegators {
                return Err(GenesisToolError::NotEnoughFoundationDelegationPools.into());
            }

            // Load the accounts to assign to them.
            let Some(mut delegators_to_assign) = schedule.accounts()? else {
                continue;
            };

            // Make room for the new balances.
            pool_balances.extend(vec![0; assigned_pools.len()]);
            // Sort the delegators by their balances. The [Ord] implementation of [AccountBalance]
            // only factors in the related `amount`.
            delegators_to_assign.balances.sort();

            for account_balance in delegators_to_assign.balances.iter() {
                let mut pool_index = 0;
                let mut min_balance = u64::MAX;

                // Find the pool that currently has the lowest balance.
                for (j, pool_balance) in pool_balances.iter().enumerate() {
                    if pool_balance < &min_balance {
                        min_balance = *pool_balance;
                        pool_index = j;
                    }
                }

                // Add the new delegator to that pool.
                let assigned_pool = assigned_pools[pool_index];
                let pool_owner_address = *assigned_pool.owner_address();
                let assigned_balance = DelegatorAccountV1::new(
                    account_balance.address,
                    account_balance.balance(),
                    pool_owner_address,
                );
                assigned.insert(assigned_balance);
                // Increment the pool's balance.
                pool_balances[pool_index] += account_balance.balance();
                // Keep track of the number of delegators assigned to each pool.
                assignment_counts
                    .entry(pool_owner_address)
                    .and_modify(|count: &mut usize| *count += 1)
                    .or_default();
            }
        }

        // Ensure that the assignments respect the upper bound.
        for (owner, count) in assignment_counts {
            if count > GenesisConfigConstants::MAXIMUM_DELEGATORS_PER_DELEGATION_POOL {
                return Err(CliError::GeneralError(format!(
                    "Too many delegators assigned to the delegation pool \
                    owned by {owner}. Count was: {count}."
                )));
            }
        }

        println!(
            "Allocated {} delegators to {} delegation pools",
            assigned.len(),
            pool_balances.len()
        );

        Ok(assigned)
    }

    /// The sum of the SUPRA delegated by the foundation to the validators.
    fn foundation_delegated_supra(&self) -> u64 {
        let genesis_committee_size = self.smr_committee_config.size();
        let foundation_delegated_stake_per_pool = self
            .smr_committee_config
            .genesis_params()
            .move_vm()
            .pbo_owner_stake();
        foundation_delegated_stake_per_pool / SUPRA_COINS_BASE_WITH_DECIMALS
            * genesis_committee_size as u64
    }

    fn foundation_operated_delegation_pools(
        &self,
        pools: &BTreeSet<DelegationPoolConfigV1>,
    ) -> BTreeSet<DelegationPoolConfigV1> {
        pools
            .iter()
            .filter(|p| {
                self.foundation_validators_consensus_public_keys
                    .keys
                    .contains(p.validator_consensus_public_key())
            })
            .cloned()
            .collect()
    }

    fn generate(&mut self) -> Result<(), CliError> {
        let delegation_pools = self.generate_delegation_pool_configs();
        let delegation_accounts = self.assign_delegators_to_delegation_pools(&delegation_pools)?;
        let vesting_pools = self.generate_vesting_pools_config()?;
        let foundation_accounts_schema = self.generate_foundation_multisig_accounts_schema();

        if foundation_accounts_schema.balance
            < *self
                .smr_committee_config
                .genesis_params()
                .move_vm()
                .pbo_owner_stake()
        {
            return Err(CliError::GenesisToolError(
                GenesisToolError::InvalidPBOOwnerStake(
                    self.smr_committee_config
                        .genesis_params()
                        .move_vm()
                        .pbo_owner_stake()
                        - foundation_accounts_schema.balance,
                    foundation_accounts_schema.balance,
                ),
            ));
        }

        let validator_owner_accounts = self.generate_foundation_multisig_owner_accounts();
        let multisig_accounts = self.generate_foundation_standalone_multisig_accounts();

        let config = GenesisAccountsV1::new(
            delegation_pools,
            delegation_accounts,
            vesting_pools,
            self.vesting_accounts.balances.clone(),
            // No standalone accounts for mainnet except foundation MultiSig Owners.
            // All non-foundation-multisig accounts must be subject to a vesting/locking schedule.
            validator_owner_accounts,
            multisig_accounts,
            foundation_accounts_schema,
        );

        // Write the generated data to the output files defined in [GenesisConfigConstants].
        config.write_as_genesis_config()?;
        self.foundation_multisig_accounts.write_to_file()
    }

    fn generate_delegation_pool_configs(&self) -> BTreeSet<DelegationPoolConfigV1> {
        let operator_consensus_keys = self
            .smr_committee_config
            .members()
            .iter()
            .map(|m| m.1.node_public_identity.consensus_public_key);

        let mut pool_configs = BTreeSet::new();
        for (i, operator) in operator_consensus_keys.enumerate() {
            let unlock_schedule = self.delegation_pool_schedules.ith_pool_schedule(i);
            // Already ensured that there are at least as many Foundation multisig accounts as there
            // are validator node operator consensus public keys.
            let owner = &self.foundation_multisig_accounts.delegation_pool_owners[i];
            let seed = format!("{} {} {i}", unlock_schedule.id, DELEGATION_POOL_SEED_PREFIX);
            let pool = DelegationPoolConfigV1::new(
                owner.address,
                seed,
                operator,
                // We use the same account for both the owner and the admin of the pool so that
                // all foundation-authorized operations related to a specific pool are carried out
                // by the same account.
                owner.address,
                unlock_schedule.unlock_schedule_numerators.clone(),
                unlock_schedule.unlock_schedule_denominator,
                unlock_schedule.unlock_startup_time_from_now,
                unlock_schedule.unlock_period_duration,
            );
            pool_configs.insert(pool);
        }

        pool_configs
    }

    fn generate_foundation_multisig_owner_accounts(
        &self,
    ) -> BTreeSet<aptos_vm_genesis::AccountBalance> {
        let config = &self.foundation_multisig_accounts_config;
        let owner_balances: BTreeSet<aptos_vm_genesis::AccountBalance> = config
            .owners
            .iter()
            .map(|addr| aptos_vm_genesis::AccountBalance {
                account_address: *addr,
                balance: config.owner_balances_in_supra * SUPRA_COINS_BASE_WITH_DECIMALS,
            })
            .collect();
        owner_balances
    }

    fn generate_foundation_multisig_accounts_schema(&self) -> MultiSigAccountSchema {
        let config = &self.foundation_multisig_accounts_config;
        let genesis_committee_size = self.smr_committee_config.size();
        MultiSigAccountSchema {
            owner: config.first_owner(),
            additional_owners: config.additional_owners(),
            num_signatures_required: config.num_signatures_required,
            metadata_keys: vec!["name".to_string()],
            metadata_values: vec!["Supra Foundation Validator Owner".to_bytes()],
            timeout_duration: config.timeout_duration,
            balance: *self
                .smr_committee_config
                .genesis_params()
                .move_vm()
                .pbo_owner_stake(),
            num_of_accounts: genesis_committee_size as u32,
        }
    }

    fn generate_foundation_standalone_multisig_accounts(
        &self,
    ) -> BTreeSet<MultiSigAccountWithBalance> {
        let mut accounts = BTreeSet::new();
        let config = &self.foundation_multisig_accounts_config;

        for (i, account) in config.standalone_accounts.iter().enumerate() {
            // The collection of multisig accounts in the Rust layer is ordered and this ordering
            // includes the `multisig_address` field, so if we try to put the actual multisig
            // address here we will most likely end up changing the ordering of the entry, which
            // in turn will change the multisig address generated by the VM. The `multisig_address`
            // field should actually be called `id` in this context, because it is only intended
            // to be used to make instances of the type unique. Accordingly, we just set a dummy
            // value here.
            let placeholder_id = AccountAddress::from_str(format!("{i}").as_str())
                .expect("Must be able to convert usize to AccountAddress");
            let foundation_multisig = MultiSigAccountWithBalance {
                owner: config.first_owner(),
                additional_owners: config.additional_owners(),
                multisig_address: placeholder_id,
                num_signatures_required: config.num_signatures_required,
                metadata_keys: account.metadata_keys.clone(),
                metadata_values: account.metadata_values_serialized(),
                timeout_duration: config.timeout_duration,
                balance: account.balance(),
            };
            accounts.insert(foundation_multisig);
        }

        accounts
    }

    fn generate_vesting_pools_config(&self) -> Result<BTreeSet<VestingPoolConfig>, CliError> {
        let mut vesting_pools = BTreeSet::new();

        for s in &self.vesting_pool_schedules.schedules {
            // Find the Foundation Multisig Account with the same name as the schedule.
            let admin_name_prefix = s.admin_name_prefix();
            let foundation_admin = self
                .foundation_multisig_accounts
                .with_name_prefix(admin_name_prefix)?
                .address;
            let schedule_pools_ids = s.pool_ids()?;

            // Create each pool for the schedule. A schedule may end up with many pools if it has
            // many associated accounts.
            for id in schedule_pools_ids {
                let pool = VestingPoolConfig::new(
                    id,
                    foundation_admin,
                    s.vpool_locking_percentage,
                    s.vesting_numerators.clone(),
                    s.vesting_denominators,
                    foundation_admin,
                    s.cliff_period_in_seconds,
                    s.period_duration_in_seconds,
                );
                vesting_pools.insert(pool);
            }
        }

        Ok(vesting_pools)
    }

    fn verify(&self) -> Result<(), CliError> {
        let total = self.allocated_supra()?;
        println!("SUPRA allocated at genesis: {total}");

        // Ensure that no more than the allocated amount is minted.
        if total > SUPRA_MINTED_AT_GENESIS {
            return Err(
                GenesisToolError::MintableSupplyExceeded(total, SUPRA_MINTED_AT_GENESIS).into(),
            );
        }

        // Ensure that the expected amount is allocated and that type conversions do not cause
        // significant losses.
        if total < SUPRA_MINTED_AT_GENESIS - ERROR_MARGIN {
            return Err(
                GenesisToolError::InsufficientAllocation(total, SUPRA_MINTED_AT_GENESIS).into(),
            );
        }

        let delegators = self.delegation_pool_schedules.accounts()?;
        let delegator_accounts: HashSet<_> =
            delegators.balances.iter().map(|b| &b.address).collect();
        let vesting_accounts: HashSet<_> = self
            .vesting_accounts
            .balances
            .iter()
            .map(|b| b.address())
            .collect();
        let common_accounts: Vec<AccountAddress> = delegator_accounts
            .intersection(&vesting_accounts)
            .cloned()
            .cloned()
            .collect();

        if !common_accounts.is_empty() {
            return Err(GenesisToolError::AccountsNotUnique(common_accounts).into());
        }

        Ok(())
    }
}

impl TryFrom<&GenerateMainnetAccountsConfig> for MainnetGenesisAccountsGenerator {
    type Error = CliError;

    fn try_from(cli_args: &GenerateMainnetAccountsConfig) -> Result<Self, Self::Error> {
        let foundation_multisig_accounts_config = FoundationMultiSigAccountsConfig::from_file(
            &cli_args.foundation_multisig_accounts_config_path,
        )?;

        let supra_committees =
            SupraCommitteesConfigV1::read_from_file(&cli_args.supra_committees_path)?;
        let smr_committee_config = supra_committees
            .get_smr_committee_config()
            .expect("Must contain an SMR Committee Config.")
            .clone();
        let genesis_committee_size = smr_committee_config.size();

        let foundation_multisig_accounts = FoundationMultisigAccounts::new(
            &foundation_multisig_accounts_config,
            genesis_committee_size,
        )?;
        let foundation_validators_consensus_public_keys =
            FoundationValidatorsConsensusKeys::from_file(
                &cli_args.foundation_validators_consensus_public_keys_path,
            )?;

        let delegation_pool_schedules =
            DelegationPoolUnlockSchedules::from_file(&cli_args.delegation_pool_schedules_path)?;
        let vesting_pool_schedules =
            VestingPoolUnlockSchedules::from_file(&cli_args.vesting_pool_schedules_path)?;
        let vesting_accounts = vesting_pool_schedules.account_balances()?;

        let generator = MainnetGenesisAccountsGenerator {
            delegation_pool_schedules,
            foundation_multisig_accounts,
            foundation_multisig_accounts_config,
            foundation_validators_consensus_public_keys,
            smr_committee_config,
            vesting_accounts,
            vesting_pool_schedules,
        };
        // Ensure that we can only construct valid generators.
        generator.verify()?;

        Ok(generator)
    }
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct GenerateMainnetAccountsConfig {
    /// The path to the file containing the configuration settings to be applied to all of the of
    /// the Supra Foundation Multisig Accounts. The file must contain a [FoundationMultiSigAccountsConfig]
    /// in JSON format.
    #[clap(long)]
    pub foundation_multisig_accounts_config_path: String,

    /// The path to a file containing a JSON list of the hex-encoded ED25519 consensus public keys
    /// of the validators to be operated by The Supra Foundation at genesis.
    #[clap(long)]
    pub foundation_validators_consensus_public_keys_path: String,

    /// The path to the PBO Delegation Pool Unlock Schedule file. The file must contain a
    /// [DelegationPoolUnlockSchedules] in JSON format.
    #[clap(long)]
    pub delegation_pool_schedules_path: String,

    /// The path to the Supra Committees file. The file must contain a [SupraCommitteesConfig] in
    /// JSON format.
    #[clap(long)]
    pub supra_committees_path: String,

    /// The path to the Vesting Pool Unlock Schedules file. The file must contain a list of
    /// [VestingPoolUnlockSchedule]s in JSON format.
    #[clap(long)]
    pub vesting_pool_schedules_path: String,
}

impl GenerateMainnetAccountsConfig {
    pub fn execute(self) -> Result<GenesisToolResult, CliError> {
        let mut generator = MainnetGenesisAccountsGenerator::try_from(&self)?;
        generator.generate()?;
        Ok(GenesisToolResult::Blank)
    }
}

fn find_duplicates<T: Eq + std::hash::Hash + Clone>(vec: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();

    vec.into_iter()
        .filter(|item| {
            if !seen.insert(item.clone()) {
                duplicates.insert(item.clone())
            } else {
                false
            }
        })
        .collect::<Vec<_>>()
}

/// Converts the assigned `amount_in_supra` of SUPRA to the equivalent number of Quants.
fn to_quants(amount_in_supra: f64) -> u64 {
    // The cast to [u64] will truncate any remaining decimals, but these are negligible.
    (amount_in_supra * SUPRA_COINS_BASE_WITH_DECIMALS as f64) as u64
}
