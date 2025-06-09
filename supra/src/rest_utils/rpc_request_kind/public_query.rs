use types::impl_struct_create_and_read_methods;
use url::Url;

#[derive(Clone, PartialEq)]
pub struct PublicQueryHelper {
    url: Url,
}

impl_struct_create_and_read_methods!(PublicQueryHelper { url: Url });
