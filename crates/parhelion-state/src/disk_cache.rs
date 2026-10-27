//! On-disk cache of remote entries for a pinned ledger.
//!
//! `getLedgerEntries` has no ledger parameter and only serves current state, so a run cannot read
//! mainnet "as of ledger N". `--pin-ledger N` instead freezes what a run sees: every entry fetched
//! under it, absent keys included, is written to `<dir>/<N>/` and later runs with the same `N`
//! read those files instead of calling the remote. Keys never fetched before still go remote and
//! are added, so after one complete run the state no longer moves. `N` is also the ledger the
//! host simulates at. Without `--pin-ledger` nothing touches the disk; a cache of unpinned reads
//! would only go stale as mainnet advances.
//!
//! One JSON file per key, named by the SHA-256 of the key's XDR.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::fetcher::{FetchError, FetchResult};
use crate::rpc::RemoteEntry;
use crate::store::RemoteSource;
use crate::xdr::{LedgerEntry, LedgerKey, Limits, ReadXdr, WriteXdr};

/// A [`RemoteSource`] that answers from `<dir>/<ledger>/` and asks `inner` only for the rest.
pub struct DiskCache<R> {
    dir: PathBuf,
    inner: R,
    /// Makes temporary file names unique within the process.
    tmp_seq: AtomicU64,
}

impl<R> DiskCache<R> {
    /// Opens the cache of `ledger` under `dir`, creating it if needed.
    pub fn open(dir: impl AsRef<Path>, ledger: u32, inner: R) -> io::Result<Self> {
        let dir = dir.as_ref().join(ledger.to_string());
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            inner,
            tmp_seq: AtomicU64::new(0),
        })
    }

    fn path(&self, key: &LedgerKey) -> Result<PathBuf, FetchError> {
        let xdr = key.to_xdr(Limits::none()).map_err(invalid)?;
        let hash = Sha256::digest(xdr);
        Ok(self.dir.join(format!("{hash:x}.json")))
    }

    /// `None` if `key` is not on disk.
    fn read(&self, key: &LedgerKey) -> Result<Option<Option<RemoteEntry>>, FetchError> {
        let json = match fs::read(self.path(key)?) {
            Ok(json) => json,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(disk(error)),
        };
        let cached: CachedEntry = serde_json::from_slice(&json).map_err(invalid)?;
        let entry = cached
            .entry
            .map(|xdr| LedgerEntry::from_xdr_base64(xdr, Limits::none()))
            .transpose()
            .map_err(invalid)?;
        Ok(Some(entry.map(|entry| RemoteEntry {
            key: key.clone(),
            entry,
            live_until_ledger_seq: cached.live_until_ledger_seq,
        })))
    }

    fn write(&self, key: &LedgerKey, entry: Option<&RemoteEntry>) -> Result<(), FetchError> {
        let cached = CachedEntry {
            key: key.to_xdr_base64(Limits::none()).map_err(invalid)?,
            entry: entry
                .map(|remote| remote.entry.to_xdr_base64(Limits::none()))
                .transpose()
                .map_err(invalid)?,
            live_until_ledger_seq: entry.and_then(|remote| remote.live_until_ledger_seq),
        };
        let path = self.path(key)?;
        // Written whole, then renamed, so a concurrent reader never sees a partial file.
        let tmp = path.with_extension(format!(
            "{}.{}.tmp",
            std::process::id(),
            self.tmp_seq.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&tmp, serde_json::to_vec(&cached).map_err(invalid)?).map_err(disk)?;
        fs::rename(&tmp, &path).map_err(disk)
    }
}

impl<R: RemoteSource> RemoteSource for DiskCache<R> {
    fn fetch(&self, keys: &[LedgerKey]) -> Vec<FetchResult> {
        let mut results = Vec::with_capacity(keys.len());
        let mut misses = Vec::new();
        for (index, key) in keys.iter().enumerate() {
            results.push(match self.read(key) {
                Ok(Some(entry)) => Ok(entry),
                Ok(None) => {
                    misses.push(index);
                    // Overwritten below, unless the inner source answers short.
                    Err(FetchError::Stopped)
                }
                Err(error) => Err(error),
            });
        }

        if misses.is_empty() {
            return results;
        }
        let keys_to_fetch: Vec<LedgerKey> =
            misses.iter().map(|&index| keys[index].clone()).collect();
        for (index, result) in misses.into_iter().zip(self.inner.fetch(&keys_to_fetch)) {
            results[index] =
                result.and_then(|entry| self.write(&keys[index], entry.as_ref()).map(|()| entry));
        }
        results
    }
}

/// On-disk form of one key. `entry` is `None` when the remote does not have the key.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedEntry {
    /// Base64 `LedgerKey`, so a file can be read without knowing its name.
    key: String,
    /// Base64 `LedgerEntry`.
    entry: Option<String>,
    live_until_ledger_seq: Option<u32>,
}

