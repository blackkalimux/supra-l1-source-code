use crate::rest::move_list::MoveListError;
use crate::rest::ApiServerState;
use aptos_types::access_path::Path;
use aptos_types::state_store::state_key::inner::StateKeyInner;
use aptos_types::state_store::state_key::prefix::StateKeyPrefix;
use aptos_types::state_store::state_key::StateKey;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use rocksdb::{IteratorMode, PrefixRange, ReadOptions};
use serde::{Deserialize, Serialize};
use soserde::SmrSerialize;
use types::api::v1::MoveListQuery;
use utoipa::ToSchema;
use version_derive::TakeVariant;

/// A cursor used for paginating Move list.
#[derive(Deserialize, Serialize)]
pub struct PaginationCursor {
    state_key: StateKey,
    resource_group_item_index: Option<usize>,
}

impl PaginationCursor {
    /// Deserialize a [PaginationCursor] from BCS-encoded bytes.
    fn from_bcs_bytes(b: &[u8]) -> bcs::Result<PaginationCursor> {
        bcs::from_bytes::<Self>(b)
    }

    /// Serialize the [PaginationCursor] to BCS-encoded bytes.
    fn to_bcs_bytes(&self) -> bcs::Result<Vec<u8>> {
        bcs::to_bytes(self)
    }
}

/// Builder for constructing a Move list with pagination.
pub struct MoveListBuilder<'a> {
    api_state: &'a ApiServerState,
    account_address: AccountAddress,
    pagination_cursor: Option<PaginationCursor>,
    max_items_to_return: usize,
    move_list: MoveListV3,
}

impl<'a> MoveListBuilder<'a> {
    pub fn new(
        api_state: &'a ApiServerState,
        account_address: AccountAddress,
        query_kind: MoveListQuery,
        cursor: Option<Vec<u8>>,
        max_items_to_return: usize,
    ) -> Result<Self, MoveListError> {
        Ok(Self {
            api_state,
            account_address,
            pagination_cursor: cursor
                .map(|c| PaginationCursor::from_bcs_bytes(&c))
                .transpose()?,
            max_items_to_return,
            move_list: MoveListV3::new(&query_kind),
        })
    }

    /// Build the move list using the provided cursor and remaining count.
    pub fn build_move_list(mut self) -> Result<(MoveListV3, Option<Vec<u8>>), MoveListError> {
        if self.max_items_to_return == 0 {
            return Ok((
                self.move_list,
                self.pagination_cursor
                    .as_ref()
                    .and_then(|cursor| cursor.to_bcs_bytes().ok()),
            ));
        }

        let address_prefix = StateKeyPrefix::from(self.account_address)
            .encode()
            .map_err(MoveListError::from)?;

        let mut read_options = ReadOptions::default();
        read_options.set_iterate_range(PrefixRange(address_prefix.clone()));

        let move_state_index_cf = self.api_state.move_store.cf_move_state_index();

        let start_key = if let Some(ref cursor) = self.pagination_cursor {
            cursor.state_key.to_bytes()
        } else {
            address_prefix
        };

        let mut iter = self.api_state.move_store.db.iterator_cf_opt(
            move_state_index_cf,
            read_options,
            IteratorMode::From(start_key.as_slice(), rocksdb::Direction::Forward),
        );

        // If there is a cursor which has some resource_group_item_index then continue to paginate resource_group
        // else start with next StateKey because the StateKey is the last StateKey that was returned in the previous page.
        if let Some(ref cursor) = self.pagination_cursor {
            if cursor.resource_group_item_index.is_none() {
                iter.next();
            }
        }

        for entry in iter {
            if self.max_items_to_return == 0 {
                break;
            }
            let (key, _value_bytes) = entry.map_err(MoveListError::RocksDbError)?;
            let state_key =
                StateKey::decode(key.as_ref()).map_err(MoveListError::StateKeyDecode)?;

            let resource_group_item_index = match &self.pagination_cursor {
                Some(cursor) if cursor.state_key == state_key => cursor.resource_group_item_index,
                _ => None,
            };
            let mut current_cursor = PaginationCursor {
                state_key: state_key.clone(),
                resource_group_item_index,
            };
            let list_size_before_append = self.move_list.len();
            self.move_list.append_entry(
                &self.account_address,
                self.api_state,
                &mut current_cursor,
                self.max_items_to_return,
            )?;

            self.max_items_to_return = self
                .max_items_to_return
                .saturating_sub(self.move_list.len() - list_size_before_append);
            self.pagination_cursor = Some(current_cursor);
        }

        Ok((
            self.move_list,
            self.pagination_cursor
                .and_then(|cursor| cursor.to_bcs_bytes().ok()),
        ))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, TakeVariant)]
