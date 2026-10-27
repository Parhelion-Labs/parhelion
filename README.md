# Parhelion

A local Stellar node that lazily forks mainnet.

## `parhelion simulate`

Runs one contract call against a fork of mainnet. Only the ledger entries the call touches are
fetched, and the call is simulated locally with the same host and fee logic as mainnet RPC.

```sh
cargo run -q -p parhelion-cli -- simulate \
  --contract CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75 \
  --fn balance --arg address:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN --verbose
```

```
ledger 65162768 (protocol 29) via https://mainnet.sorobanrpc.com
fetched 1 ledger keys from https://mainnet.sorobanrpc.com:
  contract_data CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75 LedgerKeyContractInstance (persistent)
result: 9223372036854775807
result xdr: AAAACgAAAAAAAAAAf/////////8=
min resource fee: 6269 stroops
resources: 150263 instructions, 0 bytes read, 0 bytes written
footprint: 1 read-only, 0 read-write
```

Arguments are written `<type>:<value>`: `bool`, `u32`, `i32`, `u64`, `i64`, `u128`, `i128`,
`symbol`, `string`, `bytes` (hex), `address` (`G...` or `C...`), or the bare word `void`.
`--fork testnet` or `--rpc-url` pick another network; `--source` sets the transaction's source
account; `--pin-ledger N` makes the run reproducible (below). A failed call prints the host error
and its diagnostic events, and exits with status 1. Soroban and the Stellar Asset Contract only:
classic operations are not simulated.

## Differential suite

Proof that the fork simulates like mainnet: the same invocation runs through Parhelion and
through mainnet RPC's own `simulateTransaction`, and every field below must agree. The suite
runs in CI on every push (job `Differential`) and locally with
`cargo test -p parhelion-differential -- --ignored --nocapture`.

| Contract | Address | Calls |
|---|---|---|
| USDC Stellar Asset Contract | `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75` | `decimals`, `name`, `balance(issuer)` |
| Blend v2.1 lending pool | `CBAKAZUEJBAFA2DCGBRS2WKJT3FNIMJBHTFL76AY7XMAVEINQNNIQWAO` | `get_config`, `get_reserve_list` |
| Soroswap XLM/USDC pair | `CAM7DY53G63XA4AJRS24Z6VFYAFSSF76C3RZ45BE5YU3FQS5255OOABP` | `token_0`, `get_reserves`, `total_supply` |

Fields compared: return value, footprint read-only set, footprint read-write set, resources
(instructions, disk read bytes, write bytes), min resource fee. On a mismatch the harness prints
each differing field with both values.

RPC provider: `https://mainnet.sorobanrpc.com`, public and keyless; it publishes no rate limit.
A run makes about 25 requests and has hit occasional connection timeouts, which the client
retries once. Override with `PARHELION_MAINNET_RPC_URL`.

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
