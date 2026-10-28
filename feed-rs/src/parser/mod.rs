use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::fmt::Debug;
use std::hash::Hasher;
use std::io::{BufRead, BufReader, Read};
use std::ops::ControlFlow;
use std::rc::Rc;

use crate::model;
use crate::parser::util::{IdGenerator, TimestampParser};
use crate::xml;
use crate::xml::NS;
use chrono::{DateTime, Utc};
use siphasher::sip128::{Hasher128, SipHasher};

mod atom;
mod common;
pub(crate) mod extension;
mod itunes;
mod json;
mod mediarss;
mod rss0;
mod rss1;
mod rss2;
pub(crate) mod util;

pub type ParseFeedResult<T> = Result<T, ParseFeedError>;

/// An error returned when parsing a feed from a source fails
#[derive(Debug)]
#[non_exhaustive]
pub enum ParseFeedError {
    ParseError(ParseErrorKind),
    // IO error
    IoError(std::io::Error),
    // Underlying issue with JSON (poorly formatted etc.)
    JsonSerde(serde_json::error::Error),
    // Unsupported version of the JSON feed
    JsonUnsupportedVersion(String),
    // Underlying issue with XML (poorly formatted etc.)
    XmlReader(xml::XmlError),
}

impl From<serde_json::error::Error> for ParseFeedError {
    fn from(err: serde_json::error::Error) -> Self {
        ParseFeedError::JsonSerde(err)
    }
}

impl From<std::io::Error> for ParseFeedError {
    fn from(err: std::io::Error) -> Self {
        ParseFeedError::IoError(err)
    }
}

impl From<xml::XmlError> for ParseFeedError {
    fn from(err: xml::XmlError) -> Self {
        ParseFeedError::XmlReader(err)
    }
}

impl fmt::Display for ParseFeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseFeedError::ParseError(pe) => write!(f, "unable to parse feed: {}", pe),
            ParseFeedError::IoError(ie) => write!(f, "unable to read feed: {}", ie),
            ParseFeedError::JsonSerde(je) => write!(f, "unable to parse JSON: {}", je),
            ParseFeedError::JsonUnsupportedVersion(version) => write!(f, "unsupported version: {}", version),
            ParseFeedError::XmlReader(xe) => write!(f, "unable to parse XML: {}", xe),
        }
    }
}

impl Error for ParseFeedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ParseFeedError::IoError(ie) => Some(ie),
            ParseFeedError::JsonSerde(je) => Some(je),
            ParseFeedError::XmlReader(xe) => Some(xe),
            _ => None,
        }
    }
}

/// Underlying cause of the parse failure
#[derive(Debug)]
#[non_exhaustive]
pub enum ParseErrorKind {
    /// Could not find the expected root element (e.g. "channel" for RSS 2, a JSON node etc.)
    NoFeedRoot,
    /// The content type is unsupported, and we cannot parse the value into a known representation
    UnknownMimeType { position: u64, mime: String },
    /// Required content within the source was not found e.g. the XML child text element for a "content" element
    MissingContent { position: u64, reference: &'static str },
    /// Unknown enum variant
    UnknownEnumVariant { position: u64, reference: String },
    /// Unexpected state
    IllegalState { position: u64, reference: String },
    /// The source is larger than [`ParseLimits::max_input_bytes`]
    InputTooLarge { limit: u64 },
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseErrorKind::NoFeedRoot => f.write_str("no root element"),
            ParseErrorKind::UnknownMimeType { position, mime } => write!(f, "unsupported content type {} at position {}", mime, position),
            ParseErrorKind::MissingContent { position, reference } => write!(f, "missing {} at position {}", reference, position),
            ParseErrorKind::UnknownEnumVariant { position, reference } => write!(f, "unknown enum variant {} at position {}", reference, position),
            ParseErrorKind::IllegalState { position, reference } => write!(f, "illegal parser state {} at position {}", reference, position),
            ParseErrorKind::InputTooLarge { limit } => write!(f, "input exceeds the limit of {} bytes", limit),
        }
    }
}

