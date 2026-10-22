# better-feed

![Crates.io Status](https://img.shields.io/crates/v/better-feed.svg)

Parser for Atom, RSS, and JSON Feed, including iTunes, Dublin Core, and Media RSS. Unmapped elements and JSON properties are kept on `Feed::extensions` and `Entry::extensions`. Supports serde. [Documentation](https://docs.rs/better-feed/).

```toml
[dependencies]
better-feed = "^3"
```

The `sanitize` feature sanitizes parsed HTML. `text/plain` fields are left unchanged.

MIT license ([LICENSE-MIT](LICENSE-MIT)).

Originally created by Hiroki Kumamoto.
