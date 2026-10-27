//! Client for the remote Stellar RPC that backs the fork (layer L2).
//!
//! Knows HTTP and JSON-RPC, nothing about the store layers above it.
//! Method reference: <https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods>

mod client;
mod error;
mod get_latest_ledger;
mod get_ledger_entries;
mod get_network;
mod jsonrpc;
mod types;

pub use client::RpcClient;
pub use error::{Error, Result};
pub use get_ledger_entries::MAX_KEYS_PER_REQUEST;
pub use types::{LatestLedger, LedgerEntries, Network, RemoteEntry};
