#[derive(Debug, thiserror::Error)]
pub enum IdentityRotationError {
    #[error("Rotating consensus key failed: {0}")]
    ConsensusKey(String),
    #[error("Rotating network address failed: {0}")]
    NetworkAddress(String),
}
