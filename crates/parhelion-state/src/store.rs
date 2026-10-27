//! The layered ledger store that feeds the Soroban host.
//!
//! A read goes through three layers and stops at the first that knows the key:
//! L0 local writes, L1 cache of everything already fetched, L2 the remote.
//!
//! Remote round trips are the main cost, so keys known in advance (a transaction footprint) can
//! be prefetched in one call, and fetching a contract instance also fetches its Wasm code.

use std::collections::HashMap;
use std::rc::Rc;
use std::slice;
use std::sync::{Mutex, RwLock};

use soroban_env_host_curr::storage::{EntryWithLiveUntil, SnapshotSource};
use soroban_env_host_curr::HostError;

use crate::fetcher::{FetchError, FetchResult, Fetcher};
use crate::xdr::{
    ContractExecutable, LedgerEntry, LedgerEntryData, LedgerFootprint, LedgerKey,
    LedgerKeyContractCode, ScErrorCode, ScErrorType, ScVal,
};

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

impl<R: RemoteSource + ?Sized> RemoteSource for Box<R> {
    fn fetch(&self, keys: &[LedgerKey]) -> Vec<FetchResult> {
        (**self).fetch(keys)
    }
}

pub struct LayeredStore<R> {
    /// L0. `None` is a deletion and hides whatever the layers below hold.
    local: RwLock<HashMap<LedgerKey, Option<Entry>>>,
    /// L1. `None` remembers that the remote does not have the key.
    cache: RwLock<HashMap<LedgerKey, Option<Entry>>>,
    /// L2.
    remote: R,
    /// Keys asked of L2 since the last [`take_fetched`](Self::take_fetched), in order.
    fetched: Mutex<Vec<LedgerKey>>,
}

impl<R: RemoteSource> LayeredStore<R> {
    pub fn new(remote: R) -> Self {
        Self {
            local: Default::default(),
            cache: Default::default(),
            remote,
            fetched: Default::default(),
        }
    }

    /// Returns and clears the keys fetched from the remote so far.
    pub fn take_fetched(&self) -> Vec<LedgerKey> {
        std::mem::take(&mut *self.fetched.lock().unwrap())
    }

    /// Writes `key` locally; `None` deletes it. The remote is never consulted for it again.
    pub fn write_local(&self, key: LedgerKey, entry: Option<Entry>) {
        self.local.write().unwrap().insert(key, entry);
    }

    /// Loads every key of `footprint` the store does not know yet, in one remote call.
    pub fn prefetch_footprint(&self, footprint: &LedgerFootprint) -> Result<(), FetchError> {
        let keys: Vec<LedgerKey> = footprint
            .read_only
            .iter()
            .chain(footprint.read_write.iter())
            .cloned()
            .collect();
        self.prefetch(&keys)
    }

    /// Loads every key of `keys` the store does not know yet, in one remote call.
    pub fn prefetch(&self, keys: &[LedgerKey]) -> Result<(), FetchError> {
        self.fetch(&self.unknown(keys))
    }

    fn lookup(&self, key: &LedgerKey) -> Result<Option<Entry>, FetchError> {
        if let Some(entry) = self.known(key) {
            return Ok(entry);
        }
        self.fetch(slice::from_ref(key))?;
        Ok(self.known(key).flatten())
    }

    /// What L0 or L1 says about `key`; `None` if neither has seen it.
    fn known(&self, key: &LedgerKey) -> Option<Option<Entry>> {
        if let Some(entry) = self.local.read().unwrap().get(key) {
            return Some(entry.clone());
        }
        self.cache.read().unwrap().get(key).cloned()
    }

    fn unknown(&self, keys: &[LedgerKey]) -> Vec<LedgerKey> {
        let local = self.local.read().unwrap();
        let cache = self.cache.read().unwrap();
        keys.iter()
            .filter(|key| !local.contains_key(key) && !cache.contains_key(key))
            .cloned()
            .collect()
    }

