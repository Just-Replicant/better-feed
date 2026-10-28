# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

This release fixes several crashes on untrusted input and contains **breaking changes**; it should be released as
4.0.0 (release-plz will propose the major bump from the `!` commits).

### Breaking

- `Feed`, `Entry`, `FeedType`, `ParseFeedError`, `ParseErrorKind`, `Extension`, `ExtensionAttribute` and the new
  `ParseLimits` are `#[non_exhaustive]`. Build values with `Feed::new`, `Entry::default()`, `Text::new` /
  `Text::html` (now public) and add a wildcard arm when matching errors.
- The extension model is lossless: `Extension` gained `prefix`, `tail` (text after the element, for mixed content)
  and `value` (the raw JSON value for JSON Feed keys); `ExtensionAttribute` gained `prefix` and `namespace`, and its
  `name` is now the local name. Element text is no longer trimmed or cut at the first child.
- `ParseErrorKind::InputTooLarge` added (returned when `ParseLimits::max_input_bytes` is exceeded).
- `Debug` for `Feed` is now derived instead of printing JSON; use the new `Feed::to_json()` /
  `Feed::to_json_pretty()`.
- JSON Feed links now carry `rel` values (`alternate`, `self`, `next`, `hub`, `related`, `enclosure`), and the item
  `image`, `banner_image` and `language` and feed `next_url` and `hubs` are mapped into the model rather than kept as
  extensions.
- An unknown `podcast:person` role or group no longer fails the feed; the element is kept as an extension instead.
- Person names next to an email address are trimmed of surrounding brackets, e.g. `"(Dave Winer)"` is now
  `"Dave Winer"`.
- Minimum supported Rust version is declared as 1.89.

### Fixed

- Deeply nested unknown elements could overflow the stack and abort the process; extension capture is now
  iterative and bounded (default depth 32), and nesting beyond the XML reader's limit is a normal error. (C1)
- Panic on multi-byte characters (NBSP, CJK, emoji) next to an email address in author fields. (C2)
- Panics and integer overflow when parsing durations (`itunes:duration`, Media RSS times); `M:SS` and `S` iTunes
  durations are now understood. (C3, B2)
- A `Parser` reused after a failed parse panicked on its next use. (C4)
- Attribute values were not escaped when XHTML / XML content was re-serialised, allowing attribute injection. (C5)
- The `sanitize` feature tests did not compile and clippy errors in test code went unnoticed. (C6)
- With `sanitize`, Media RSS titles, descriptions and texts were not sanitised; extension text can now be sanitised
  with `Builder::sanitize_extensions(true)`. RSS image titles and descriptions stay plain text. (C7)
- iTunes episode metadata (duration, image, author, episode, season) was dropped unless the item also had
  `itunes:title` or `itunes:summary`. (B1)
- RSS with no `version`, or versions such as `2.01`, ` 2.0`, `0.93` and `0.94`, was rejected. (B3)
- XML errors while reading the root element were reported as "no root element". (B4)
- More than 8 KiB of leading whitespace hid the feed root. A leading UTF-8 BOM is skipped the same way. (B5)
- JSON Feed: numeric item ids failed the whole feed; relative URLs were never resolved (now against the builder's
  base URI or `feed_url`). `expired` is mapped onto `Feed::expired`; `user_comment` stays an extension
  (`Feed::json_user_comment`) because the spec says it is not for display. (B6)
- A `podcast:person` without a role was dropped (the spec default is "host"); "Misc." groups are accepted.
  `Builder::strict(true)` fails the feed on an unknown role or group instead of keeping the element. (B7)
- RSS `<image>` sizes above the RSS 2.0 maxima (144 x 400) were discarded. (B11)
- A second or over-long XML declaration (`<?xml … encoding=… ?>` after the prolog) could panic inside quick-xml
  ("set_encoding() called after prefix buffer was drained"); found by the new fuzz targets.
- `fixture_serde` re-parsed snapshot files and produced snapshots that differed from what the tests compare; the
  `testurls` tool stopped at the first network error. (B9, B10)

### Added

