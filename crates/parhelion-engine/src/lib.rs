//! Simulation of Soroban invocations against a Parhelion fork.

pub mod fork;
pub mod simulate;

pub use fork::{Fork, ForkOptions};
pub use parhelion_state::xdr;
pub use simulate::{invoke_contract, Simulation};

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
    #[error("XDR error: {0}")]
    Xdr(#[from] xdr::Error),
    /// The network's `ConfigSetting` entries are missing or inconsistent.
    #[error("network configuration: {0:#}")]
    NetworkConfig(#[source] anyhow::Error),
    /// The simulation itself could not run; a failing invocation is not an error here.
    #[error("simulation: {0:#}")]
    Simulation(#[source] anyhow::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