/// Limits applied while parsing, to bound memory use when parsing untrusted input.
///
/// Start from [`ParseLimits::default()`] (or [`ParseLimits::untrusted()`]) and adjust the fields:
///
/// ```
/// use better_feed::parser::{Builder, ParseLimits};
///
/// let limits = ParseLimits::untrusted().with_max_input_bytes(Some(5 * 1024 * 1024));
/// let parser = Builder::new().limits(limits).build();
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ParseLimits {
    /// Maximum number of bytes read from the source; larger input fails with [`ParseErrorKind::InputTooLarge`].
    /// `None` (the default) means unlimited.
    pub max_input_bytes: Option<u64>,
    /// Maximum nesting depth kept for a captured extension (default 32). Deeper children are skipped.
    pub max_extension_depth: usize,
    /// Maximum number of extension nodes (XML elements or JSON values) kept per feed (default 1,000,000).
    /// Once spent, further unmapped elements are skipped.
    pub max_extension_nodes: usize,
    /// Maximum bytes of text kept per extension node (default 1 MiB). Longer text is truncated at a character boundary.
    pub max_extension_text_bytes: usize,
    /// Maximum number of entries kept (default unlimited). Parsing stops once this many entries have been read, so the
    /// rest of the input is not processed; feed-level elements after that point are not seen.
    pub max_entries: Option<usize>,
}

impl Default for ParseLimits {
    fn default() -> Self {
        ParseLimits {
            max_input_bytes: None,
            max_extension_depth: 32,
            max_extension_nodes: 1_000_000,
            max_extension_text_bytes: 1024 * 1024,
            max_entries: None,
        }
    }
}

impl ParseLimits {
    /// Conservative limits for feeds fetched from third parties: 20 MiB of input, extension depth 16,
    /// 100,000 extension nodes, 64 KiB of text per extension and 10,000 entries.
    pub fn untrusted() -> Self {
        ParseLimits {
            max_input_bytes: Some(20 * 1024 * 1024),
            max_extension_depth: 16,
            max_extension_nodes: 100_000,
            max_extension_text_bytes: 64 * 1024,
            max_entries: Some(10_000),
        }
    }

    /// Sets [`ParseLimits::max_entries`]
    pub fn with_max_entries(mut self, max: Option<usize>) -> Self {
        self.max_entries = max;
        self
    }

    /// Sets [`ParseLimits::max_input_bytes`]
    pub fn with_max_input_bytes(mut self, max: Option<u64>) -> Self {
        self.max_input_bytes = max;
        self
    }

    /// Sets [`ParseLimits::max_extension_depth`]
    pub fn with_max_extension_depth(mut self, max: usize) -> Self {
        self.max_extension_depth = max;
        self
    }

    /// Sets [`ParseLimits::max_extension_nodes`]
    pub fn with_max_extension_nodes(mut self, max: usize) -> Self {
        self.max_extension_nodes = max;
        self
    }

    /// Sets [`ParseLimits::max_extension_text_bytes`]
    pub fn with_max_extension_text_bytes(mut self, max: usize) -> Self {
        self.max_extension_text_bytes = max;
        self
    }
}

/// Parser for various feed formats
///
/// A parser holds only configuration, so it is `Send + Sync` and can be shared (e.g. in an `Arc`) and reused.
pub struct Parser {
    base_uri: Option<String>,
    id_generator: Box<IdGenerator>,
    sanitize_content: bool,
    sanitize_extensions: bool,
    capture_extensions: bool,
    limits: ParseLimits,
    /// Unknown `podcast:person` role or group fails the parse instead of being kept as an extension
    strict: bool,
    timestamp_parser: Box<TimestampParser>,
}

impl Parser {
    /// Parse the input (Atom, a flavour of RSS or JSON Feed) into our model
    ///
    /// # Arguments
    ///
    /// * `input` - A source of content such as a string, file etc.
    ///
    /// NOTE: better-feed uses the encoding attribute in the XML prolog to decode content.
    /// HTTP libraries (such as reqwest) provide a `text()` method which applies the content-encoding header and decodes the source into UTF-8.
    /// This then causes better-feed to fail when it attempts to interpret the UTF-8 stream as a different character set.
    /// Instead, pass the raw, encoded source to better-feed e.g. the `.bytes()` method if using reqwest.
    ///
    /// # Examples
    ///
    /// ```
    /// use better_feed::parser;
    /// let xml = r#"
    /// <feed>
    ///    <title type="text">sample feed</title>
    ///    <updated>2005-07-31T12:29:29Z</updated>
    ///    <id>feed1</id>
    ///    <entry>
    ///        <title>sample entry</title>
    ///        <id>entry1</id>
    ///    </entry>
    /// </feed>
    /// "#;
    /// let feed_from_xml = parser::parse(xml.as_bytes()).unwrap();
    ///
    ///
    /// ```
    pub fn parse<R: Read>(&self, source: R) -> ParseFeedResult<model::Feed> {
        self.parse_inner(source, &mut EntrySink::collect(self.limits.max_entries))
    }