- `ParseLimits` (`Builder::limits`) with `max_input_bytes`, `max_extension_depth`, `max_extension_nodes`,
  `max_extension_text_bytes` and `max_entries`, plus a `ParseLimits::untrusted()` preset. (F3, Q2)
- `Builder::capture_extensions(false)` to skip extension capture entirely. (Q2)
- `Parser::parse_with(source, |entry| ControlFlow)` streams entries to a callback and can stop early. For XML, peak
  memory on a 13 MB / 50,000-item feed drops from about 200 MB to 18 MB. JSON Feed is still read as a whole document. (F6)
- `writer::to_atom`, `writer::to_rss`, `writer::to_json_feed` and `writer::to_json_feed_pretty` serialise a `Feed` as
  Atom 1.0, RSS 2.0 or JSON Feed 1.1, including media and captured extensions. (F7)
- Extension lookups on `Feed` and `Entry` (`extension`, `extensions_named`, `extensions_in`, `extension_text`), the
  `model::ns` namespace constants and helpers on `Extension` (`named`, `is`, `attr`, `attr_ns`, `child`,
  `children_named`, `all_text`). (F1)
- Typed views of common extensions: Syndication (`update_period`, `update_frequency`, `update_interval`), GeoRSS
  (`geo_point`), RSS `cloud` and `rss_source`, Slash `comment_count`, item-level iTunes (`itunes_explicit`,
  `itunes_episode_type`, `itunes_block`), channel iTunes (`itunes_new_feed_url`, `itunes_type`, `itunes_complete`)
  and Podcasting 2.0 (`podcast_guid`, `podcast_locked`, `podcast_funding`, `podcast_value`, `podcast_chapters`,
  `podcast_soundbites`, `podcast_alternate_enclosures`). (F2)
- `parser::SANITIZE_AVAILABLE` tells whether the `sanitize` feature is compiled in. (Q9)
- `Parser` is now `Send + Sync` and can be shared between threads. (Q1)
- A `better-feed` command line in the (unpublished) tools crate: `parse` (summary, JSON, Atom or JSON Feed output,
  streaming) and `check` (URL list). (F4)

### Changed

- Timestamps are parsed strictly first, and the clean-up regexes only run for malformed values. (Q8)
- Extension memory: with capture on, peak memory on an extension-heavy 13 MB / 50,000-item feed is about 215 MB
  (3.0.1: 191 MB) because each extension node now records prefix, tail and raw value; with
  `capture_extensions(false)` it is about 104 MB, the same as upstream feed-rs. Use `parse_with` for large feeds
  (18 MB on the same feed).
- Test fixtures are no longer shipped in the published crate. (D4)

### Development

- Property tests (proptest), cargo-fuzz targets, per-format extension tests and criterion benchmarks. (Q4, Q5, F5)
- GitHub Actions rewritten: pull-request CI on stable, MSRV, beta and nightly with all features, clippy on all
  targets, rustdoc with `-D warnings`, security audit, cargo-deny, semver checks and coverage; release-plz gated on CI
  with `semver_check`; weekly fuzzing. The unused GitLab CI file was removed. (D1, D2, D6)
- Rustfmt and the tools crate moved to edition 2024; the tools crate is `publish = false`. (Q6, D3)
- New READMEs, a root licence file and CONTRIBUTING.md. (D5)

## [3.0.1](https://github.com/Just-Replicant/better-feed/compare/v3.0.0...v3.0.1) - 2026-10-22

First release as `better-feed`, a fork of [feed-rs](https://github.com/feed-rs/feed-rs) 3.0.0.

### Added

- Elements and JSON Feed keys without a field in the model are kept as `Extension` trees on `Feed::extensions` and
  `Entry::extensions` (this added public fields to `Feed` and `Entry`, a breaking change for code using struct
  literals that was not called out at the time).

### Other

- Crate renamed to `better-feed`; release automation with release-plz.
- *(readme)* condense crate readme

## 3.0.0 and earlier

Released as feed-rs; see the
[feed-rs changelog](https://github.com/feed-rs/feed-rs/blob/master/feed-rs/CHANGELOG.md).
