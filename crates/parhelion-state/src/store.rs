//! The layered ledger store that feeds the Soroban host.
//!
//! A read goes through three layers and stops at the first that knows the key:
//! L0 local writes, L1 cache of everything already fetched, L2 the remote.

use std::collections::HashMap;
use std::rc::Rc;
use std::slice;
use std::sync::RwLock;

use soroban_env_host_curr::storage::{EntryWithLiveUntil, SnapshotSource};
use soroban_env_host_curr::HostError;

use crate::fetcher::{FetchError, FetchResult, Fetcher};
use crate::xdr::{LedgerEntry, LedgerKey, ScErrorCode, ScErrorType};

/// A ledger entry and the ledger it lives until (`None` for entries without a TTL).
pub type Entry = (LedgerEntry, Option<u32>);

/// Where entries the store has never seen come from.
pub trait RemoteSource {
    /// Results in the order of `keys`; `None` means the key does not exist.
    fn fetch(&self, keys: &[LedgerKey]) -> Vec<FetchResult>;
}

impl RemoteSource for Fetcher {
    fn fetch(&self, keys: &[LedgerKey]) -> Vec<FetchResult> {
        Fetcher::fetch(self, keys)
    }
}

pub struct LayeredStore<R> {
    /// L0. `None` is a deletion and hides whatever the layers below hold.
    local: RwLock<HashMap<LedgerKey, Option<Entry>>>,
    /// L1. `None` remembers that the remote does not have the key.
    cache: RwLock<HashMap<LedgerKey, Option<Entry>>>,
    /// L2.
    remote: R,
}

impl<R: RemoteSource> LayeredStore<R> {
    pub fn new(remote: R) -> Self {
        Self {
            local: Default::default(),
            cache: Default::default(),
            remote,
        }
    }

    /// Writes `key` locally; `None` deletes it. The remote is never consulted for it again.
    pub fn write_local(&self, key: LedgerKey, entry: Option<Entry>) {
        self.local.write().unwrap().insert(key, entry);
    }

    fn lookup(&self, key: &LedgerKey) -> Result<Option<Entry>, FetchError> {
        if let Some(entry) = self.local.read().unwrap().get(key) {
            return Ok(entry.clone());
        }
        if let Some(entry) = self.cache.read().unwrap().get(key) {
            return Ok(entry.clone());
        }
        // No lock is held while fetching; the fetcher merges concurrent misses on the same key.
        let fetched = self
            .remote
            .fetch(slice::from_ref(key))
            .pop()
            .unwrap_or(Err(FetchError::Stopped))?;
        let entry = fetched.map(|remote| (remote.entry, remote.live_until_ledger_seq));
        self.cache
            .write()
            .unwrap()
            .insert(key.clone(), entry.clone());
        Ok(entry)
    }
}

impl<R: RemoteSource> SnapshotSource for LayeredStore<R> {
    fn get(&self, key: &Rc<LedgerKey>) -> Result<Option<EntryWithLiveUntil>, HostError> {
        let entry = self
            .lookup(key)
            .map_err(|_| HostError::from((ScErrorType::Storage, ScErrorCode::InternalError)))?;
        Ok(entry.map(|(entry, live_until)| (Rc::new(entry), live_until)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::rpc::RemoteEntry;
    use crate::test_utils::{account_data, account_key};
    use crate::xdr::LedgerEntryExt;

    /// Remote holding only account `1`, counting how often it is asked.
    #[derive(Default)]
    struct FakeRemote {
        fail: bool,
        calls: AtomicUsize,
    }

    impl RemoteSource for FakeRemote {
        fn fetch(&self, keys: &[LedgerKey]) -> Vec<FetchResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            keys.iter()
                .map(|key| {
                    if self.fail {
                        return Err(FetchError::Stopped);
                    }
                    Ok((*key == account_key(1)).then(|| RemoteEntry {
                        key: key.clone(),
                        entry: account(1, 7),
                        live_until_ledger_seq: None,
                    }))
                })
                .collect()
        }
    }

    fn account(byte: u8, balance: i64) -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 90,
            data: account_data(byte, balance),
            ext: LedgerEntryExt::V0,
        }
    }

    fn calls(store: &LayeredStore<FakeRemote>) -> usize {
        store.remote.calls.load(Ordering::SeqCst)
    }

    #[test]
    fn local_write_shadows_the_remote() {
        let store = LayeredStore::new(FakeRemote::default());
        store.write_local(account_key(1), Some((account(1, 9), None)));

        let entry = store.lookup(&account_key(1)).unwrap();

        assert_eq!(entry, Some((account(1, 9), None)));
        assert_eq!(calls(&store), 0);
    }

    #[test]
    fn local_write_shadows_the_cache() {
        let store = LayeredStore::new(FakeRemote::default());
        store.lookup(&account_key(1)).unwrap();
        store.write_local(account_key(1), Some((account(1, 9), None)));

        let entry = store.lookup(&account_key(1)).unwrap();

        assert_eq!(entry, Some((account(1, 9), None)));
    }

    #[test]
    fn deletion_hides_the_entry_without_a_remote_call() {
        let store = LayeredStore::new(FakeRemote::default());
        store.write_local(account_key(1), None);

        assert_eq!(store.lookup(&account_key(1)).unwrap(), None);
        assert_eq!(calls(&store), 0);
    }

    #[test]
    fn fetches_an_existing_key_once() {
        let store = LayeredStore::new(FakeRemote::default());

        store.lookup(&account_key(1)).unwrap();
        let entry = store.lookup(&account_key(1)).unwrap();

        assert_eq!(entry, Some((account(1, 7), None)));
        assert_eq!(calls(&store), 1);
    }

    #[test]
    fn fetches_a_missing_key_once() {
        let store = LayeredStore::new(FakeRemote::default());

        store.lookup(&account_key(2)).unwrap();
        let entry = store.lookup(&account_key(2)).unwrap();

        assert_eq!(entry, None);
        assert_eq!(calls(&store), 1);
    }

    #[test]
    fn does_not_cache_a_failed_fetch() {
        let store = LayeredStore::new(FakeRemote {
            fail: true,
            ..Default::default()
        });

        assert!(store.lookup(&account_key(1)).is_err());
        assert!(store.lookup(&account_key(1)).is_err());
        assert_eq!(calls(&store), 2);
    }

    #[test]
    fn serves_the_host_through_snapshot_source() {
        let store = LayeredStore::new(FakeRemote::default());

        let (entry, live_until) = store.get(&Rc::new(account_key(1))).unwrap().unwrap();

        assert_eq!(*entry, account(1, 7));
        assert_eq!(live_until, None);
        assert!(store.get(&Rc::new(account_key(2))).unwrap().is_none());
    }
}
