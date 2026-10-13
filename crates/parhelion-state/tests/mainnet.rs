//! Tests against a live mainnet RPC. Ignored by default; run with
//! `cargo test -p parhelion-state -- --ignored`. Override the endpoint with
//! `PARHELION_MAINNET_RPC_URL`.

use std::str::FromStr;

use parhelion_state::rpc::RpcClient;
use parhelion_state::xdr::{
    ContractDataDurability, ContractExecutable, LedgerEntryData, LedgerKey, LedgerKeyContractData,
    ScAddress, ScVal,
};

const DEFAULT_MAINNET_RPC_URL: &str = "https://mainnet.sorobanrpc.com";

/// Stellar Asset Contract of Circle's USDC on mainnet.
const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";

fn mainnet() -> RpcClient {
    let url = std::env::var("PARHELION_MAINNET_RPC_URL")
        .unwrap_or_else(|_| DEFAULT_MAINNET_RPC_URL.to_owned());
    RpcClient::new(url)
}

fn instance_key(contract: &str) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::from_str(contract).unwrap(),
        key: ScVal::LedgerKeyContractInstance,
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
