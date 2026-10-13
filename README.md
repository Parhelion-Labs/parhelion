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

## License

[Apache-2.0](LICENSE)
