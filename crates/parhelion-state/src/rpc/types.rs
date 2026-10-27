//! Decoded results of the RPC methods. Response shapes and samples:
//! <https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods>

use crate::xdr::{
    LedgerEntry, LedgerKey, ScVal, SorobanAuthorizationEntry, SorobanTransactionData,
};

/// `getLatestLedger` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestLedger {
    pub sequence: u32,
    pub protocol_version: u32,
    /// Unix seconds.
    pub close_time: u64,
}

/// `getNetwork` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    pub passphrase: String,
    pub protocol_version: u32,
}

/// `simulateTransaction` result.
#[derive(Debug, Clone, PartialEq)]
pub struct SimulateTransactionResult {
    pub latest_ledger: u32,
    /// Footprint, resources and resource fee; `None` when the simulation failed.
    pub transaction_data: Option<SorobanTransactionData>,
    /// Stroops.
    pub min_resource_fee: Option<i64>,
    /// One per host function; a transaction holds at most one.
    pub results: Vec<HostFunctionResult>,
    /// Why the simulation failed.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HostFunctionResult {
    pub return_value: ScVal,
    pub auth: Vec<SorobanAuthorizationEntry>,
}

/// `getLedgerEntries` result.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerEntries {
    pub latest_ledger: u32,
    /// Absent keys are omitted.
    pub entries: Vec<RemoteEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteEntry {
    pub key: LedgerKey,
    pub entry: LedgerEntry,
    /// `None` for entries without a TTL. An archived Soroban entry is still returned, with a
    /// value below the current ledger (the RPC sends `0`).
    pub live_until_ledger_seq: Option<u32>,
}
