//! Tests against a live mainnet RPC. Ignored by default; run with
//! `cargo test -p parhelion-engine -- --ignored`. Override the endpoint with
//! `PARHELION_MAINNET_RPC_URL`.

use std::path::Path;
use std::str::FromStr;

use parhelion_engine::xdr::{
    AccountId, ContractDataDurability, LedgerKey, LedgerKeyContractData, ScAddress, ScVal,
};
use parhelion_engine::{invoke_contract, Fork, ForkOptions};

const DEFAULT_MAINNET_RPC_URL: &str = "https://mainnet.sorobanrpc.com";

/// Stellar Asset Contract of Circle's USDC on mainnet.
const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";

/// An account that does not need to exist: the calls below require no authorization.
const SOURCE: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

/// SHA-256 of "Public Global Stellar Network ; September 2015".
const MAINNET_NETWORK_ID: [u8; 32] = [
    0x7a, 0xc3, 0x39, 0x97, 0x54, 0x4e, 0x31, 0x75, 0xd2, 0x66, 0xbd, 0x02, 0x24, 0x39, 0xb2, 0x2c,
    0xdb, 0x16, 0x50, 0x8c, 0x01, 0x16, 0x3f, 0x26, 0xe5, 0xcb, 0x2a, 0x3e, 0x10, 0x45, 0xa9, 0x79,
];

fn mainnet_url() -> String {
    std::env::var("PARHELION_MAINNET_RPC_URL")
        .unwrap_or_else(|_| DEFAULT_MAINNET_RPC_URL.to_owned())
}

fn options(pin_ledger: Option<u32>, cache_dir: &Path) -> ForkOptions {
    ForkOptions {
        rpc_url: mainnet_url(),
        pin_ledger,
        cache_dir: cache_dir.to_path_buf(),
    }
}

#[tokio::test]
#[ignore = "needs network access to a mainnet RPC"]
async fn network_config_is_built_from_live_mainnet_entries() {
    let dir = tempfile::tempdir().unwrap();

    let fork = Fork::connect(&options(None, dir.path())).await.unwrap();

    let info = &fork.ledger_info;
    assert!(info.protocol_version >= 29, "{}", info.protocol_version);
    assert!(info.sequence_number > 0);
    assert!(info.timestamp > 0);
    assert_eq!(info.network_id, MAINNET_NETWORK_ID);
    assert!(info.min_persistent_entry_ttl > 0);
    assert!(info.max_entry_ttl > info.min_persistent_entry_ttl);
    let config = &fork.network_config;
    assert!(config.tx_max_instructions > 0);
    assert!(config.fee_configuration.fee_per_instruction_increment > 0);
    assert!(config.rent_fee_configuration.fee_per_rent_1kb > 0);
}

#[tokio::test]
#[ignore = "needs network access to a mainnet RPC"]
async fn a_pinned_fork_needs_no_network_on_the_second_run() {
    let dir = tempfile::tempdir().unwrap();
    let first = Fork::connect(&options(Some(1), dir.path())).await.unwrap();
    assert_eq!(first.ledger_info.sequence_number, 1);

    let offline = ForkOptions {
        rpc_url: "http://127.0.0.1:9".into(),
        ..options(Some(1), dir.path())
    };
    let second = Fork::connect(&offline).await.unwrap();

    assert_eq!(second.ledger_info, first.ledger_info);
    assert_eq!(second.network_config, first.network_config);
}

#[tokio::test]
#[ignore = "needs network access to a mainnet RPC"]
async fn simulates_a_mainnet_contract_call() {
    let dir = tempfile::tempdir().unwrap();
    let fork = Fork::connect(&options(None, dir.path())).await.unwrap();
    let usdc = ScAddress::from_str(USDC_SAC).unwrap();
    let source = AccountId::from_str(SOURCE).unwrap();
    let instance = LedgerKey::ContractData(LedgerKeyContractData {
        contract: usdc.clone(),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    });
    let decimals = || invoke_contract(usdc.clone(), "decimals", vec![]).unwrap();

    let simulation = fork.simulate(decimals(), &source).unwrap();

    assert_eq!(simulation.return_value().unwrap(), &ScVal::U32(7));
    let footprint = simulation.footprint().unwrap();
    assert!(footprint.read_only.contains(&instance), "{footprint:?}");
    assert!(footprint.read_write.is_empty(), "{footprint:?}");
    assert!(simulation.resources().unwrap().instructions > 0);
    assert!(simulation.min_resource_fee().unwrap() > 0);
    assert!(simulation.fetched_keys.contains(&instance));

    // Everything is cached now.
    let again = fork.simulate(decimals(), &source).unwrap();
    assert_eq!(again.return_value().unwrap(), &ScVal::U32(7));
    assert!(again.fetched_keys.is_empty(), "{:?}", again.fetched_keys);
}
