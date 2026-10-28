# better-feed

[![Crates.io](https://img.shields.io/crates/v/better-feed.svg)](https://crates.io/crates/better-feed)
[![Documentation](https://docs.rs/better-feed/badge.svg)](https://docs.rs/better-feed/)

A lenient parser for Atom, RSS (0.9x, 1.0, 2.0) and JSON Feed (1.0, 1.1) that maps every format onto one model,
including iTunes, Podcasting 2.0, Dublin Core and Media RSS. Anything without a dedicated field is kept, losslessly,
as an extension tree. It can also write feeds back out as Atom, RSS 2.0 or JSON Feed.

```toml
[dependencies]
better-feed = "3"
# or, to sanitise HTML content with ammonia:
# better-feed = { version = "3", features = ["sanitize"] }
```

Minimum supported Rust version: 1.89.

## Parsing

```rust
use better_feed::parser;

let xml = r#"<rss version="2.0"><channel><title>Example</title>
  <item><title>Hello</title><link>https://example.com/hello</link></item>
</channel></rss>"#;

let feed = parser::parse(xml.as_bytes())?;
assert_eq!(feed.entries[0].title.as_ref().unwrap().content, "Hello");
# Ok::<(), better_feed::parser::ParseFeedError>(())
```

The format (XML vs JSON, then Atom / RSS 0.9x / 1.0 / 2.0) is detected automatically. `parser::Builder` configures
the base URI for relative links, timestamp parsing, id generation, sanitisation and resource limits:

```rust
use better_feed::parser::{Builder, ParseLimits};

let parser = Builder::new()
    .base_uri(Some("https://example.com/feed.xml")) // resolve relative links
    .limits(ParseLimits::untrusted())               // cap input size and extension depth/size
    .build();
```

`Parser` is `Send + Sync` and can be shared between threads.

### Streaming large feeds

`Parser::parse_with` hands each entry to a callback as soon as it is parsed, and you can stop early. For XML, memory
stays bounded by one entry. JSON Feed is read into a document first, so a large JSON feed is still fully buffered:

```rust
use std::ops::ControlFlow;
use better_feed::parser::Builder;

# let xml = r#"<rss version="2.0"><channel><item><title>a</title></item></channel></rss>"#;
let mut titles = Vec::new();
let feed = Builder::new().build().parse_with(xml.as_bytes(), |entry| {
    titles.push(entry.title.map(|t| t.content));
    if titles.len() == 10 { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
})?;
// `feed` holds the channel metadata; its `entries` is empty
# Ok::<(), better_feed::parser::ParseFeedError>(())
```

On a 13 MB, 50,000-item RSS feed this lowers peak memory from about 200 MB to 18 MB.

## Extensions

Elements (and JSON Feed keys) that have no field in the model are kept on `Feed::extensions` and
`Entry::extensions` as `Extension` trees with namespace URI, prefix, attributes, text and children. Lookups and typed
views of common extensions are provided:

```rust
use better_feed::{model::ns, parser};

let xml = r#"<rss version="2.0" xmlns:slash="http://purl.org/rss/1.0/modules/slash/"
                 xmlns:podcast="https://podcastindex.org/namespace/1.0"><channel>
  <podcast:funding url="https://donate.example/">Support us</podcast:funding>
  <item><title>t</title><slash:comments>42</slash:comments></item>
</channel></rss>"#;
let feed = parser::parse(xml.as_bytes())?;

// Generic lookups by namespace and name
let funding = feed.extension(Some(ns::PODCAST), "funding").unwrap();
assert_eq!(funding.attr("url"), Some("https://donate.example/"));

// Typed views (Slash, Syndication, GeoRSS, RSS source/cloud, Podcasting 2.0, item-level iTunes, ...)
assert_eq!(feed.entries[0].comment_count(), Some(42));
assert_eq!(feed.podcast_funding()[0].message.as_deref(), Some("Support us"));
# Ok::<(), better_feed::parser::ParseFeedError>(())
```

Capture is bounded by `ParseLimits` (depth, node count, text size) and can be switched off with
`Builder::capture_extensions(false)`, which also brings memory use back to that of a parser without extension support.

## Writing

```rust
use better_feed::{parser, writer};

# let xml = r#"<rss version="2.0"><channel><title>t</title></channel></rss>"#;
let feed = parser::parse(xml.as_bytes())?;
let atom: String = writer::to_atom(&feed);
let rss: String = writer::to_rss(&feed);
assert!(rss.contains("<rss version=\"2.0\""));
let json_feed: String = writer::to_json_feed(&feed).unwrap();
let model_json: String = feed.to_json().unwrap(); // the parsed model itself
# Ok::<(), better_feed::parser::ParseFeedError>(())
```

## Sanitisation

With the `sanitize` feature, HTML content, summaries, titles, rights and media text are cleaned with
[ammonia](https://crates.io/crates/ammonia) by default (`Builder::sanitize_content(false)` turns it off;
`Builder::sanitize_extensions(true)` extends it to extension text). `text/plain` fields are never modified,
including RSS image titles and descriptions. `parser::SANITIZE_AVAILABLE` tells you whether the feature is compiled in.

## Differences from feed-rs

better-feed is a fork of [feed-rs](https://github.com/feed-rs/feed-rs). The main differences are:

- Unmapped elements and JSON Feed keys are kept as lossless `Extension` trees (namespace, prefix, attributes, text,
  tail, children), with lookups and typed accessors, instead of being dropped.
- Resource limits (`ParseLimits`) and iterative extension capture, so deeply nested input cannot overflow the stack.
- A streaming `parse_with` API (XML is incremental; JSON Feed is buffered) and writers for Atom, RSS 2.0 and JSON Feed.
- `Builder::strict(true)` fails a feed on an unknown `podcast:person` role or group. The default keeps that element
  as an extension.
- More lenient parsing of real-world feeds: RSS without a version, numeric JSON Feed ids, relative JSON Feed URLs,
  unknown `podcast:person` roles, `M:SS` iTunes durations, large RSS image sizes, and more (see the
  [changelog](CHANGELOG.md)).

## License

MIT ([LICENSE-MIT](LICENSE-MIT)). Originally created by Hiroki Kumamoto as feed-rs.
