# better-feed

A lenient feed parser for Rust (Atom, RSS 0.9x/1.0/2.0, JSON Feed) with lossless extension capture, a streaming API,
and Atom / JSON Feed writers.

| Directory | Contents |
|-----------|----------|
| [`feed-rs/`](feed-rs/) | The `better-feed` library crate: see its [README](feed-rs/README.md) and the [docs](https://docs.rs/better-feed/) |
| [`tools/`](tools/) | Development tools (not published): the `better-feed` CLI, `testurls` and `fixture_serde` |
| [`fuzz/`](fuzz/) | cargo-fuzz targets (nightly; excluded from the workspace) |

## Command line

```sh
cargo run -p tools --bin better-feed -- parse https://example.com/feed.xml          # summary
cargo run -p tools --bin better-feed -- parse feed.xml --json --pretty              # parsed model as JSON
cargo run -p tools --bin better-feed -- parse feed.xml --atom                       # convert to Atom
cargo run -p tools --bin better-feed -- parse - --json-feed < feed.xml              # convert stdin to JSON Feed
cat urls.txt | cargo run -p tools --bin better-feed -- check                        # fetch and parse many URLs
```

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md). In short:

```sh
cargo test --workspace && cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo bench -p better-feed
```

## Status of the improvement plan

[docs/PLAN.md](docs/PLAN.md) holds the full analysis and the status of each of the 41 items: 35 are done and 6 are partial, and each one names the commits that address it. The changes since 3.0.1 are summarised in [feed-rs/CHANGELOG.md](feed-rs/CHANGELOG.md).

## License

MIT ([LICENSE-MIT](LICENSE-MIT)).
