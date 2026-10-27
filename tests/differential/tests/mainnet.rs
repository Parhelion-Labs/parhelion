//! The differential suite against a live mainnet RPC. Ignored by default; run with
//! `cargo test -p parhelion-differential -- --ignored --nocapture`. Override the endpoint with
//! `PARHELION_MAINNET_RPC_URL`.
//!
//! Every call is read-only and independent of the ledger close time, so the two sides agree as
//! long as mainnet state does not change between them. If it does, the case is run once more on
//! a fresh fork.

use std::str::FromStr;

use parhelion_differential::{assert_all_agree, compare, Case, Comparison};
use parhelion_engine::xdr::{AccountId, ScVal};
use parhelion_engine::{Fork, ForkOptions};
use parhelion_state::rpc::RpcClient;

const DEFAULT_MAINNET_RPC_URL: &str = "https://mainnet.sorobanrpc.com";

/// Stellar Asset Contract of Circle's USDC.
const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const USDC_ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
/// Blend v2.1 lending pool, the one in the v2.1 backstop's reward zone.
const BLEND_POOL: &str = "CBAKAZUEJBAFA2DCGBRS2WKJT3FNIMJBHTFL76AY7XMAVEINQNNIQWAO";
/// Soroswap XLM/USDC pair, as returned by the Soroswap factory's `get_pair`; `token_0` is the
/// native XLM Stellar Asset Contract.
const SOROSWAP_XLM_USDC: &str = "CAM7DY53G63XA4AJRS24Z6VFYAFSSF76C3RZ45BE5YU3FQS5255OOABP";

/// An account that need not exist: the calls require no authorization.
const SOURCE: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

fn rpc_url() -> String {
    std::env::var("PARHELION_MAINNET_RPC_URL")
        .unwrap_or_else(|_| DEFAULT_MAINNET_RPC_URL.to_owned())
}

fn address(contract: &str) -> ScVal {
    ScVal::Address(contract.parse().unwrap())
}

async fn fresh_fork(cache_dir: &std::path::Path) -> Fork {
    Fork::connect(&ForkOptions {
        rpc_url: rpc_url(),
        pin_ledger: None,
        cache_dir: cache_dir.to_path_buf(),
    })
    .await
    .unwrap()
}

async fn run(cases: Vec<Case>) {
    let dir = tempfile::tempdir().unwrap();
    let rpc = RpcClient::new(rpc_url());
    let source = AccountId::from_str(SOURCE).unwrap();
    let mut fork = fresh_fork(dir.path()).await;
    let mut comparisons = Vec::new();
    for case in &cases {
        let mut comparison: Comparison = compare(&fork, &rpc, &source, case).await.unwrap();
        if !comparison.agrees() {
            // Mainnet may have moved between the two sides; look again at current state.
            println!("retrying after:\n{comparison}");
            fork = fresh_fork(dir.path()).await;
            comparison = compare(&fork, &rpc, &source, case).await.unwrap();
        }
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
            name: "USDC name",
            contract: USDC_SAC,
            function: "name",
            args: vec![],
        },
        Case {
            name: "USDC balance of the issuer",
            contract: USDC_SAC,
            function: "balance",
            args: vec![address(USDC_ISSUER)],
        },
    ])
    .await;
}

#[tokio::test]
#[ignore = "needs network access to a mainnet RPC"]
async fn blend_lending_pool_agrees_with_mainnet() {
    run(vec![
        Case {
            name: "Blend pool config",
            contract: BLEND_POOL,
            function: "get_config",
            args: vec![],
        },
        Case {
            name: "Blend pool reserve list",
            contract: BLEND_POOL,
            function: "get_reserve_list",
            args: vec![],
        },
    ])
    .await;
}

#[tokio::test]
#[ignore = "needs network access to a mainnet RPC"]
async fn soroswap_pair_agrees_with_mainnet() {
    run(vec![
        Case {
            name: "Soroswap XLM/USDC token_0",
            contract: SOROSWAP_XLM_USDC,
            function: "token_0",
            args: vec![],
        },
        Case {
            name: "Soroswap XLM/USDC reserves",
            contract: SOROSWAP_XLM_USDC,
            function: "get_reserves",
            args: vec![],
        },
        Case {
            name: "Soroswap XLM/USDC total supply",
            contract: SOROSWAP_XLM_USDC,
            function: "total_supply",
            args: vec![],
        },
    ])
    .await;
}
