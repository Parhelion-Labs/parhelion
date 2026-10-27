//! Tests against a live mainnet RPC. Ignored by default; run with
//! `cargo test -p parhelion-state -- --ignored`. Override the endpoint with
//! `PARHELION_MAINNET_RPC_URL`.

use std::rc::Rc;
use std::str::FromStr;

use parhelion_state::disk_cache::DiskCache;
use parhelion_state::fetcher::{FetchError, FetchResult, Fetcher};
use parhelion_state::rpc::RpcClient;
use parhelion_state::store::{LayeredStore, RemoteSource};
use parhelion_state::xdr::{
    ContractDataDurability, ContractExecutable, LedgerEntryData, LedgerKey, LedgerKeyContractData,
    ScAddress, ScVal,
};
use soroban_env_host_curr::storage::SnapshotSource;

const DEFAULT_MAINNET_RPC_URL: &str = "https://mainnet.sorobanrpc.com";

/// Stellar Asset Contract of Circle's USDC on mainnet.
const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";

fn mainnet_url() -> String {
    std::env::var("PARHELION_MAINNET_RPC_URL")
        .unwrap_or_else(|_| DEFAULT_MAINNET_RPC_URL.to_owned())
}

fn mainnet() -> RpcClient {
    RpcClient::new(mainnet_url())
}

fn instance_key(contract: &str) -> LedgerKey {
    persistent_key(contract, ScVal::LedgerKeyContractInstance)
}

fn persistent_key(contract: &str, key: ScVal) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::from_str(contract).unwrap(),
        key,
        durability: ContractDataDurability::Persistent,
    })
}

#[tokio::test]
#[ignore = "needs network access to a mainnet RPC"]
async fn fetches_and_decodes_a_mainnet_contract_instance() {
    let key = instance_key(USDC_SAC);

    let result = mainnet()
        .get_ledger_entries(std::slice::from_ref(&key))
        .await
        .unwrap();

    assert!(result.latest_ledger > 0);
    let [entry] = result.entries.as_slice() else {
        panic!("expected exactly one entry, got {:?}", result.entries);
    };
    assert_eq!(entry.key, key);
    assert!(entry.live_until_ledger_seq.is_some());
    let LedgerEntryData::ContractData(data) = &entry.entry.data else {
        panic!("expected contract data, got {:?}", entry.entry.data);
    };
    let ScVal::ContractInstance(instance) = &data.val else {
        panic!("expected a contract instance, got {:?}", data.val);
    };
    assert_eq!(instance.executable, ContractExecutable::StellarAsset);
}

#[test]
#[ignore = "needs network access to a mainnet RPC"]
fn layered_store_serves_mainnet_entries_to_the_host() {
    let store = LayeredStore::new(Fetcher::spawn(mainnet_url()).unwrap());
    let instance = Rc::new(instance_key(USDC_SAC));
    let absent = Rc::new(persistent_key(USDC_SAC, ScVal::U32(0)));

    let (entry, live_until) = store.get(&instance).unwrap().unwrap();
    let LedgerEntryData::ContractData(data) = &entry.data else {
        panic!("expected contract data, got {:?}", entry.data);
    };
    assert!(matches!(data.val, ScVal::ContractInstance(_)));
    assert!(live_until.is_some());

    // Served from the cache this time.
    assert_eq!(store.get(&instance).unwrap(), Some((entry, live_until)));
    assert_eq!(store.get(&absent).unwrap(), None);
}

/// Remote that must never be asked.
struct NoRemote;

impl RemoteSource for NoRemote {
    fn fetch(&self, keys: &[LedgerKey]) -> Vec<FetchResult> {
        keys.iter().map(|_| Err(FetchError::Stopped)).collect()
    }
}

#[test]
#[ignore = "needs network access to a mainnet RPC"]
fn a_pinned_run_is_served_from_disk_on_the_second_run() {
    let dir = tempfile::tempdir().unwrap();
    let instance = Rc::new(instance_key(USDC_SAC));
    let absent = Rc::new(persistent_key(USDC_SAC, ScVal::U32(0)));

    let first = LayeredStore::new(
        DiskCache::open(dir.path(), 1, Fetcher::spawn(mainnet_url()).unwrap()).unwrap(),
    );
    let entry = first.get(&instance).unwrap();
    assert!(entry.is_some());
    assert_eq!(first.get(&absent).unwrap(), None);

    let second = LayeredStore::new(DiskCache::open(dir.path(), 1, NoRemote).unwrap());
    assert_eq!(second.get(&instance).unwrap(), entry);
    assert_eq!(second.get(&absent).unwrap(), None);
}
