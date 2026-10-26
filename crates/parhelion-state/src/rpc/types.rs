//! Decoded `getLedgerEntries` result. Response shape and sample:
//! <https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getLedgerEntries>

use crate::xdr::{LedgerEntry, LedgerKey};

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