    /// Fetches `keys` in one remote call into L1. Then fetches, in one more call, the Wasm code
    /// of every contract instance among them, which the host always asks for next.
    fn fetch(&self, keys: &[LedgerKey]) -> Result<(), FetchError> {
        if keys.is_empty() {
            return Ok(());
        }
        // No lock is held while fetching; the fetcher merges concurrent misses on the same key.
        let results = self.remote.fetch(keys);
        self.fetched.lock().unwrap().extend_from_slice(keys);
        let mut code_keys = Vec::new();
        let mut first_error = None;
        {
            let mut cache = self.cache.write().unwrap();
            for (key, result) in keys.iter().zip(results) {
                match result {
                    Ok(fetched) => {
                        let entry =
                            fetched.map(|remote| (remote.entry, remote.live_until_ledger_seq));
                        if let Some((entry, _)) = &entry {
                            code_keys.extend(wasm_code_key(entry));
                        }
                        cache.insert(key.clone(), entry);
                    }
                    Err(error) => {
                        first_error.get_or_insert(error);
                    }
                }
            }
        }
        // Best effort: on failure the code is fetched when the host asks for it.
        let _ = self.fetch(&self.unknown(&code_keys));
        first_error.map_or(Ok(()), Err)
    }
}

/// Key of the Wasm code a contract instance runs, if `entry` is one. Built-in contracts such as
/// the Stellar Asset Contract have no Wasm.
fn wasm_code_key(entry: &LedgerEntry) -> Option<LedgerKey> {
    let LedgerEntryData::ContractData(data) = &entry.data else {
        return None;
    };
    let ScVal::ContractInstance(instance) = &data.val else {
        return None;
    };
    let ContractExecutable::Wasm(hash) = &instance.executable else {
        return None;
    };
    Some(LedgerKey::ContractCode(LedgerKeyContractCode {
        hash: hash.clone(),
    }))
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
    use std::sync::Mutex;

    use super::*;
    use crate::rpc::RemoteEntry;
    use crate::test_utils::{account_data, account_key};
    use crate::xdr::{
        ContractCodeEntry, ContractCodeEntryExt, ContractDataDurability, ContractDataEntry,
        ContractId, ExtensionPoint, Hash, LedgerEntryExt, LedgerKeyContractData, ScAddress,
        ScContractInstance,
    };

    /// Remote holding account `1` plus `entries`, recording the keys of every request.
    #[derive(Default)]
    struct FakeRemote {
        entries: HashMap<LedgerKey, LedgerEntry>,
        fail: bool,
        requests: Mutex<Vec<Vec<LedgerKey>>>,
    }

    impl FakeRemote {
        fn with(entries: impl IntoIterator<Item = (LedgerKey, LedgerEntry)>) -> Self {
            Self {
                entries: entries.into_iter().collect(),
                ..Default::default()
            }
        }
    }

    impl RemoteSource for FakeRemote {
        fn fetch(&self, keys: &[LedgerKey]) -> Vec<FetchResult> {
            self.requests.lock().unwrap().push(keys.to_vec());
            keys.iter()
                .map(|key| {
                    if self.fail {
                        return Err(FetchError::Stopped);
                    }
                    let entry = if *key == account_key(1) {
                        Some(account(1, 7))
                    } else {
                        self.entries.get(key).cloned()
                    };
                    Ok(entry.map(|entry| RemoteEntry {
                        key: key.clone(),
                        entry,
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

    fn instance_key(contract: u8) -> LedgerKey {
        LedgerKey::ContractData(LedgerKeyContractData {
            contract: ScAddress::Contract(ContractId(Hash([contract; 32]))),
            key: ScVal::LedgerKeyContractInstance,
            durability: ContractDataDurability::Persistent,
        })
    }

    fn instance(contract: u8, executable: ContractExecutable) -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 90,
            data: LedgerEntryData::ContractData(ContractDataEntry {
                ext: ExtensionPoint::V0,
                contract: ScAddress::Contract(ContractId(Hash([contract; 32]))),
                key: ScVal::LedgerKeyContractInstance,
                durability: ContractDataDurability::Persistent,
                val: ScVal::ContractInstance(ScContractInstance {
                    executable,
                    storage: None,
                }),
            }),
            ext: LedgerEntryExt::V0,
        }
    }

    fn code_key(hash: u8) -> LedgerKey {
        LedgerKey::ContractCode(LedgerKeyContractCode {
            hash: Hash([hash; 32]),
        })
    }

    fn code(hash: u8) -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 90,
            data: LedgerEntryData::ContractCode(ContractCodeEntry {
                ext: ContractCodeEntryExt::V0,
                hash: Hash([hash; 32]),
                code: vec![hash].try_into().unwrap(),
            }),
            ext: LedgerEntryExt::V0,
        }
    }

    /// Contract `contract` running the Wasm with hash `hash`, with that code.
    fn wasm_contract(contract: u8, hash: u8) -> [(LedgerKey, LedgerEntry); 2] {
        [
            (
                instance_key(contract),
                instance(contract, ContractExecutable::Wasm(Hash([hash; 32]))),
            ),
            (code_key(hash), code(hash)),
        ]
    }

    fn requests(store: &LayeredStore<FakeRemote>) -> Vec<Vec<LedgerKey>> {
        store.remote.requests.lock().unwrap().clone()
    }

    fn calls(store: &LayeredStore<FakeRemote>) -> usize {
        requests(store).len()
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

    #[test]
    fn prefetches_a_footprint_in_one_call() {
        let store = LayeredStore::new(FakeRemote::default());
        let footprint = LedgerFootprint {
            read_only: vec![account_key(1), account_key(2)].try_into().unwrap(),
            read_write: vec![account_key(3)].try_into().unwrap(),
        };

        store.prefetch_footprint(&footprint).unwrap();
        let entries: Vec<_> = (1..=3)
            .map(|byte| store.lookup(&account_key(byte)).unwrap())
            .collect();

        assert_eq!(entries, [Some((account(1, 7), None)), None, None]);
        assert_eq!(calls(&store), 1);
    }

    #[test]
    fn prefetch_skips_keys_already_known() {
        let store = LayeredStore::new(FakeRemote::default());
        store.lookup(&account_key(1)).unwrap();
        store.write_local(account_key(2), None);

        store
            .prefetch(&[account_key(1), account_key(2), account_key(3)])
            .unwrap();

        assert_eq!(
            requests(&store),
            [vec![account_key(1)], vec![account_key(3)]]
        );
    }

    #[test]
    fn fetching_an_instance_also_loads_its_code() {
        let store = LayeredStore::new(FakeRemote::with(wasm_contract(1, 9)));

        store.lookup(&instance_key(1)).unwrap();
        let entry = store.lookup(&code_key(9)).unwrap();

        assert_eq!(entry, Some((code(9), None)));
        assert_eq!(requests(&store), [vec![instance_key(1)], vec![code_key(9)]]);
    }

    #[test]
    fn prefetch_loads_the_code_of_every_instance_in_one_more_call() {
        let remote = FakeRemote::with(wasm_contract(1, 8).into_iter().chain(wasm_contract(2, 9)));
        let store = LayeredStore::new(remote);

        store.prefetch(&[instance_key(1), instance_key(2)]).unwrap();

        assert_eq!(
            requests(&store),
            [
                vec![instance_key(1), instance_key(2)],
                vec![code_key(8), code_key(9)]
            ]
        );
    }

    #[test]
    fn a_built_in_contract_has_no_code_to_load() {
        let remote = FakeRemote::with([(
            instance_key(1),
            instance(1, ContractExecutable::StellarAsset),
        )]);
        let store = LayeredStore::new(remote);

        store.lookup(&instance_key(1)).unwrap();

        assert_eq!(calls(&store), 1);
    }
}
