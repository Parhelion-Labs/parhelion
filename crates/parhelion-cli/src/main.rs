mod args;
mod format;
mod simulate;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// A local Stellar node that lazily forks mainnet.
#[derive(Parser)]
#[command(name = "parhelion", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Simulate a contract call against a forked network.
    Simulate(simulate::Args),
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    match Cli::parse().command {
        Command::Simulate(args) => simulate::run(args).await,
    }
}
