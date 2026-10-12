//! Lazy mainnet ledger state for the Soroban host.

/// Stellar XDR types, re-exported from the pinned host so every crate uses the same
/// `stellar-xdr` version as the host. Never depend on `stellar-xdr` directly.
pub use soroban_env_host_curr::xdr;
