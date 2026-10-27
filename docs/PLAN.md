# better-feed: analysis and fix and improvement plan

Repo: https://github.com/Just-Replicant/better-feed (cloned 27 Oct 2026, HEAD `f47e137`, last commit 22 Oct 2026 17:57 Tehran time). Analysis was done read-only on the box, with no GitHub auth, nothing pushed, and upstream unchanged.

## 0. Implementation status (branch `fixes-and-improvements`)

All 41 items were implemented locally: 35 Done, 6 Partial, 0 Skipped. There are 41 commits on top of upstream `f47e137`, including this plan, and none were pushed.
Every check passes: build, test (default and `--all-features`), `clippy --workspace --all-targets --all-features -D warnings`, `fmt --check`, `cargo doc -D warnings`, and MSRV 1.89.
There is one extra fix the plan did not list, 7595dbb: fuzzing found a quick-xml panic on a second or over-long XML declaration. It now has a regression test.

| ID | Status | Commit(s) | Notes |
|---|---|---|---|
| C1 | Done | 48b5801 | Iterative, bounded capture (default depth 32); 100k-depth regression test |
| C2 | Done | 4529df0 | NBSP/CJK/emoji tests |
| C3 | Done | b0e6495 | Checked arithmetic, anchored regexes |
| C4 | Done | 4a32a6f | Per-parse MediaRSS context |
| C5 | Done | 286abc1 | Attribute values escaped; snapshots regenerated |
| C6 | Done | 0927a68 | Sanitize tests compile and pass; clippy on all targets |
| C7 | Partial | 0469ea8 | Media title/description/texts sanitised + `sanitize_extensions(bool)`; `Image.description` left as plain text |
| B1 | Done | 66f5a40 | `itunes_only_metadata_is_kept` |
| B2 | Done | b0e6495 | S, M:SS, H:MM:SS |
| B3 | Done | eae757e | `rss_version_is_lenient` |
| B4 | Done | eae757e | `root_xml_errors_are_reported` |
| B5 | Done | eae757e | `large_leading_whitespace_is_skipped` |
| B6 | Partial | 6c78165 | ids, URL resolution, image/banner/language/next_url/hubs mapped; `expired`, `user_comment` stay as extensions |
| B7 | Partial | 46cd967 | Bad role/group kept as extension; no `strict` flag added |
| B8 | Done | 48b5801 | prefix/namespace/tail/raw JSON value on extensions |
| B9 | Done | a325d4f |  |
| B10 | Done | 2337e86 | Per-URL errors, timeout, summary |
| B11 | Done | eb6c07a | `rss_image_dimensions_are_kept` |
| B12 | Done | 801f384 |  |
| Q1 | Done | 4a32a6f | `Parser: Send + Sync` asserted |
| Q2 | Done | 48b5801, d62d9c7 | `capture_extensions(bool)`, `ParseLimits` incl. `max_entries` |
| Q3 | Done | 048146c, 4ac5938 | `#[non_exhaustive]`, public constructors, CHANGELOG rewritten, semver-checks in CI |
| Q4 | Done | 6e043b4 | 6 per-format extension tests |
| Q5 | Done | 4dfb6e7, e1752df | 8 proptests, 2 fuzz targets, weekly fuzz job; fuzzing found a quick-xml panic, fixed in 7595dbb |
| Q6 | Done | 51475c2 |  |
| Q7 | Done | 835ebc0 | Derived Debug + `to_json()`/`to_json_pretty()` |
| Q8 | Done | 32c75e5 | Strict parse first, `Cow` fix-ups |
| Q9 | Done | c165149 | `cargo doc -D warnings` clean |
| D1 | Partial | c0df244 | Modern workflow (stable/1.89/beta/nightly, quality, audit, semver, coverage); uses `cargo audit`, not `cargo deny` |
| D2 | Done | 14b7815 | .gitlab-ci.yml removed |
| D3 | Done | 73bd46d |  |
| D4 | Done | 30572c3 | Fixture-based tests therefore can't run from the crate tarball |
| D5 | Done | 30eb272 | README examples are doctested |
| D6 | Done | 6c6a3d8 |  |
| F1 | Done | a6e4316 |  |
| F2 | Done | a6e4316, 10e7db9, 46cf143 | Typed accessors over extensions (geo, cloud, source, locked, funding, value, chapters, soundbite, alternateEnclosure, …) |
| F3 | Done | 48b5801, d62d9c7 | `ParseLimits::untrusted()` preset |
| F4 | Done | c49806a, e1752df | `better-feed parse|check` in the tools crate (not published) |
| F5 | Done | b2a163a, e1752df | criterion benches; manual CI job |
| F6 | Partial | 29b7579 | Callback API `Parser::parse_with` (not an iterator); peak memory 202 MB → 18 MB |
| F7 | Partial | fd77ac4 | Atom 1.0 + JSON Feed 1.1 writers; no RSS writer, media/extensions not written |

