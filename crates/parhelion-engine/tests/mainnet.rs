//! Tests against a live mainnet RPC. Ignored by default; run with
//! `cargo test -p parhelion-engine -- --ignored`. Override the endpoint with
//! `PARHELION_MAINNET_RPC_URL`.

use std::path::Path;

use parhelion_engine::{Fork, ForkOptions};

const DEFAULT_MAINNET_RPC_URL: &str = "https://mainnet.sorobanrpc.com";

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
