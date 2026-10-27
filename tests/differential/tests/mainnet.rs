//! The differential suite against a live mainnet RPC. Ignored by default; run with
//! `cargo test -p parhelion-differential -- --ignored`. Override the endpoint with
//! `PARHELION_MAINNET_RPC_URL`.

use std::str::FromStr;

use parhelion_differential::{assert_all_agree, compare, Case};
use parhelion_engine::xdr::{AccountId, ScVal};
use parhelion_engine::{Fork, ForkOptions};
use parhelion_state::rpc::RpcClient;

const DEFAULT_MAINNET_RPC_URL: &str = "https://mainnet.sorobanrpc.com";

/// Stellar Asset Contract of Circle's USDC on mainnet.
const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const USDC_ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

/// An account that need not exist: the calls require no authorization.
const SOURCE: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

fn options(cache_dir: &std::path::Path) -> ForkOptions {
    ForkOptions {
        rpc_url: std::env::var("PARHELION_MAINNET_RPC_URL")
            .unwrap_or_else(|_| DEFAULT_MAINNET_RPC_URL.to_owned()),
        pin_ledger: None,
        cache_dir: cache_dir.to_path_buf(),
    }
}

async fn run(cases: Vec<Case>) {
    let dir = tempfile::tempdir().unwrap();
    let options = options(dir.path());
    let fork = Fork::connect(&options).await.unwrap();
    let rpc = RpcClient::new(options.rpc_url);
    let source = AccountId::from_str(SOURCE).unwrap();
    let mut comparisons = Vec::new();
    for case in &cases {
        let comparison = compare(&fork, &rpc, &source, case).await.unwrap();
        println!("{comparison}");
        comparisons.push(comparison);
    }
    assert_all_agree(&comparisons).unwrap();
}

#[tokio::test]
#[ignore = "needs network access to a mainnet RPC"]
async fn stellar_asset_contract_agrees_with_mainnet() {
    run(vec![
        Case {
            name: "USDC decimals",
            contract: USDC_SAC,
            function: "decimals",
            args: vec![],
        },
        Case {
            name: "USDC balance of the issuer",
            contract: USDC_SAC,
            function: "balance",
            args: vec![ScVal::Address(USDC_ISSUER.parse().unwrap())],
        },
    ])
    .await;
}
