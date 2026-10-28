#![doc(html_root_url = "https://docs.rs/better-feed/")]
//! This crate provides a parser and common data model over Atom, RSS and JSON Feed content.
//!
//! The parser will automatically detect the type of content (XML vs. JSON) and the feed format (Atom vs. RSS).
//!
//! It populates a unified data model over all feed formats, attempting to find a balance between:
//! * convenience of a single field where semantic equivalence exists e.g. "description" from RSS 2 and "subtitle" from Atom are semantically equivalent and mapped to the same field
//! * the real world where mandatory fields may not be specified and data may not be in the correct form
//!
//! The parser errs on the side of leniency with the outcome that certain fields are represented as `Option<T>` in the model, even though they may be mandatory in one of the specifications.
//!
//! It uses [quick-xml](https://crates.io/crates/quick-xml) - a light-weight, streaming XML parser to minimise memory usage and avoids copying (clone) where possible.
//!
//! # Usage
//!
//! The `parser::parse` method accepts any source that implements the `Read` trait.
//! For example, to process a string:
//!
//! ```rust,no_run
//! use better_feed::parser;
//!
//! let example_rss = r#"<?xml version="1.0" encoding="UTF-8" ?>
//!   <rss version="2.0">
//!     <channel>
//!       <title>RSS Title</title>
//!       <description>This is an example of an RSS feed</description>
//!       <link>http://www.example.com/main.html</link>
//!       <lastBuildDate>Mon, 06 Sep 2010 00:01:00 +0000</lastBuildDate>
//!       <pubDate>Sun, 06 Sep 2009 16:20:00 +0000</pubDate>
//!       <ttl>1800</ttl>
//!
//!       <item>
//!         <title>Example entry</title>
//!         <description>Here is some text containing an interesting description.</description>
//!         <link>http://www.example.com/blog/post/1</link>
//!         <guid isPermaLink="true">7bd204c6-1655-4c27-aeee-53f933c5395f</guid>
//!         <pubDate>Sun, 06 Sep 2009 16:20:00 +0000</pubDate>
//!       </item>
//!
//!     </channel>
//!   </rss>"#;
//!
//! let feed = parser::parse(example_rss.as_bytes()).unwrap();
//!```
//!
//! Or from a file:
//!
//! ```rust,no_run
//! use std::fs::File;
//! use std::io::BufReader;
//! use better_feed::parser;
//!
//! let file = File::open("example.xml").unwrap();
//! let feed = parser::parse(BufReader::new(file)).unwrap();
//! ```
//!
//! ## Parser configuration
//!
//! The default parser configuration provides sensible defaults, such as lenient timestamp parsing. You may change this
//! behaviour and configure other parser behaviour with the [`parser::Builder`], for example to resolve relative links
//! against the URL the feed was fetched from, or to apply stricter resource limits to untrusted input:
//!
//! ```rust
//! use better_feed::parser::{self, ParseLimits};
//!
//! let parser = parser::Builder::new()
//!     .base_uri(Some("https://example.com/feed.xml"))
//!     .limits(ParseLimits::untrusted())
//!     .build();
//! let feed = parser.parse(r#"<rss version="2.0"><channel><link>/</link></channel></rss>"#.as_bytes()).unwrap();
//! assert_eq!(feed.links[0].href, "https://example.com/");
//! ```
//!
//! ## Sanitisation
//!
//! HTML content can be sanitised with [ammonia](https://crates.io/crates/ammonia). This requires the optional
//! `sanitize` cargo feature; without it sanitisation is a no-op (see [`parser::SANITIZE_AVAILABLE`]).
//! When the feature is enabled, sanitisation is **on by default** for HTML content and media text; it can be turned off
//! with [`parser::Builder::sanitize_content`], and extended to captured extensions with
//! [`parser::Builder::sanitize_extensions`]. `text/plain` fields are never modified, including RSS image titles and
//! descriptions (they are alt and title text, not HTML).
//!
//! ```toml
//! better-feed = { version = "3", features = ["sanitize"] }
//! ```
//!
//! ## Extensions
//!
//! Elements (and JSON Feed keys) without a dedicated field in the model are kept as [`model::Extension`] trees on
//! [`model::Feed::extensions`] and [`model::Entry::extensions`], preserving namespaces, prefixes, attributes and text.
//! Capture can be disabled with [`parser::Builder::capture_extensions`] and is bounded by [`parser::ParseLimits`].

#![forbid(unsafe_code)]
// Standard names like MediaRSS and JSON are used throughout this crate
#![allow(clippy::upper_case_acronyms)]
// Nested ifs are more readable when we are using a local variable in a match pattern etc
#![allow(clippy::collapsible_if)]

mod util;
mod xml;

// Compile and run the README examples as doctests
#[doc = include_str!("../README.md")]
#[cfg(doctest)]
pub struct ReadmeDoctests;

pub mod model;
pub mod parser;
pub mod writer;
