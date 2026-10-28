# Contributing

Thanks for helping! Bug reports with a sample feed (or a link to one) are especially useful.

## Checks

Pull requests run the same checks as CI (`.github/workflows/rust.yml`); please run them locally first:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace                    # default features
cargo test --workspace --all-features     # includes the `sanitize` feature
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
```

The minimum supported Rust version is 1.89 (`rust-version` in `feed-rs/Cargo.toml`); CI tests it. CI also runs
`cargo deny check --all-features` (advisories, licences and crate sources, configured in `deny.toml`).

## Tests and fixtures

- Every bug fix needs a regression test that fails without the fix.
- Sample feeds live in `feed-rs/fixture/<format>/`. Each fixture with a `*.serde.json` snapshot is checked by the
  `serde_regression` test, which compares the parsed model against the snapshot.
- After an intended change to parser output, regenerate the snapshots and review the diff:

  ```sh
  cargo run -p tools --bin fixture_serde
  git diff feed-rs/fixture
  ```

  The tool only rewrites snapshots that already exist, and keeps previously generated (random) ids so unrelated
  snapshots do not churn. To add a snapshot for a new fixture, create an empty `name.serde.json` next to it first.
- Property tests (`feed-rs/src/parser/proptests.rs`) and fuzz targets (`fuzz/`, see its README) check that parsing
  never panics.

## Commits and releases

Commits follow [Conventional Commits](https://www.conventionalcommits.org/) (`fix:`, `feat:`, `perf:`, `docs:` ...).
Breaking changes are marked with `!` and a `BREAKING CHANGE:` footer. Releases are made by
[release-plz](https://release-plz.ieni.dev/) from `master`: it opens a release PR with the version bump and changelog,
runs cargo-semver-checks, and publishes once the PR is merged and the checks pass. Do not bump versions by hand.