Not done locally: `cargo audit`, because it needs a git clone of the advisory DB. It runs in CI instead.

## 1. What the project is

- **better-feed** is a Rust library (`feed-rs/`, crate `better-feed` v3.0.1, edition 2024, MIT). It is a renamed fork of **feed-rs** 3.0.0 (upstream commit `17ddc9e`, 27 Sep 2026). It parses Atom, RSS 0.9x/1.0/2.0 and JSON Feed into a single `model::Feed`. It also understands iTunes, Podcasting 2.0, Media RSS, Dublin Core, content:encoded and wfw. Serde is supported, and there is an optional `sanitize` feature (ammonia).
- **Fork delta (PR #3, "1 new ability")**: unmapped elements and JSON properties are kept in a new `Feed::extensions` / `Entry::extensions` (`model::Extension { namespace, name, attributes, text, children }`). The rest of the fork is the rename, release-plz automation and a shorter README.
- **Workspace**: `feed-rs` (library, about 7.9k lines of Rust including tests) and `tools` (two binaries: `fixture_serde`, which regenerates the serde snapshots, and `testurls`, which fetches URLs from stdin and tries to parse them).
- **Architecture**: `parser::Parser` sniffs `<` or `{`. XML goes through `xml::ElementSource`, a pull-iterator over quick-xml with a `RefCell` state, namespace mapping and an xml:base stack, and is then dispatched by root element to `atom`, `rss0`, `rss1` or `rss2`. Namespace handlers live in `itunes`, `podcast` and `mediarss`, which is stateful. JSON uses serde structs and is then converted. After parsing, missing IDs are filled in (SipHash of link and title, or a UUID) and content is optionally sanitized.
- **Tests**: 89 unit tests, 1 integration test (`id_generator`), 5 sanitize tests (feature-gated), 4 doctests and an 80-fixture serde snapshot regression.

## 2. What I ran (box: cargo 1.99.1)

| Check | Result |
|---|---|
| `cargo build --all-targets` | OK |
| `cargo test --workspace` (default features) | OK: 89 unit, 1 integration and 4 doc tests pass. The 5 sanitize tests are compiled out. |
| `cargo test --all-features` | **Does not compile**: `feed-rs/tests/sanitize.rs:50` compares `Option<String>` with `&str`. With a local one-line fix (reverted afterwards) all 5 sanitize tests pass. |
| `cargo clippy --workspace --all-targets -- -D warnings` | **Fails, 3 errors**: `parser/rss2/tests.rs:978` (zero-prefixed literal) and `parser/tests.rs:42,49` (needless borrow). Plain `cargo clippy -- -D warnings`, which is what CI runs, passes. |
| `cargo fmt --all -- --check` | OK |
| `cargo doc --no-deps` | OK, with 6 rustdoc warnings (redundant link targets, e.g. `model/mod.rs:1435`) |
| `cargo audit` (RustSec DB, 1,379 advisories) | 0 vulnerabilities across 215 crates (fresh lockfile) |
| Probes (scratch crate `/workspace/bf-probe`; upstream comparison `/workspace/bf-probe-up` against a `17ddc9e` worktree) | Confirmed the bugs marked "repro" below |
| Perf probe (synthetic 50k-item RSS with unknown namespaced elements, 13 MB, release build) | Fork: about 415–530 ms and 191 MB peak RSS. Upstream feed-rs: about 325–480 ms and 105 MB. That is roughly **+81% memory** from extension capture. |

Not checked: real-network behaviour of `tools/testurls` (no URL list is shipped), the GitHub Actions runs themselves (no GitHub access by design), MSRV 1.96.1 (only the box toolchain 1.99.1 was used), Windows CRLF behaviour, and fuzzing beyond the targeted probes.

## 3. Prioritised plan

Priority: P0 = fix now (crash, security, broken CI); P1 = next; P2 = quality; P3 = nice-to-have. Effort: S ≈ under ½ day, M ≈ 1–3 days, L ≈ a week or more.

### Phase 0: Critical fixes (crashes on untrusted input, security, CI that hides failures)

| ID | What | Where | Why | Approach | Effort | Pri |
|---|---|---|---|---|---|---|
| C1 | **Stack overflow (process abort) from deeply nested unknown elements**. This is a fork regression. | `feed-rs/src/parser/extension.rs:12-36` (recursive `capture`, line 27); every `_ => …capture(child)?` arm in atom, rss1, rss2, itunes, podcast and mediarss | Repro: 50,000 nested `<x:a>` inside an RSS item aborts a release build ("fatal runtime error: stack overflow"); 10,000 is enough in debug. Upstream feed-rs returns a clean `TooDeeplyNested(65535)` error on the same input. An abort cannot be caught with `catch_unwind`, so any service parsing third-party feeds can be killed by one feed. | Make `capture` iterative with an explicit stack, or add a depth limit (e.g. 64) plus a node and byte budget. Past the limit, drain the subtree without storing it (or return a parse error). Add a regression test with 100k depth. | S–M | P0 |
| C2 | **Panic on a non-ASCII character next to an email in author fields** | `feed-rs/src/parser/util/mod.rs:155` and `:157` (`&raw[..span.start() - 1]`, `&raw[span.end() + 1..]`) | Repro: `<author>Name\u00A0a@b.com</author>` or `a@b.com\u00A0Name` panics with "byte index is not a char boundary". This affects every format (`Person::parse` is used by RSS author, dc:creator, itunes:author/owner, managingEditor and others). | Slice at `span.start()` / `span.end()`, then trim delimiters and whitespace with `trim_matches`. Add tests for NBSP, emoji and CJK neighbours. | S | P0 |
| C3 | **Panic or overflow in `parse_npt` (itunes:duration, media:content start/end, media:text time)** | `feed-rs/src/parser/util/mod.rs:199-201`, `:220`, `:244` | Repro: `<itunes:duration>99999999999999999999999</itunes:duration>` panics on `ParseIntError` (PosOverflow). `9999999999999999:00:00` panics with "attempt to multiply with overflow" in debug and silently wraps in release. | Use `parse::<u64>().ok()?` and `checked_mul`/`checked_add`. Anchor the regexes (`^…$`). Return `None` on overflow. Add proptest/fuzz cases. | S | P0 |
| C4 | **A parser that is reused after a failed parse panics on its next use** (inherited from upstream) | `feed-rs/src/parser/mediarss.rs:41-47` (`assert!(self.root.borrow().is_none())`); `entry_begin`/`entry_end` pairs in `rss2/mod.rs:212/260` and `atom/mod.rs:215/263` | Repro: one `Parser` parses a malformed RSS item (error returned), then parses a valid feed and panics with "assertion failed" at `mediarss.rs:43`. Any long-lived `Parser` in a server is poisoned by one bad feed. | Quick fix: `entry_begin` replaces the state unconditionally, or use an RAII guard that clears scope on drop. Proper fix: see Q1 (move the state into a per-parse context). | S | P0 |
| C5 | **Serialized XHTML/children content does not escape attribute values** | `feed-rs/src/xml/mod.rs:756-767` (`append_element_start`), used by `children_as_string` and `children_as_xhtml` | Repro: Atom `type="xhtml"` content `<a href="…?x=1&amp;y=&quot;2&quot;" title="&lt;b&gt;">` comes out as `<a href="http://a/?x=1&y="2"" title="<b>">`. That is malformed, and lets a feed inject new attributes (e.g. `onerror`) into HTML handed to consumers who don't enable `sanitize`. | Escape `&`, `<` and `"` in attribute values (`quick_xml::escape::escape`). Regenerate affected snapshots and add a test. | S | P0 |
| C6 | **CI does not test what ships**: feature tests are broken and invisible, and clippy skips test code | `.github/workflows/rust.yml`; `feed-rs/tests/sanitize.rs:50`; `parser/rss2/tests.rs:978`; `parser/tests.rs:42,49` | `cargo test` in CI never enables `sanitize`, so the 5 sanitize tests are compiled out, and they don't even compile with the feature on. `cargo clippy` without `--all-targets` misses 3 errors in tests. | Fix the 3 clippy errors and the sanitize assertion (`feed_author.name.as_deref() == Some(...)`). In CI, run `cargo test --workspace --all-features` and `cargo clippy --workspace --all-targets --all-features -D warnings`. | S | P0 |
| C7 | **Sanitize leaves HTML-bearing fields untouched, including the new extensions** | `feed-rs/src/parser/mod.rs:337-362` (`sanitise`) | Repro with `sanitize` on: `media:description type="html"` and a custom `<x:html>` extension both keep `<script>alert(…)</script>`. Users who turn on `sanitize` will reasonably assume all rendered text is clean. | Also sanitize `MediaObject.title`, `description` and `texts`, and `Image.description`. Document clearly that `extensions` are raw. Optionally add `Builder::sanitize_extensions(bool)`. Extend `tests/sanitize.rs`. | M | P1 |

### Phase 1: Correctness bugs (wrong or lost data)

| ID | What | Where | Why | Approach | Effort | Pri |
|---|---|---|---|---|---|---|
| B1 | **iTunes episode metadata dropped** unless the item also has itunes:title or itunes:summary (inherited) | `feed-rs/src/model/mod.rs:795-797` (`MediaObject::is_not_empty`); `rss2/mod.rs:263-267` | Repro: item with `<enclosure>` and `<itunes:duration>` gives every media object a `duration` of `None`. `is_not_empty` ignores duration, thumbnails (itunes:image), credits (itunes:author), episode, season, people and community. All 7 podcast fixtures happen to include itunes:title or summary, so tests never catch it. | Include those fields in `is_not_empty`, or always merge iTunes into the enclosure object. Add a minimal podcast fixture. | S | P1 |
| B2 | **`itunes:duration` "MM:SS" parsed wrongly** | `feed-rs/src/parser/util/mod.rs:185-233`, `itunes.rs:90-92` | The NPT H:MM:SS regex needs 3 parts, and the unanchored seconds regex then matches the first number, so "12:34" would become 12 s once B1 is fixed. "MM:SS" is common in podcast feeds. | Add a dedicated iTunes duration parser (`S`, `M:SS`, `H:MM:SS`, with optional fraction) using checked arithmetic. | S | P1 |
| B3 | **Valid-in-practice RSS rejected** | `feed-rs/src/parser/mod.rs:208-215` | Repro: `<rss>` with no version, `version="2.01"`, `"0.93"`/`"0.94"` and `" 2.0"` all return `NoFeedRoot`. This goes against the crate's stated leniency. | Trim the version; treat `0.9x` as RSS0 and anything else (including missing) as RSS2. | S | P1 |
| B4 | **Real XML errors reported as "no root element"** | `feed-rs/src/parser/mod.rs:196` (`if let Ok(Some(root))`) | Errors from the root element read are swallowed and replaced by `NoFeedRoot`, which hides the cause (e.g. bad encoding or a malformed prolog). | Propagate with `?`, and return `NoFeedRoot` only for `Ok(None)` or an unknown root. | S | P1 |
| B5 | **Format sniffing only looks at the first BufReader fill (8 KiB)** | `feed-rs/src/parser/mod.rs:153-157` | Repro: 10,000 leading spaces before `<rss>` gives `NoFeedRoot`. Long comments or whitespace before the root are possible in generated feeds. | Loop `fill_buf`/`consume` while the bytes are whitespace or BOM, then peek. | S | P2 |
| B6 | **JSON Feed: numeric `id` fails the whole feed; relative URLs never resolved; spec fields not mapped** | `feed-rs/src/parser/json/mod.rs:194-211` (`id: Option<String>`), `:38-41`, `:85`, `:124-126` (`Link::new(uri, None)`); missing `next_url`, `expired`, `hubs`, `user_comment`, item `image`, `banner_image`, `language` | Repro: `"id":123` gives "invalid type: integer, expected a string". With `base_uri("https://ex.com/feed.json")`, the item `url` "/a" stays "/a", and item `image` lands in `extensions`. | Accept string-or-number ids (custom deserializer), resolve against `base_uri` and `feed_url`, and map the spec fields onto the model (image → `media`/`logo`, `next_url` → link rel="next"). | M | P1 |
| B7 | **One invalid enum value aborts the whole feed** (e.g. an unknown `podcast:person` role) | `feed-rs/src/parser/podcast.rs` (`handle_person`); asserted by `parser/tests.rs:93-110` (`test_broken`) | A single bad attribute in one channel element makes the entire feed unreadable. That goes against the lenient design and is common in real Podcasting 2.0 feeds. | Skip the element, or keep it in `extensions`. Optionally add a `strict` builder flag that keeps today's behaviour. This changes behaviour, so give it a minor-version note. | S | P2 |
| B8 | **Extension model is lossy and inconsistent** (fork feature) | `feed-rs/src/parser/extension.rs:24` (text only before first child, trimmed), `:46-52` (scalar arrays joined with ", "; nulls dropped); `xml/mod.rs:709-711` vs `:724` (element prefix stripped, attribute names keep the raw `prefix:` qname without a namespace URI) | `["a, b"]` and `["a","b"]` serialize identically. Mixed content and tail text are lost. Every JSON extension claims namespace `https://jsonfeed.org`. Consumers can't reliably round-trip or match attributes by namespace. | For JSON, store the raw `serde_json::Value` (e.g. `Extension::value`). For XML, keep `prefix`, plus a namespace URI on attributes and ordered `text`/`children` nodes (or a raw-XML option). Document the format and freeze it before 4.0. | M | P1 |
| B9 | `tools/fixture_serde` filter never matches, and its output is inconsistent with the test | `tools/src/bin/fixture_serde.rs:31` (`Path::ends_with(".serde.json")` compares path components, so it is always false), `:16` (no CRLF stripping, unlike `parser/tests.rs:44`) | It re-parses the `.serde.json` files (silently fails today) and on Windows regenerates snapshots that the test then rejects. | Use `path.to_string_lossy().ends_with(".serde.json")`, share the same loader as the test (strip `\r`), and use `sanitize_content(false)` like the test. | S | P2 |
| B10 | `tools/testurls` stops at the first network error and dumps whole bodies | `tools/src/bin/testurls.rs:12` (`?`), `:16` (`{:?}` of raw bytes) | It is useless for batch-checking a URL list. | Handle per-URL errors, print a truncated body and status, and add a summary count and a timeout. | S | P3 |
| B11 | RSS `<image>` width/height silently dropped above spec maxima | `feed-rs/src/parser/rss2/mod.rs:145-159` (`<=144`, `<=400`) | Many real feeds use larger logos, so their size is lost for no benefit. | Keep any positive value (optionally flag that it exceeds the spec). | S | P3 |
| B12 | Duplicate assignment | `feed-rs/src/parser/mediarss.rs:85` and `:87` (`thumbnails` cloned twice) | Wasted clone and a sign of copy-paste. | Remove line 87. | S | P3 |

### Phase 2: Quality, performance, API, tests

| ID | What | Where | Why | Approach | Effort | Pri |
|---|---|---|---|---|---|---|
| Q1 | **`Parser` is not `Sync`** | `feed-rs/src/parser/mod.rs:110-118` holds `mediarss::MediaRssState` (`RefCell`, `mediarss.rs:16-18`) | Verified by compile check: `Parser` cannot be shared in an `Arc` across threads, which is the natural use in web servers. The same state causes C4. | Move the MediaRSS scope into a per-call context passed through the handlers (or create it inside `handle_item`/`handle_entry`). `Parser` then becomes immutable config, `Send + Sync`. Add a `static_assertions` check. | M | P1 |
| Q2 | **Extension capture is always on, unbounded and costly** | `parser/extension.rs`; Builder in `parser/mod.rs:237-321` | Measured +81% peak memory (105 → 191 MB) and about +25% time on an extension-heavy 50k-item feed. There is no way to opt out and no size limits (also see C1). | Add `Builder::capture_extensions(bool)` (default on for compatibility) and `Builder::limits(ParseLimits { max_depth, max_extension_nodes, max_text_bytes, max_entries })`. Add criterion benches. | M | P1 |
| Q3 | **Semver hygiene for the public model** | `feed-rs/src/model/mod.rs` (`Feed`, `Entry`, `Extension`, `ParseFeedError`, `ParseErrorKind`, none `#[non_exhaustive]`); `feed-rs/CHANGELOG.md` | The extensions feature added public fields to exhaustive structs (a breaking change for struct-literal users), yet the changelog for 3.0.1 only says "condense crate readme", and upstream history was dropped from the changelog. | Add `#[non_exhaustive]` in the next major. Run `cargo-semver-checks` in CI. Rewrite the CHANGELOG to cover the fork's actual changes and link to upstream feed-rs history. | S | P1 |
| Q4 | **Thin tests for the fork's main feature** | `feed-rs/src/util/test.rs:9-14` (`copy_extensions`, used at 27 call sites); only `rss2/tests.rs:984` and `json/tests.rs:150` target extensions | Hand-written tests copy actual extensions into expected values, so they never assert extension content. Atom, RSS1, iTunes, Podcast and MediaRSS capture paths are covered only indirectly by snapshots. | Add focused tests per format (namespace, attrs, nesting, media:group scoping), limit tests (C1, Q2) and round-trip serde tests. | M | P1 |
| Q5 | **No fuzzing or property tests** (one historical fuzz vector only) | `parser/tests.rs:20-35`; `lib.rs:75` TODO about coverage | C1–C3 are exactly the class of bugs fuzzing finds in minutes. | Add a `cargo-fuzz` target (`parser::parse` on arbitrary bytes), proptest for `parse_npt`, `parse_person_name_email` and timestamps, a nightly CI fuzz job and `cargo llvm-cov` coverage. | M | P1 |
| Q6 | Config and style drift | `.rustfmt.toml` (`edition = "2018"`) vs crate edition 2024; `tools/Cargo.toml` edition 2021; `lib.rs:83-85` legacy `#[macro_use] extern crate serde; extern crate core;` | Inconsistent formatting edition and outdated idioms. | Align editions, use `use serde::{Deserialize, Serialize}`, and drop `extern crate`. | S | P3 |
| Q7 | `Debug for Feed` goes through serde_json with `unwrap` | `feed-rs/src/model/mod.rs:139-143` | It is surprising (Debug prints JSON) and panics if serialization ever fails. | Derive `Debug`. If JSON output is wanted, expose `Feed::to_json()`. | S | P3 |
| Q8 | Timestamp leniency does repeated regex allocations | `feed-rs/src/parser/util/mod.rs:254-284` | Each date runs up to 10 regex replacements with a new `String` each time. That is minor, but dates appear on every entry. | Try strict `parse_from_rfc3339`/`rfc2822` first and apply fixes only on failure; use `Cow` with `replace`. Benchmark first. | S | P3 |
| Q9 | Doc warnings and stale doc text | `cargo doc` (6 warnings, e.g. `model/mod.rs:1435`); `lib.rs:59-72` says "to enable content sanitisation" with `.sanitize_content(true)`, but it already defaults to `true` (`parser/mod.rs:317`) and does nothing without the `sanitize` feature | This misleads users about the security behaviour. | Fix the links. Document the default and the feature requirement, and consider `#[cfg]`-gating or warning when the flag is set without the feature. | S | P2 |

### Phase 3: CI, release, docs, DX

| ID | What | Where | Why | Approach | Effort | Pri |
|---|---|---|---|---|---|---|
| D1 | **Modernise GitHub CI** | `.github/workflows/rust.yml` | It uses archived `actions-rs/*` and `actions/checkout@v2` (deprecated Node runtimes), runs on `push` only (not on `pull_request`), has no cache, no docs check, no audit, and pins MSRV 1.96.1 without `rust-version` in Cargo.toml. | Use `dtolnay/rust-toolchain` and `Swatinem/rust-cache`. Trigger on `push` and `pull_request`. Jobs: test (stable, MSRV, beta, with `--all-features`), clippy `--all-targets --all-features`, fmt, `RUSTDOCFLAGS=-D warnings cargo doc`, `cargo audit`/`cargo deny`, and `cargo-semver-checks`. Set `rust-version` in `feed-rs/Cargo.toml`. | S–M | P1 |
| D2 | Leftover GitLab CI disables TLS verification | `.gitlab-ci.yml:4-5` (`GIT_SSL_NO_VERIFY: "true"`) | It is dead config inherited from upstream, and it is a security smell if anyone enables GitLab mirroring. | Delete it, or remove the flag and match the GitHub jobs. | S | P2 |
| D3 | `tools` crate marked publishable | `tools/Cargo.toml:6` (`publish = true`, commit `32a9fa6`) vs `release-plz.toml` (`release = false`) | It has a generic name, is not meant for crates.io, and the two configs contradict each other. | Set `publish = false` (or rename it to `better-feed-cli` if you want to publish it, see F4). | S | P2 |
| D4 | Published crate ships test fixtures | `feed-rs/Cargo.toml` `include = [..., "fixture/*"]` | `cargo package --list` shows 179 files, 146 of them fixtures, which bloats downloads for every dependent. | Drop `fixture/*` from `include` (unit tests that need fixtures run in the repo only), or move snapshot tests to `tests/` and exclude them. | S | P3 |
| D5 | Docs gaps after the rename | `README.md` (4 lines); `feed-rs/README.md` (usage, examples and sanitize notes cut from upstream's version, which is 76 lines longer); no root LICENSE | New users have no quick-start, no extension examples, and no explanation of the fork's relationship to feed-rs. | Restore the usage section. Add an extensions example (iterate and match by namespace). Add a "Differences from feed-rs" section, a root LICENSE-MIT that keeps upstream copyright, and a CONTRIBUTING file with how to regenerate snapshots. | S | P2 |
| D6 | Release process sanity | `.github/workflows/release-plz.yml`; `release-plz.toml` | Releases cut automatically from `master` with no required CI gate in the workflow. | Make release-plz depend on the CI workflow (or require status checks via branch protection), and enable `semver_check` in release-plz. | S | P2 |

### Phase 4: Features and improvements

| ID | What | Why | Approach | Effort | Pri |
|---|---|---|---|---|---|
| F1 | Extension accessors | Finding an extension today means walking `Vec<Extension>` by hand | Add `Feed::extension(ns, name)`, `Entry::extensions_in(ns)` and `Extension::attr(name)`, with namespace-constant helpers (`ns::SLASH`, `ns::GEORSS`, …). | S–M | P2 |
| F2 | Map more common namespaces natively | Data that users usually want ends up in untyped extensions | Map `slash:comments`, `sy:updatePeriod`/`updateFrequency`, `georss:point`, RSS `<source>`/`<cloud>`, and Podcasting 2.0 `funding`, `chapters`, `soundbite`, `locked`, `guid`, `value`, `alternateEnclosure`, as well as `itunes:explicit`, `type`, `episodeType` and `block` at item level. | M–L | P2 |
| F3 | Parse limits and an untrusted-input mode | Server use needs bounded memory and time | `ParseLimits` (see Q2) plus `Builder::max_input_bytes`; document recommended settings. | M | P1 |
| F4 | Proper CLI (`better-feed-cli`) | Replaces the ad-hoc `testurls` and is useful for debugging feeds | `better-feed parse <file|url> [--json] [--no-extensions]`, with a URL list mode, per-URL status and timing. | S–M | P3 |
| F5 | Benchmarks and perf tracking | Guards against regressions like Q2 | Criterion benches on large fixtures (plain RSS, podcast, Atom xhtml, JSON); run in CI on demand. | S | P2 |
| F6 | Optional streaming/entry iterator API | Large feeds (tens of MB) are parsed fully into memory | `Parser::entries(reader)` returns a header plus an iterator of `Entry`; this reuses the pull model already in `xml::ElementSource`. | L | P3 |
| F7 | Feed writer (serialize back to Atom/RSS/JSON Feed) | It would round-trip the extensions model and is useful for proxies and aggregators | Separate module or crate; start with JSON Feed and Atom. | L | P3 |

## 4. Suggested sequencing

1. **Week 1 (hotfix 3.0.2)**: C1, C2, C3, C4, C5 and C6, each with a regression test and the probe inputs above added as test cases.
2. **Week 2 (3.1.0)**: B1, B2, B3, B4 and B6; Q1; Q2 and F3 (limits and opt-out); C7; D1, D2, D3 and D6.
3. **Weeks 3–4**: B8 (extension model v2, behind a feature or in 4.0), Q3, Q4, Q5, D5 and F1, then plan **4.0.0** with `#[non_exhaustive]` and the new extension model.
4. **Later**: F2, F4, F5, F6, F7 and the P3 cleanups.

## 5. Repro inputs (from `/workspace/bf-probe/src/main.rs`)

- C1: `<rss version="2.0"><channel><item><x:a xmlns:x="urn:x">` followed by 50,000 nested `<x:a>`, which aborts.
- C2: `<author>Name\u00A0a@b.com</author>`, which panics.
- C3: `<itunes:duration>99999999999999999999999</itunes:duration>`, which panics.
- C4: the same `Parser` parses `<rss version="2.0"><channel><item><title>a</title><enclosure url="x"></item></channel></rss>` (error), then any valid RSS, which panics.
- C5: Atom xhtml `<a href="http://a/?x=1&amp;y=&quot;2&quot;" title="&lt;b&gt;">`, which gives the output `href="http://a/?x=1&y="2""`.
- B1: `<item><itunes:duration>12:34</itunes:duration><enclosure url="http://a/b.mp3" type="audio/mpeg"/></item>`, which gives `duration: None`.
- B3: `<rss><channel>…`, which gives `NoFeedRoot`.
- B6: JSON item `"id":123`, which gives a parse error.
