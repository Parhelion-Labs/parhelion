//! `parhelion simulate`: run one contract call against a forked network and show what it cost.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::ValueEnum;
use parhelion_engine::xdr::{AccountId, ContractEventBody, LedgerKey, Limits, ScAddress, WriteXdr};
use parhelion_engine::{invoke_contract, Fork, ForkOptions, Simulation};

use crate::{args, format};

/// An account that need not exist: fine for calls that require no authorization.
const DEFAULT_SOURCE: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

#[derive(clap::Args)]
pub struct Args {
    /// Contract to call (C...).
    #[arg(long)]
    contract: String,
    /// Function to call.
    #[arg(long = "fn")]
    function: String,
    /// Argument, repeatable, in call order.
    #[arg(long = "arg", value_name = "TYPE:VALUE", long_help = args::SYNTAX)]
    args: Vec<String>,
    /// Network to fork.
    #[arg(long, default_value = "mainnet")]
    fork: Network,
    /// RPC of the forked network; overrides the default of --fork.
    #[arg(long)]
    rpc_url: Option<String>,
    /// Freeze the state this and later runs see under ledger N (see the README).
    #[arg(long, value_name = "N")]
    pin_ledger: Option<u32>,
    /// Where pinned ledgers are cached.
    #[arg(long, default_value = ".parhelion/cache")]
    cache_dir: PathBuf,
    /// Source account of the transaction (G...).
    #[arg(long, default_value = DEFAULT_SOURCE)]
    source: String,
    /// Also list every ledger key fetched from the forked network.
    #[arg(long, short)]
    verbose: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Network {
    Mainnet,
    Testnet,
}

impl Network {
    fn rpc_url(self) -> &'static str {
        match self {
            Network::Mainnet => "https://mainnet.sorobanrpc.com",
            Network::Testnet => "https://soroban-testnet.stellar.org",
        }
    }
}

pub async fn run(args: Args) -> Result<ExitCode> {
    let contract: ScAddress = args.contract.parse().context("--contract")?;
    let source: AccountId = args.source.parse().context("--source")?;
    let call_args = args::parse_all(&args.args)?;
    let host_function = invoke_contract(contract, &args.function, call_args)?;
    let rpc_url = args
        .rpc_url
        .unwrap_or_else(|| args.fork.rpc_url().to_owned());

    let fork = Fork::connect(&ForkOptions {
        rpc_url: rpc_url.clone(),
        pin_ledger: args.pin_ledger,
        cache_dir: args.cache_dir,
    })
    .await?;
    println!(
        "ledger {} (protocol {}) via {rpc_url}",
        fork.ledger_info.sequence_number, fork.ledger_info.protocol_version
    );

    let simulation = fork.simulate(host_function, &source)?;
    if args.verbose {
        let source = match args.pin_ledger {
            Some(ledger) => format!("the cache of ledger {ledger} or {rpc_url}"),
            None => rpc_url,
        };
        print_fetched(&simulation.fetched_keys, &source);
    }
    print_outcome(&simulation)
}

fn print_fetched(keys: &[LedgerKey], source: &str) {
    println!("fetched {} ledger keys from {source}:", keys.len());
    for key in keys {
        println!("  {}", format::ledger_key(key));
    }
}

fn print_outcome(simulation: &Simulation) -> Result<ExitCode> {
    match simulation.return_value() {
        Ok(value) => {
            println!("result: {}", format::scval(value));
            println!("result xdr: {}", value.to_xdr_base64(Limits::none())?);
        }
        Err(error) => {
            println!("call failed: {error}");
            for event in &simulation.result.diagnostic_events {
                let ContractEventBody::V0(body) = &event.event.body;
                println!(
                    "  {} {}",
                    body.topics
                        .iter()
                        .map(format::scval)
                        .collect::<Vec<_>>()
                        .join(" "),
                    format::scval(&body.data)
                );
            }
            return Ok(ExitCode::FAILURE);
        }
    }
    if let Some(data) = simulation.transaction_data() {
        let resources = &data.resources;
        println!("min resource fee: {} stroops", data.resource_fee);
        println!(
            "resources: {} instructions, {} bytes read, {} bytes written",
            resources.instructions, resources.disk_read_bytes, resources.write_bytes
        );
        println!(
            "footprint: {} read-only, {} read-write",
            resources.footprint.read_only.len(),
            resources.footprint.read_write.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}
