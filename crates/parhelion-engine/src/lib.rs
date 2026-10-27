//! Simulation of Soroban invocations against a Parhelion fork.

pub mod fork;

pub use fork::{Fork, ForkOptions};
pub use parhelion_state::xdr;

/// Errors of this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Rpc(#[from] parhelion_state::rpc::Error),
    #[error(transparent)]
    Fetch(#[from] parhelion_state::fetcher::FetchError),
    /// The fetcher thread or the cache directory could not be set up.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The network's `ConfigSetting` entries are missing or inconsistent.
    #[error("network configuration: {0:#}")]
    NetworkConfig(#[source] anyhow::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