fn disk(error: io::Error) -> FetchError {
    FetchError::Disk(Arc::new(error))
}

fn invalid(error: impl std::error::Error + Send + Sync + 'static) -> FetchError {
    disk(io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;
    use crate::test_utils::{account_data, account_key};
    use crate::xdr::LedgerEntryExt;

    /// Remote holding account `1`, recording the keys of every request.
    #[derive(Default)]
    struct FakeRemote {
        fail: bool,
        requests: Mutex<Vec<Vec<LedgerKey>>>,
    }

    impl RemoteSource for FakeRemote {
        fn fetch(&self, keys: &[LedgerKey]) -> Vec<FetchResult> {
            self.requests.lock().unwrap().push(keys.to_vec());
            keys.iter()
                .map(|key| {
                    if self.fail {
                        Err(FetchError::Stopped)
                    } else {
                        Ok((*key == account_key(1)).then(|| account(1)))
                    }
                })
                .collect()
        }
    }

    fn account(byte: u8) -> RemoteEntry {
        RemoteEntry {
            key: account_key(byte),
            entry: LedgerEntry {
                last_modified_ledger_seq: 5,
                data: account_data(byte, 7),
                ext: LedgerEntryExt::V0,
            },
            live_until_ledger_seq: None,
        }
    }

    fn cache(dir: &Path, ledger: u32) -> DiskCache<FakeRemote> {
        DiskCache::open(dir, ledger, FakeRemote::default()).unwrap()
    }

    fn requests(cache: &DiskCache<FakeRemote>) -> Vec<Vec<LedgerKey>> {
        cache.inner.requests.lock().unwrap().clone()
    }

    /// Fetches `keys`, expecting every result to be `Ok`.
    fn fetch_ok(cache: &DiskCache<FakeRemote>, keys: &[LedgerKey]) -> Vec<Option<RemoteEntry>> {
        cache.fetch(keys).into_iter().map(Result::unwrap).collect()
    }

    #[test]
    fn a_second_run_is_served_from_disk_with_no_remote_call() {
        let dir = tempfile::tempdir().unwrap();
        let keys = [account_key(1), account_key(2)];

        let first = cache(dir.path(), 100);
        let results = fetch_ok(&first, &keys);
        assert_eq!(results, vec![Some(account(1)), None]);
        assert_eq!(requests(&first), vec![keys.to_vec()]);

        let second = cache(dir.path(), 100);
        assert_eq!(fetch_ok(&second, &keys), results);
        assert!(requests(&second).is_empty());
    }

    #[test]
    fn only_misses_go_remote_and_results_keep_the_key_order() {
        let dir = tempfile::tempdir().unwrap();
        cache(dir.path(), 100).fetch(&[account_key(1)]);

        let second = cache(dir.path(), 100);
        let results = fetch_ok(&second, &[account_key(2), account_key(1), account_key(3)]);

        assert_eq!(results, vec![None, Some(account(1)), None]);
        assert_eq!(
            requests(&second),
            vec![vec![account_key(2), account_key(3)]]
        );
    }

    #[test]
    fn every_pinned_ledger_has_its_own_cache() {
        let dir = tempfile::tempdir().unwrap();
        cache(dir.path(), 100).fetch(&[account_key(1)]);

        let other = cache(dir.path(), 101);
        other.fetch(&[account_key(1)]);

        assert_eq!(requests(&other), vec![vec![account_key(1)]]);
    }

    #[test]
    fn a_failed_fetch_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let failing = DiskCache::open(
            dir.path(),
            100,
            FakeRemote {
                fail: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(
            failing.fetch(&[account_key(1)]).as_slice(),
            [Err(FetchError::Stopped)]
        ));

        let retry = cache(dir.path(), 100);
        assert_eq!(fetch_ok(&retry, &[account_key(1)]), vec![Some(account(1))]);
        assert_eq!(requests(&retry), vec![vec![account_key(1)]]);
    }

    #[test]
    fn a_corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path(), 100);
        fs::write(cache.path(&account_key(1)).unwrap(), b"not json").unwrap();

        let results = cache.fetch(&[account_key(1)]);

        assert!(matches!(results.as_slice(), [Err(FetchError::Disk(_))]));
        assert!(requests(&cache).is_empty());
    }

    #[test]
    fn files_are_self_describing_json() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path(), 100);
        cache.fetch(&[account_key(1)]);

        let json: HashMap<String, serde_json::Value> =
            serde_json::from_slice(&fs::read(cache.path(&account_key(1)).unwrap()).unwrap())
                .unwrap();

        assert_eq!(
            json["key"],
            account_key(1).to_xdr_base64(Limits::none()).unwrap()
        );
        assert!(json["entry"].is_string());
        assert!(json["liveUntilLedgerSeq"].is_null());
    }
}
