# Parhelion

A local Stellar node that lazily forks mainnet.

## Development

```sh
cargo build
cargo test   # first run also installs the git hooks (.cargo-husky/hooks)
cargo run -p parhelion-cli
```

Git hooks: `pre-commit` runs `cargo fmt --check` and `cargo clippy -D warnings`;
`pre-push` runs `cargo test`.

## License

[Apache-2.0](LICENSE)