pub enum MoveListV3 {
    #[schema(value_type = Object)]
    ResourceList(ResourceIdList),
    #[schema(value_type = Object)]
    ModuleList(ModuleIdList),
}

/// List of the [StructTag]s of the given [AccountAddress].
#[derive(Default, Clone, Serialize, Deserialize, Debug, Eq, PartialEq, Hash)]
pub struct ResourceIdList {
    pub resources: Vec<StructTag>,
}

/// List of the [ModuleId]s of the given [AccountAddress].
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct ModuleIdList {
    pub modules: Vec<ModuleId>,
}

impl MoveListV3 {
    pub fn new(query_kind: &MoveListQuery) -> Self {
        match query_kind {
            MoveListQuery::Modules => MoveListV3::ModuleList(ModuleIdList::default()),
            MoveListQuery::Resources => MoveListV3::ResourceList(ResourceIdList::default()),
        }
    }

    pub fn append_entry(
        &mut self,
        account_address: &AccountAddress,
        api_state: &ApiServerState,
        cursor: &mut PaginationCursor,
        max_items_to_return: usize,
    ) -> Result<(), MoveListError> {
        match self {
            MoveListV3::ResourceList(resource_list) => resource_list.maybe_append_resource(
                account_address,
                api_state,
                cursor,
                max_items_to_return,
            )?,
            MoveListV3::ModuleList(module_list) => {
                module_list.maybe_append_module(&cursor.state_key)
            }
        };
        Ok(())
    }

    pub fn len(&self) -> usize {
        match self {
            MoveListV3::ResourceList(r) => r.resources.len(),
            MoveListV3::ModuleList(m) => m.modules.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ResourceIdList {
    pub fn maybe_append_resource(
        &mut self,
        account_address: &AccountAddress,
        api_state: &ApiServerState,
        cursor: &mut PaginationCursor,
        max_resources_to_append: usize,
    ) -> Result<(), MoveListError> {
        if max_resources_to_append == 0 {
            return Ok(());
        }
        if let StateKeyInner::AccessPath(access_path) = cursor.state_key.inner() {
            return match access_path.get_path() {
                Path::Code(_module_id) => Ok(()),
                Path::Resource(struct_tag) => {
                    self.resources.push(struct_tag);
                    Ok(())
                }
                Path::ResourceGroup(struct_tag) => {
                    let group_items = api_state
                        .read_resource_group(account_address, &struct_tag)
                        .ok_or(MoveListError::MissingResourceGroupData(
                            struct_tag.to_string(),
                        ))?;
                    let start_index = cursor.resource_group_item_index.unwrap_or(0);
                    if start_index >= group_items.len() {
                        return Ok(());
                    }
                    let end_index =
                        usize::min(start_index + max_resources_to_append, group_items.len());
                    // considering the slice of group_items[this_page_start_index, next_page_start_index)
                    for (tag, _bytes) in &group_items[start_index..end_index] {
                        self.resources.push(tag.clone());
                    }
                    *cursor = PaginationCursor {
                        state_key: cursor.state_key.clone(),
                        resource_group_item_index: Some(end_index), // next_page_start_index,
                    };
                    Ok(())
                }
            };
        }
        Ok(())
    }
}

impl ModuleIdList {
    pub fn maybe_append_module(&mut self, state_key: &StateKey) {
        if let StateKeyInner::AccessPath(access_path) = state_key.inner() {
            match access_path.get_path() {
                Path::Code(module_id) => {
                    self.modules.push(module_id);
                }
                Path::Resource(_struct_tag) | Path::ResourceGroup(_struct_tag) => {}
            }
        }
    }
}
