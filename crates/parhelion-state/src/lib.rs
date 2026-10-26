//! Lazy mainnet ledger state for the Soroban host.

pub mod fetcher;
pub mod rpc;
pub mod store;

#[cfg(test)]
mod test_utils;

/// Stellar XDR types, re-exported from the pinned host so every crate uses the same
/// `stellar-xdr` version as the host. Never depend on `stellar-xdr` directly.
pub use soroban_env_host_curr::xdr;