    /// Parses the source, handing each entry to `on_entry` as soon as it has been parsed instead of collecting them.
    ///
    /// For XML, memory stays bounded by the largest single entry rather than the whole feed. JSON Feed is read by serde
    /// into a document first, so a large JSON feed is fully buffered; the callback still receives items one at a time.
    /// Return [`ControlFlow::Break`] to stop early (the rest of the input is then not read). Each entry has already had
    /// its id generated and been sanitised (as configured), exactly as with [`Parser::parse`]; for JSON Feed, entries
    /// without authors inherit the feed authors as usual.
    ///
    /// Returns the feed-level metadata seen while parsing, with `entries` empty. Feed elements that appear after the
    /// point where parsing stopped are not included.
    ///
    /// ```
    /// use std::ops::ControlFlow;
    /// use better_feed::parser;
    ///
    /// let xml = r#"<rss version="2.0"><channel><title>t</title>
    ///   <item><title>first</title></item><item><title>second</title></item><item><title>third</title></item>
    /// </channel></rss>"#;
    ///
    /// let mut titles = Vec::new();
    /// let feed = parser::Builder::new().build().parse_with(xml.as_bytes(), |entry| {
    ///     titles.push(entry.title.unwrap().content);
    ///     if titles.len() == 2 { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
    /// }).unwrap();
    ///
    /// assert_eq!(titles, vec!["first", "second"]);
    /// assert_eq!(feed.title.unwrap().content, "t");
    /// assert!(feed.entries.is_empty());
    /// ```
    pub fn parse_with<R, F>(&self, source: R, mut on_entry: F) -> ParseFeedResult<model::Feed>
    where
        R: Read,
        F: FnMut(model::Entry) -> ControlFlow<()>,
    {
        self.parse_inner(source, &mut EntrySink::streaming(self, &mut on_entry))
    }

    // Applies the input size limit (if any), then parses
    fn parse_inner<R: Read>(&self, source: R, sink: &mut EntrySink) -> ParseFeedResult<model::Feed> {
        match self.limits.max_input_bytes {
            Some(limit) => {
                let exceeded = Rc::new(Cell::new(false));
                let source = LimitedReader {
                    inner: source,
                    remaining: limit,
                    exceeded: exceeded.clone(),
                };
                let result = self.parse_unlimited(source, sink);
                if exceeded.get() {
                    Err(ParseFeedError::ParseError(ParseErrorKind::InputTooLarge { limit }))
                } else {
                    result
                }
            }
            None => self.parse_unlimited(source, sink),
        }
    }

    fn parse_unlimited<R: Read>(&self, source: R, sink: &mut EntrySink) -> ParseFeedResult<model::Feed> {
        // Buffer the reader for performance (e.g. when streaming from a network) and so we can peek to determine the type of content
        let mut input = BufReader::new(source);

        // Determine whether this is XML or JSON and call the appropriate parser
        let result = match sniff_format(&mut input)? {
            Some(b'<') => self.parse_xml(input, sink),

            Some(b'{') => self.parse_json(input, sink),

            _ => Err(ParseFeedError::ParseError(ParseErrorKind::NoFeedRoot)),
        };

        // Post-processing as required
        if let Ok(mut feed) = result {
            // Some feeds do not provide IDs, so we need to generate a consistent identifier
            if feed.id.is_empty() {
                feed.id = (self.id_generator)(&feed.links, &feed.title, self.base_uri.as_deref());
            }

            // Extensions are collected incrementally; release spare capacity since the feed is now complete
            feed.extensions.shrink_to_fit();

            // Sanitise content as required
            if self.sanitize_content {
                sanitise_feed(&mut feed, self.sanitize_extensions);
            }

            // Entries (when collected rather than streamed) get the same per-entry processing as streamed ones
            feed.entries.iter_mut().for_each(|entry| self.finish_entry(entry));

            Ok(feed)
        } else {
            result
        }
    }

    // Handles JSON content
    fn parse_json<R: BufRead>(&self, source: R, sink: &mut EntrySink) -> ParseFeedResult<model::Feed> {
        json::parse(self, source, sink)
    }

