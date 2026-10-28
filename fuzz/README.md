# Fuzzing

Fuzz targets for [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (requires a nightly toolchain):

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run parse                 # default parser
cargo +nightly fuzz run parse_untrusted       # ParseLimits::untrusted() + sanitised extensions
```

The fixtures make a good seed corpus:

```sh
mkdir -p fuzz/corpus/parse && cp feed-rs/fixture/*/*.xml feed-rs/fixture/*/*.json fuzz/corpus/parse/
```

This crate is excluded from the main workspace so `cargo test` on stable does not need nightly.
