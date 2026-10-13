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
    pub live_until_ledger_seq: Option<u32>,
}