    // Post-processing applied to every entry: id generation, releasing spare capacity and sanitisation
    fn finish_entry(&self, entry: &mut model::Entry) {
        if entry.id.is_empty() {
            entry.id = (self.id_generator)(&entry.links, &entry.title, self.base_uri.as_deref());
        }
        entry.extensions.shrink_to_fit();
        if self.sanitize_content {
            sanitise_entry(entry, self.sanitize_extensions);
        }
    }

    // Settings for capturing unmapped elements during one parse
    pub(crate) fn capture_config(&self) -> extension::CaptureConfig {
        extension::CaptureConfig::new(self.capture_extensions, &self.limits)
    }

    // Parses timestamps with the configured parser (internal, or supplied via the builder)
    fn parse_timestamp(&self, text: &str) -> Option<DateTime<Utc>> {
        (self.timestamp_parser)(text)
    }

    // Handles XML content
    fn parse_xml<R: BufRead>(&self, source: R, sink: &mut EntrySink) -> ParseFeedResult<model::Feed> {
        // Set up the source of XML elements from the input
        let element_source = xml::ElementSource::new(source, self.base_uri.as_deref())?.with_capture(self.capture_config());
        // Errors reading the root element (malformed prolog, bad encoding etc) are reported as such
        if let Some(root) = element_source.root()? {
            // Dispatch to the correct parser
            let version = root.attr_value("version");
            match (root.name.as_str(), version.as_deref().map(str::trim)) {
                ("feed", _) => {
                    element_source.set_default_default_namespace(NS::Atom);
                    return atom::parse_feed(self, root, sink);
                }
                ("entry", _) => {
                    element_source.set_default_default_namespace(NS::Atom);
                    return atom::parse_entry(self, root, sink);
                }
                // RSS 0.90 - 0.94 (0.9x is upward compatible with 2.0, but reported as RSS0)
                ("rss", Some(version)) if version.starts_with("0.9") => {
                    element_source.set_default_default_namespace(NS::RSS);
                    return rss0::parse(self, root, sink);
                }
                // RSS 2.0, and leniently any other or missing version (e.g. "2.01", "2", or none at all)
                ("rss", _) => {
                    element_source.set_default_default_namespace(NS::RSS);
                    return rss2::parse(self, root, sink);
                }
                ("RDF", _) => {
                    element_source.set_default_default_namespace(NS::RSS);
                    return rss1::parse(self, root, sink);
                }
                _ => {}
            };
        }

        // Couldn't find a recognised feed within the provided XML stream
        Err(ParseFeedError::ParseError(ParseErrorKind::NoFeedRoot))
    }
}

// Finds the first '<' or '{' in the input without consuming it, so the matching parser sees the whole document.
// A leading UTF-8 BOM and buffers holding only ASCII whitespace are skipped, so they do not hide the root.
fn sniff_format<R: Read>(input: &mut BufReader<R>) -> ParseFeedResult<Option<u8>> {
    let mut at_start = true;
    loop {
        let buffer = input.fill_buf()?;
        if buffer.is_empty() {
            return Ok(None);
        }
        let mut offset = 0;
        if at_start && buffer.starts_with(&[0xEF, 0xBB, 0xBF]) {
            offset = 3;
        }
        at_start = false;
        let window = &buffer[offset..];
        if let Some(b) = window.iter().find(|b| **b == b'<' || **b == b'{') {
            return Ok(Some(*b));
        }
        if !window.iter().all(u8::is_ascii_whitespace) {
            return Ok(None);
        }
        let len = buffer.len();
        input.consume(len);
    }
}

/// Whether HTML sanitisation is compiled in (the `sanitize` cargo feature).
///
/// When `false`, [`Builder::sanitize_content`] and [`Builder::sanitize_extensions`] have no effect.
pub const SANITIZE_AVAILABLE: bool = cfg!(feature = "sanitize");

/// Parses the provided source with the defaults
///
/// Customisation of the parser (e.g. base URI, custom timestamp parsers etc. can be configured through the builder.
pub fn parse<R: Read>(source: R) -> ParseFeedResult<model::Feed> {
    Builder::new().build().parse(source)
}

/// Builder to create instances of `FeedParser`
pub struct Builder {
    base_uri: Option<String>,
    id_generator: Box<IdGenerator>,
    sanitize_content: bool,
    sanitize_extensions: bool,
    capture_extensions: bool,
    limits: ParseLimits,
    strict: bool,
    timestamp_parser: Box<TimestampParser>,
}

impl Builder {
    /// Create a new instance of the builder
    pub fn new() -> Builder {
        Builder::default()
    }

