use crate::gas::gas_monitor;
use crate::rest::api_server_state_builder::ApiServerStateBuilder;
use crate::rest::ApiServerState;
use crate::transactions::dispatcher::TransactionDispatcher;
use crate::RpcStorage;
use aptos_api_types::IdentifierWrapper;
use archive::tests::utilities::new_db_with_cache_size;
use core::str::FromStr;
use mempool::MoveSequenceNumberProvider;
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use serde::Deserialize;
use std::collections::HashSet;
use test_utils::execution_utils;
use types::api::v1::MoveListQuery;

fn api_server_state() -> ApiServerState {
    let executor = execution_utils::setup_executor();
    let (archive_db, store) = new_db_with_cache_size(10);
    let (_, gas_price_reader) = gas_monitor();

    let rpc_storage = RpcStorage::new(
        archive_db,
        MoveSequenceNumberProvider::new(executor.move_store.clone()),
        &Default::default(),
    );

    ApiServerStateBuilder::new()
        .storage(rpc_storage.clone())
        .tx_sender(TransactionDispatcher::default())
        .move_store(executor.move_store.clone())
        .store(store)
        .gas_price_reader(gas_price_reader)
        .build()
}

#[test]
fn test_read_account_resources() {
    let api_server_state = api_server_state();

    let extended_addr_01 = "0x0000000000000000000000000000000000000000000000000000000000000001"; // root account
    let extended_addr_05 = "0x0000000000000000000000000000000000000000000000000000000000000005"; // standard account

    let address_01 = AccountAddress::from_str(extended_addr_01).unwrap(); // root account
    let address_05 = AccountAddress::from_str(extended_addr_05).unwrap(); // standard account

    // address_05 should just have an account struct, represents an account has been created for validator node.
    let resource_under_addr5 = api_server_state
        .get_move_list(address_05, None, 1, MoveListQuery::Resources)
        .expect("fail to read account path for addr");
    let resource_under_addr5 = resource_under_addr5.0.take_resourcelist().unwrap();
    assert_eq!(resource_under_addr5.resources.len(), 1);
    assert_eq!(
        resource_under_addr5.resources[0],
        StructTag {
            address: address_01,
            module: Identifier::new("account").unwrap(),
            name: Identifier::new("Account").unwrap(),
            type_args: vec![]
        }
    );

    // total resource under root account
    let resource_under_addr1 = api_server_state
        .get_move_list(address_01, None, 100, MoveListQuery::Resources)
        .expect("fail to read account path for addr")
        .0
        .take_resourcelist()
        .unwrap();
    // Note that the expected value needs to be updated whenever a new contract/resource is added to the Supra Move Framework.
    let keyless_account_resource_group_expected = api_server_state
        .read_resource_group(
            &address_01,
            &StructTag::from_str("0x1::keyless_account::Group").unwrap(),
        )
        .unwrap()
        .iter()
        .map(|(s, _)| s)
        .cloned()
        .collect::<HashSet<StructTag>>();

    let keyless_account_resource_group_actual = resource_under_addr1
        .resources
        .iter()
        .cloned()
        .collect::<HashSet<StructTag>>()
        .into_iter()
        .filter(|s| s.module == Identifier::new("keyless_account").unwrap())
        .collect::<HashSet<StructTag>>();

    assert_eq!(
        keyless_account_resource_group_actual,
        keyless_account_resource_group_expected
    );
    assert_eq!(keyless_account_resource_group_actual.len(), 2);

    // account resource list pagination
    let mut resources_on_page_hashset = HashSet::new();
    let mut resources_on_page_vec = Vec::new();

    let mut cursor = Option::<Vec<u8>>::None;
    let mut page_count = 0;
    for _ in 0..100 {
        let (page_listing, curser) = api_server_state
            .get_move_list(address_01, cursor.clone(), 1, MoveListQuery::Resources)
            .expect("fail to read account path for addr");
        page_count += if page_listing.is_empty() { 0 } else { 1 };
        assert!(
            page_listing.len() == 1 || page_listing.is_empty(),
            "Inconsistent page length"
        );
        resources_on_page_hashset
            .extend(page_listing.clone().take_resourcelist().unwrap().resources);
        resources_on_page_vec.extend(page_listing.take_resourcelist().unwrap().resources);
        cursor = curser;
    }
    assert_eq!(page_count, 53);
    assert_eq!(resources_on_page_hashset.len(), 53);
    assert_eq!(resources_on_page_vec.len(), 53);
    assert_eq!(resource_under_addr1.resources.len(), 53);
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Module {
    extension: Extension,
    name: String,
    source: String,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Extension {
    vec: Vec<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Entry {
    deps: Vec<serde_json::Value>,
    extension: Extension,
    manifest: String,
    modules: Vec<Module>,
}

#[test]
fn test_read_account_modules() {
    let api_server_state = api_server_state();

    let extended_addr_01 = "0x0000000000000000000000000000000000000000000000000000000000000001"; // root account

    let address_01 = AccountAddress::from_str(extended_addr_01).unwrap(); // root account

    let binding = api_server_state
        .get_move_resource(
            address_01,
            &StructTag::from_str("0x1::code::PackageRegistry").unwrap(),
        )
        .unwrap()
        .unwrap();
    let pkg_reg = binding
        .data
        .0
        .get(&IdentifierWrapper(Identifier::new("packages").unwrap()))
        .unwrap();
    let entries: Vec<Entry> = serde_json::from_value(pkg_reg.clone()).expect("Invalid JSON");

    let total_modules: usize = entries.iter().map(|e| e.modules.len()).sum();

    // total resource under root account
    // Note that the expected value needs to be updated whenever a new contract/resource is added to the Supra Move Framework.
    let modules_of_root_account = api_server_state
        .get_move_list(address_01, None, 150, MoveListQuery::Modules)
        .expect("fail to read account path for addr")
        .0
        .take_modulelist()
        .unwrap();

    // account module list pagination
    let mut modules_on_page_hashset = HashSet::new();
    let mut modules_on_page_vec = Vec::new();

    let mut cursor = Option::<Vec<u8>>::None;
    let mut page_count = 0;
    for _ in 0..150 {
        let (page_listing, curser) = api_server_state
            .get_move_list(address_01, cursor.clone(), 1, MoveListQuery::Modules)
            .expect("fail to read account path for addr");
        page_count += if page_listing.is_empty() { 0 } else { 1 };
        assert!(
            page_listing.len() == 1 || page_listing.is_empty(),
            "Inconsistent page length"
        );
        modules_on_page_hashset.extend(page_listing.clone().take_modulelist().unwrap().modules);
        modules_on_page_vec.extend(page_listing.take_modulelist().unwrap().modules);
        cursor = curser;
    }
    assert_eq!(page_count, total_modules);
    assert_eq!(modules_on_page_hashset.len(), total_modules);
    assert_eq!(modules_on_page_vec.len(), total_modules);
    assert_eq!(modules_of_root_account.modules.len(), total_modules);
}
