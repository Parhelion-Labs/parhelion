# Parhelion

A local Stellar node that lazily forks mainnet.

## Development

```sh
cargo build
cargo test   # first run also installs the git hooks (.cargo-husky/hooks)
cargo run -p parhelion-cli

# Tests against a live mainnet RPC (default https://mainnet.sorobanrpc.com)
PARHELION_MAINNET_RPC_URL=<url> cargo test --workspace -- --ignored
```

Git hooks: `pre-commit` runs `cargo fmt --check` and `cargo clippy -D warnings`;
`pre-push` runs `cargo test`.

## Pinned ledgers

Mainnet RPC only serves current state: `getLedgerEntries` has no ledger parameter, so a run cannot
read mainnet "as of ledger N". `--pin-ledger N` makes runs reproducible another way:

- Every entry fetched under it, absent keys included, is written to `<cache-dir>/N/`, one JSON
  file per key. The ledger header the host simulates with (close time, protocol version,
  network passphrase) is written there too, as `ledger.json`.
- Later runs with the same `N` read those files and never call the remote for them. Keys seen for
  the first time still go remote and are added, so after one complete run the state no longer
  moves.
- `N` is also the ledger the host simulates at.
- Without `--pin-ledger` nothing is written to disk; a cache of unpinned reads would only go stale.

The first pinned run captures whatever mainnet holds at that moment, which is current state, not
the state at ledger `N`. Pick `N` close to the live ledger.

## License

[Apache-2.0](LICENSE)