    /// Source of the content, used to resolve relative URLs in XML based feeds
    pub fn base_uri<S: AsRef<str>>(mut self, uri: Option<S>) -> Self {
        self.base_uri = uri.map(|s| s.as_ref().to_string());
        self
    }

    /// Create a new instance of the parser
    pub fn build(self) -> Parser {
        Parser {
            base_uri: self.base_uri,
            id_generator: self.id_generator,
            sanitize_content: self.sanitize_content,
            sanitize_extensions: self.sanitize_extensions,
            capture_extensions: self.capture_extensions,
            limits: self.limits,
            strict: self.strict,
            timestamp_parser: self.timestamp_parser,
        }
    }

    /// Registers an ID generator
    pub fn id_generator<F>(mut self, generator: F) -> Self
    where
        F: Fn(&[model::Link], &Option<model::Text>, Option<&str>) -> String + 'static + Send + Sync,
    {
        self.id_generator = Box::new(generator);
        self
    }

    /// Registers an ID generator compatible with v0.2 of feed-rs
    pub fn id_generator_v0_2(self) -> Self {
        self.id_generator(|links, title, _uri| {
            // If we have a link without relative components, use that
            if let Some(link) = links.iter().find(|l| l.rel.is_none()) {
                // Trim the trailing slash if it exists
                let mut link = model::Link::new(link.href.clone(), None);
                if link.href.ends_with('/') {
                    link.href.pop();
                }

                generate_id_from_link_and_title(&link, title)
            } else {
                util::uuid_gen()
            }
        })
    }

    /// Sanitise HTML content (entry content, summaries, titles, rights and media text) with ammonia (default `true`).
    ///
    /// Has no effect unless the `sanitize` cargo feature is enabled (see [`SANITIZE_AVAILABLE`]).
    pub fn sanitize_content(mut self, flag: bool) -> Self {
        self.sanitize_content = flag;
        self
    }

    /// Also sanitise the text of captured extensions (default `false`).
    ///
    /// Extensions carry arbitrary data whose type the parser does not know, so by default their text is left untouched
    /// even when [`Builder::sanitize_content`] is on. Enable this if extension text may be rendered as HTML.
    /// Like `sanitize_content`, this has no effect unless the `sanitize` feature is enabled.
    pub fn sanitize_extensions(mut self, flag: bool) -> Self {
        self.sanitize_extensions = flag;
        self
    }

    /// Whether unmapped elements and JSON properties are kept on `Feed::extensions` / `Entry::extensions` (default `true`).
    ///
    /// Disabling capture reduces memory use and parse time when extensions are not needed.
    pub fn capture_extensions(mut self, flag: bool) -> Self {
        self.capture_extensions = flag;
        self
    }

    /// Limits applied while parsing (input size, extension depth / count / text size)
    pub fn limits(mut self, limits: ParseLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Reject the feed when a `podcast:person` role or group is not in the Podcasting 2.0 taxonomy (default `false`).
    ///
    /// By default that element is kept as an extension and the rest of the feed is parsed. `true` restores the
    /// earlier behaviour of failing the whole feed on the first unknown value.
    pub fn strict(mut self, flag: bool) -> Self {
        self.strict = flag;
        self
    }

    /// Registers a custom timestamp parser
    pub fn timestamp_parser<F>(mut self, ts_parser: F) -> Self
    where
        F: Fn(&str) -> Option<DateTime<Utc>> + 'static + Send + Sync,
    {
        self.timestamp_parser = Box::new(ts_parser);
        self
    }
}

/// Creates a parser instance with sensible defaults
impl Default for Builder {
    fn default() -> Self {
        Builder {
            base_uri: None,
            id_generator: Box::new(generate_id),
            sanitize_content: true,
            sanitize_extensions: false,
            capture_extensions: true,
            limits: ParseLimits::default(),
            strict: false,
            timestamp_parser: Box::new(util::parse_timestamp_lenient),
        }
    }
}

// Assigns IDs to missing feed + entries as required
// Sanitises the feed-level text fields
fn sanitise_feed(feed: &mut model::Feed, extensions: bool) {
    for text in [feed.description.as_mut(), feed.rights.as_mut(), feed.title.as_mut()].into_iter().flatten() {
        text.sanitize();
    }
    if extensions {
        feed.extensions.iter_mut().for_each(model::Extension::sanitize);
    }
}

// Sanitises the text fields of one entry
fn sanitise_entry(entry: &mut model::Entry, extensions: bool) {
    if let Some(c) = entry.content.as_mut() {
        c.sanitize()
    }
    for text in [entry.rights.as_mut(), entry.summary.as_mut(), entry.title.as_mut()].into_iter().flatten() {
        text.sanitize();
    }
    for media in entry.media.iter_mut() {
        media.sanitize();
    }
    if extensions {
        entry.extensions.iter_mut().for_each(model::Extension::sanitize);
    }
}

// Callback receiving streamed entries
type EntryCallback<'a> = &'a mut dyn FnMut(model::Entry) -> ControlFlow<()>;

/// Destination for parsed entries: either collected on the feed, or handed to a callback as they are parsed
pub(crate) struct EntrySink<'a> {
    streaming: Option<(&'a Parser, EntryCallback<'a>)>,
    remaining: Option<usize>,
}

