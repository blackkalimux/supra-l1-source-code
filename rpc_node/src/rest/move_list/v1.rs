use move_core_types::language_storage::{ModuleId, StructTag};
use serde::{Deserialize, Serialize};
use types::serde::optional_hex;
use utoipa::ToSchema;
use version_derive::TakeVariant;

#[derive(Debug, Deserialize, Serialize, TakeVariant, ToSchema)]
pub enum MoveList {
    Resources(MoveResourceList),
    Modules(MoveModuleList),
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Hash, ToSchema)]
pub struct MoveResourceList {
    #[schema(value_type = Vec<(String, types::api::delegates::StructTag)>)]
    pub resource: Vec<(String, StructTag)>,
    /// Cursor specifying where to start for pagination.
    /// Use the cursor returned by the API when making the next request.
    #[serde(with = "optional_hex", default)]
    pub cursor: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct MoveModuleList {
    #[schema(value_type = Vec<(String, types::api::delegates::ModuleId)>)]
    pub modules: Vec<(String, ModuleId)>,
    /// Cursor specifying where to start for pagination.
    /// Use the cursor returned by the API when making the next request.
    #[serde(with = "optional_hex", default)]
    pub cursor: Option<Vec<u8>>,
}
