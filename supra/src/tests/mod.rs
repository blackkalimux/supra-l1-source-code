#![allow(clippy::unwrap_used)]

mod rest_client;

use crate::SupraTool;

#[test]
fn verify_tool() {
    use clap::CommandFactory;
    SupraTool::command().debug_assert()
}