impl<'a> EntrySink<'a> {
    fn collect(max_entries: Option<usize>) -> Self {
        EntrySink {
            streaming: None,
            remaining: max_entries,
        }
    }

    fn streaming(parser: &'a Parser, callback: EntryCallback<'a>) -> Self {
        EntrySink {
            streaming: Some((parser, callback)),
            remaining: parser.limits.max_entries,
        }
    }

    /// Adds an entry; a `Break` result means the caller wants no more entries, so parsing should stop
    pub(crate) fn push(&mut self, feed: &mut model::Feed, mut entry: model::Entry) -> ControlFlow<()> {
        // Entries beyond ParseLimits::max_entries are dropped and parsing stops
        if let Some(remaining) = self.remaining.as_mut() {
            if *remaining == 0 {
                return ControlFlow::Break(());
            }
            *remaining -= 1;
        }

        let flow = match &mut self.streaming {
            None => {
                feed.entries.push(entry);
                ControlFlow::Continue(())
            }
            Some((parser, callback)) => {
                parser.finish_entry(&mut entry);
                callback(entry)
            }
        };
        if self.remaining == Some(0) { ControlFlow::Break(()) } else { flow }
    }
}

// Reader that stops with an error once more than the allowed number of bytes has been read
struct LimitedReader<R> {
    inner: R,
    remaining: u64,
    exceeded: Rc<Cell<bool>>,
}

impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Read one byte more than allowed, so input of exactly the limit is accepted
        let max = buf.len().min(usize::try_from(self.remaining.saturating_add(1)).unwrap_or(usize::MAX));
        let read = self.inner.read(&mut buf[..max])?;
        if read as u64 > self.remaining {
            self.exceeded.set(true);
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "input exceeds max_input_bytes"));
        }
        self.remaining -= read as u64;
        Ok(read)
    }
}

const LINK_HASH_KEY1: u64 = 0x5d78_4074_2887_2d60;
const LINK_HASH_KEY2: u64 = 0x90ee_ca4c_90a5_e228;

// Creates a unique ID by trying the following in order:
// 1) the first link + optional title
// 2) the uri + title provided
// 3) a UUID
pub fn generate_id(links: &[model::Link], title: &Option<model::Text>, uri: Option<&str>) -> String {
    if let Some(link) = links.first() {
        generate_id_from_link_and_title(link, title)
    } else if let (Some(uri), Some(title)) = (uri, title) {
        generate_id_from_uri_and_title(uri, title)
    } else {
        // Generate a UUID as last resort
        util::uuid_gen()
    }
}

// Generate an ID from the link + title
pub fn generate_id_from_link_and_title(link: &model::Link, title: &Option<model::Text>) -> String {
    let mut hasher = SipHasher::new_with_keys(LINK_HASH_KEY1, LINK_HASH_KEY2);
    hasher.write(link.href.as_bytes());
    if let Some(title) = title {
        hasher.write(title.content.as_bytes());
    }
    let hash = hasher.finish128();
    format!("{:x}{:x}", hash.h1, hash.h2)
}

// Generate an ID from the URI and title
pub fn generate_id_from_uri_and_title(uri: &str, title: &model::Text) -> String {
    let mut hasher = SipHasher::new_with_keys(LINK_HASH_KEY1, LINK_HASH_KEY2);
    hasher.write(uri.as_bytes());
    hasher.write(title.content.as_bytes());
    let hash = hasher.finish128();
    format!("{:x}{:x}", hash.h1, hash.h2)
}

mod podcast;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod extension_tests;

#[cfg(test)]
mod proptests;
