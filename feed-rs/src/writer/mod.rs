//! Serialises a [`Feed`] back out as [Atom 1.0](https://www.rfc-editor.org/rfc/rfc4287),
//! [RSS 2.0](https://www.rssboard.org/rss-specification) or [JSON Feed 1.1](https://www.jsonfeed.org/version/1.1/).
//!
//! This is useful for converting between formats (e.g. normalising RSS to Atom) or re-publishing a filtered feed.
//! The output covers the core model (ids, titles, links, dates, people, categories, summaries, content, rights,
//! language, generator, icon and logo), media objects (RSS enclosures and `media:*` elements, Atom enclosure links,
//! JSON Feed attachments and images) and captured extensions.
//!
//! ```
//! use better_feed::{parser, writer};
//!
//! let rss = r#"<rss version="2.0"><channel><title>Example</title><link>https://example.com/</link>
//!   <item><title>Hello</title><link>https://example.com/hello</link><guid>hello</guid></item>
//! </channel></rss>"#;
//! let feed = parser::parse(rss.as_bytes()).unwrap();
//!
//! let atom = writer::to_atom(&feed);
//! let rss = writer::to_rss(&feed);
//! let again = parser::parse(atom.as_bytes()).unwrap();
//! assert_eq!(again.entries[0].title.as_ref().unwrap().content, "Hello");
//! assert!(rss.contains("<title>Hello</title>"));
//!
//! let json = writer::to_json_feed(&feed).unwrap();
//! assert!(json.contains(r#""title":"Hello""#));
//! ```

use std::collections::HashSet;

use chrono::{DateTime, SecondsFormat, Utc};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::model::{Category, Content, Entry, Extension, Feed, Link, Person, Text};

#[cfg(test)]
mod tests;

const ATOM_NS: &str = "http://www.w3.org/2005/Atom";
const CONTENT_NS: &str = "http://purl.org/rss/1.0/modules/content/";
const MEDIA_NS: &str = "http://search.yahoo.com/mrss/";
const JSON_FEED_VERSION: &str = "https://jsonfeed.org/version/1.1";

// ---------------------------------------------------------------------------------------------------------------------
// Atom
// ---------------------------------------------------------------------------------------------------------------------

/// Serialises the feed as an Atom 1.0 document.
///
/// Atom requires `updated` on the feed and each entry; when missing, the entry's `published` date is used, then the
/// newest entry date for the feed, and finally the Unix epoch so the output is deterministic.
pub fn to_atom(feed: &Feed) -> String {
    let mut out = AtomWriter {
        writer: Writer::new_with_indent(Vec::new(), b' ', 2),
    };
    out.feed(feed);
    String::from_utf8(out.writer.into_inner()).expect("the writer only emits UTF-8")
}

struct AtomWriter {
    writer: Writer<Vec<u8>>,
}

fn rfc3339(t: &DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

// Writing to a Vec cannot fail, so the io::Result from quick-xml is discarded with this helper
fn ok<T>(r: std::io::Result<T>) {
    r.expect("writing to a Vec<u8> cannot fail");
}

impl AtomWriter {
    fn event(&mut self, event: Event) {
        ok(self.writer.write_event(event));
    }

    fn start(&mut self, start: BytesStart) {
        self.event(Event::Start(start));
    }

    fn end(&mut self, name: &str) {
        self.event(Event::End(BytesEnd::new(name)));
    }

    fn empty(&mut self, start: BytesStart) {
        self.event(Event::Empty(start));
    }

    fn text_element(&mut self, name: &str, text: &str) {
        self.start(BytesStart::new(name));
        self.event(Event::Text(BytesText::new(text)));
        self.end(name);
    }

    fn opt_text_element(&mut self, name: &str, text: Option<&str>) {
        if let Some(text) = text {
            self.text_element(name, text);
        }
    }

    fn feed(&mut self, feed: &Feed) {
        self.event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)));

        let mut root = BytesStart::new("feed");
        root.push_attribute(("xmlns", ATOM_NS));
        push_xmlns(&mut root, xmlns_decls(feed));
        if let Some(lang) = &feed.language {
            root.push_attribute(("xml:lang", lang.as_str()));
        }
        self.start(root);

        self.text_element("id", &feed.id);
        self.text("title", feed.title.as_ref().or(Some(&Text::new(String::new()))));
        self.text("subtitle", feed.description.as_ref());
        let updated = feed
            .updated
            .or(feed.published)
            .or_else(|| feed.entries.iter().filter_map(|e| e.updated.or(e.published)).max())
            .unwrap_or(DateTime::UNIX_EPOCH);
        self.text_element("updated", &rfc3339(&updated));
        self.people("author", &feed.authors);
        self.people("contributor", &feed.contributors);
        self.links(&feed.links);
        self.categories(&feed.categories);
        if let Some(generator) = &feed.generator {
            let mut start = BytesStart::new("generator");
            if let Some(uri) = &generator.uri {
                start.push_attribute(("uri", uri.as_str()));
            }
            if let Some(version) = &generator.version {
                start.push_attribute(("version", version.as_str()));
            }
            self.start(start);
            self.event(Event::Text(BytesText::new(&generator.content)));
            self.end("generator");
        }
        self.opt_text_element("icon", feed.icon.as_ref().map(|i| i.uri.as_str()));
        self.opt_text_element("logo", feed.logo.as_ref().map(|i| i.uri.as_str()));
        self.text("rights", feed.rights.as_ref());
        write_model_extensions(&mut self.writer, &feed.extensions, atom_core_extension);

        for entry in &feed.entries {
            self.entry(entry, updated);
        }

        self.end("feed");
    }

    fn entry(&mut self, entry: &Entry, feed_updated: DateTime<Utc>) {
        let mut start = BytesStart::new("entry");
        if let Some(lang) = &entry.language {
            start.push_attribute(("xml:lang", lang.as_str()));
        }
        if let Some(base) = &entry.base {
            start.push_attribute(("xml:base", base.as_str()));
        }
        self.start(start);

        self.text_element("id", &entry.id);
        self.text("title", entry.title.as_ref().or(Some(&Text::new(String::new()))));
        let updated = entry.updated.or(entry.published).unwrap_or(feed_updated);
        self.text_element("updated", &rfc3339(&updated));
        self.opt_text_element("published", entry.published.as_ref().map(rfc3339).as_deref());
        self.people("author", &entry.authors);
        self.people("contributor", &entry.contributors);
        self.links(&entry.links);
        self.categories(&entry.categories);
        self.text("summary", entry.summary.as_ref());
        if let Some(content) = &entry.content {
            self.content(content);
        }
        self.text("rights", entry.rights.as_ref());
        self.media_enclosures(entry);
        write_model_extensions(&mut self.writer, &entry.extensions, atom_core_extension);

        self.end("entry");
    }

    // Writes an Atom text construct (text or html)
    fn text(&mut self, name: &str, text: Option<&Text>) {
        let Some(text) = text else { return };
        let mut start = BytesStart::new(name);
        start.push_attribute(("type", atom_text_type(text.content_type.essence().to_string().as_str())));
        if let Some(src) = &text.src {
            start.push_attribute(("src", src.as_str()));
        }
        self.start(start);
        self.event(Event::Text(BytesText::new(&text.content)));
        self.end(name);
    }

    fn content(&mut self, content: &Content) {
        let mime = content.content_type.essence().to_string();
        let mut start = BytesStart::new("content");
        // Inline content is preferred; out-of-line content (src) is only written when there is no body
        if let (None, Some(src)) = (&content.body, &content.src) {
            start.push_attribute(("src", src.href.as_str()));
            start.push_attribute(("type", mime.as_str()));
            self.empty(start);
            return;
        }
        start.push_attribute(("type", atom_text_type(&mime)));
        self.start(start);
        self.event(Event::Text(BytesText::new(content.body.as_deref().unwrap_or(""))));
        self.end("content");
    }

    fn people(&mut self, name: &str, people: &[Person]) {
        for person in people {
            self.start(BytesStart::new(name));
            self.text_element("name", person.name.as_deref().unwrap_or(""));
            self.opt_text_element("uri", person.uri.as_deref());
            self.opt_text_element("email", person.email.as_deref());
            self.end(name);
        }
    }

    fn links(&mut self, links: &[Link]) {
        for link in links {
            let mut start = BytesStart::new("link");
            start.push_attribute(("href", link.href.as_str()));
            if let Some(rel) = &link.rel {
                start.push_attribute(("rel", rel.as_str()));
            }
            if let Some(media_type) = &link.media_type {
                start.push_attribute(("type", media_type.as_str()));
            }
            if let Some(lang) = &link.href_lang {
                start.push_attribute(("hreflang", lang.as_str()));
            }
            if let Some(title) = &link.title {
                start.push_attribute(("title", title.as_str()));
            }
            if let Some(length) = link.length {
                start.push_attribute(("length", length.to_string().as_str()));
            }
            self.empty(start);
        }
    }

    fn media_enclosures(&mut self, entry: &Entry) {
        let mut seen: HashSet<&str> = entry.links.iter().map(|link| link.href.as_str()).collect();
        for media in &entry.media {
            for content in &media.content {
                let Some(url) = content.url.as_ref() else { continue };
                if !seen.insert(url.as_str()) {
                    continue;
                }
                let mut start = BytesStart::new("link");
                start.push_attribute(("rel", "enclosure"));
                start.push_attribute(("href", url.as_str()));
                if let Some(media_type) = &content.content_type {
                    start.push_attribute(("type", media_type.as_str()));
                }
                if let Some(length) = content.size {
                    start.push_attribute(("length", length.to_string().as_str()));
                }
                self.empty(start);
            }
        }
    }

    fn categories(&mut self, categories: &[Category]) {
        for category in categories {
            let mut start = BytesStart::new("category");
            start.push_attribute(("term", category.term.as_str()));
            if let Some(scheme) = &category.scheme {
                start.push_attribute(("scheme", scheme.as_str()));
            }
            if let Some(label) = &category.label {
                start.push_attribute(("label", label.as_str()));
            }
            self.empty(start);
        }
    }
}

// Atom text constructs are "text", "html" or "xhtml"; XHTML is written escaped as HTML
fn atom_text_type(mime: &str) -> &'static str {
    match mime {
        "text/html" | "application/xhtml+xml" => "html",
        _ => "text",
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// RSS 2.0
// ---------------------------------------------------------------------------------------------------------------------

/// Serialises the feed as an RSS 2.0 document.
///
/// The channel `link` is the `alternate` link (or the first link that is not an enclosure). Item `description` is the
/// summary, or the content body when there is no summary; when both exist the body is also written as
/// `content:encoded`. Media objects are written as `enclosure` elements and Media RSS elements. Captured extensions
/// are written as elements, with their namespace prefixes declared on the root.
pub fn to_rss(feed: &Feed) -> String {
    let mut out = RssWriter {
        writer: Writer::new_with_indent(Vec::new(), b' ', 2),
    };
    out.feed(feed);
    String::from_utf8(out.writer.into_inner()).expect("the writer only emits UTF-8")
}

struct RssWriter {
    writer: Writer<Vec<u8>>,
}

impl RssWriter {
    fn event(&mut self, event: Event) {
        ok(self.writer.write_event(event));
    }

    fn start(&mut self, start: BytesStart) {
        self.event(Event::Start(start));
    }

    fn end(&mut self, name: &str) {
        self.event(Event::End(BytesEnd::new(name)));
    }

    fn empty(&mut self, start: BytesStart) {
        self.event(Event::Empty(start));
    }

    fn text_element(&mut self, name: &str, text: &str) {
        self.start(BytesStart::new(name));
        self.event(Event::Text(BytesText::new(text)));
        self.end(name);
    }

    fn opt_text_element(&mut self, name: &str, text: Option<&str>) {
        if let Some(text) = text {
            self.text_element(name, text);
        }
    }

    fn feed(&mut self, feed: &Feed) {
        self.event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)));

        let mut root = BytesStart::new("rss");
        root.push_attribute(("version", "2.0"));
        let mut extra = Vec::new();
        if feed_needs_content_module(feed) {
            extra.push(("content", CONTENT_NS));
        }
        if feed_needs_media(feed) {
            extra.push(("media", MEDIA_NS));
        }
        push_xmlns(&mut root, xmlns_decls_plus(feed, &extra));
        self.start(root);
        self.start(BytesStart::new("channel"));

        self.text_element("title", feed.title.as_ref().map(|t| t.content.as_str()).unwrap_or(""));
        self.opt_text_element("link", primary_link(&feed.links));
        self.text_element("description", feed.description.as_ref().map(|t| t.content.as_str()).unwrap_or(""));
        self.opt_text_element("language", feed.language.as_deref());
        self.opt_text_element("copyright", feed.rights.as_ref().map(|t| t.content.as_str()));
        self.opt_text_element("pubDate", feed.published.as_ref().map(rfc2822).as_deref());
        self.opt_text_element("lastBuildDate", feed.updated.as_ref().map(rfc2822).as_deref());
        self.opt_text_element("ttl", feed.ttl.map(|ttl| ttl.to_string()).as_deref());
        self.opt_text_element("generator", feed.generator.as_ref().map(|g| g.content.as_str()));
        for category in &feed.categories {
            self.category(category);
        }
        if let Some(image) = &feed.logo {
            self.image(feed, image);
        }
        write_model_extensions(&mut self.writer, &feed.extensions, rss_core_extension);

        for entry in &feed.entries {
            self.item(entry);
        }

        self.end("channel");
        self.end("rss");
    }

    fn image(&mut self, feed: &Feed, image: &crate::model::Image) {
        self.start(BytesStart::new("image"));
        self.text_element("url", &image.uri);
        self.text_element(
            "title",
            image.title.as_deref().or(feed.title.as_ref().map(|t| t.content.as_str())).unwrap_or(""),
        );
        self.text_element("link", image.link.as_ref().map(|l| l.href.as_str()).or(primary_link(&feed.links)).unwrap_or(""));
        self.opt_text_element("width", image.width.map(|w| w.to_string()).as_deref());
        self.opt_text_element("height", image.height.map(|h| h.to_string()).as_deref());
        self.opt_text_element("description", image.description.as_deref());
        self.end("image");
    }

    fn item(&mut self, entry: &Entry) {
        self.start(BytesStart::new("item"));
        self.opt_text_element("title", entry.title.as_ref().map(|t| t.content.as_str()));
        self.opt_text_element("link", primary_link(&entry.links));
        if let Some(author) = entry.authors.iter().find_map(rss_author) {
            self.text_element("author", &author);
        }
        for category in &entry.categories {
            self.category(category);
        }
        let summary = entry.summary.as_ref().map(|t| t.content.as_str());
        let body = entry.content.as_ref().and_then(|c| c.body.as_deref());
        match (summary, body) {
            (Some(summary), Some(body)) => {
                self.text_element("description", summary);
                self.text_element("content:encoded", body);
            }
            (Some(summary), None) => self.text_element("description", summary),
            (None, Some(body)) => self.text_element("description", body),
            (None, None) => {}
        }
        if !entry.id.is_empty() {
            let mut guid = BytesStart::new("guid");
            guid.push_attribute(("isPermaLink", if url::Url::parse(&entry.id).is_ok() { "true" } else { "false" }));
            self.start(guid);
            self.event(Event::Text(BytesText::new(&entry.id)));
            self.end("guid");
        }
        self.opt_text_element("pubDate", entry.published.or(entry.updated).as_ref().map(rfc2822).as_deref());
        self.enclosures(entry);
        self.media(entry);
        write_model_extensions(&mut self.writer, &entry.extensions, rss_core_extension);
        self.end("item");
    }

    fn category(&mut self, category: &Category) {
        let mut start = BytesStart::new("category");
        if let Some(scheme) = &category.scheme {
            start.push_attribute(("domain", scheme.as_str()));
        }
        self.start(start);
        self.event(Event::Text(BytesText::new(&category.term)));
        self.end("category");
    }

    fn enclosures(&mut self, entry: &Entry) {
        let mut seen = HashSet::new();
        for link in entry.links.iter().filter(|link| link.rel.as_deref() == Some("enclosure")) {
            if !seen.insert(link.href.as_str()) {
                continue;
            }
            let mut start = BytesStart::new("enclosure");
            start.push_attribute(("url", link.href.as_str()));
            if let Some(media_type) = &link.media_type {
                start.push_attribute(("type", media_type.as_str()));
            }
            if let Some(length) = link.length {
                start.push_attribute(("length", length.to_string().as_str()));
            }
            self.empty(start);
        }
        for media in &entry.media {
            for content in &media.content {
                let Some(url) = content.url.as_ref() else { continue };
                if !seen.insert(url.as_str()) {
                    continue;
                }
                let mut start = BytesStart::new("enclosure");
                start.push_attribute(("url", url.as_str()));
                start.push_attribute(("type", content.content_type.as_ref().map(|m| m.as_str()).unwrap_or("application/octet-stream")));
                if let Some(length) = content.size {
                    start.push_attribute(("length", length.to_string().as_str()));
                }
                self.empty(start);
            }
        }
    }

    fn media(&mut self, entry: &Entry) {
        for media in &entry.media {
            self.opt_text_element("media:title", media.title.as_ref().map(|t| t.content.as_str()));
            self.opt_text_element("media:description", media.description.as_ref().map(|t| t.content.as_str()));
            for content in &media.content {
                let Some(url) = content.url.as_ref() else { continue };
                let mut start = BytesStart::new("media:content");
                start.push_attribute(("url", url.as_str()));
                if let Some(media_type) = &content.content_type {
                    start.push_attribute(("type", media_type.as_str()));
                }
                if let Some(size) = content.size {
                    start.push_attribute(("fileSize", size.to_string().as_str()));
                }
                self.empty(start);
            }
            for thumbnail in &media.thumbnails {
                let mut start = BytesStart::new("media:thumbnail");
                start.push_attribute(("url", thumbnail.image.uri.as_str()));
                self.empty(start);
            }
        }
    }
}

fn rfc2822(t: &DateTime<Utc>) -> String {
    t.to_rfc2822()
}

fn rss_author(person: &Person) -> Option<String> {
    match (&person.email, &person.name) {
        (Some(email), Some(name)) if !name.is_empty() => Some(format!("{email} ({name})")),
        (Some(email), _) => Some(email.clone()),
        (None, Some(name)) if !name.is_empty() => Some(name.clone()),
        _ => None,
    }
}

fn primary_link(links: &[Link]) -> Option<&str> {
    link_with_rel(links, &[Some("alternate"), None]).or_else(|| {
        links
            .iter()
            .find(|link| link.rel.as_deref() != Some("enclosure"))
            .map(|link| link.href.as_str())
    })
}

fn feed_needs_content_module(feed: &Feed) -> bool {
    feed.entries
        .iter()
        .any(|entry| entry.summary.is_some() && entry.content.as_ref().and_then(|c| c.body.as_deref()).is_some())
}

fn feed_needs_media(feed: &Feed) -> bool {
    feed.entries.iter().any(|entry| {
        entry
            .media
            .iter()
            .any(|media| media.title.is_some() || media.description.is_some() || !media.content.is_empty() || !media.thumbnails.is_empty())
    })
}

// ---------------------------------------------------------------------------------------------------------------------
// JSON Feed
// ---------------------------------------------------------------------------------------------------------------------

/// Serialises the feed as a JSON Feed 1.1 document.
///
/// Links are mapped by `rel`: `alternate` (or no rel) to `home_page_url` / item `url`, `self` to `feed_url`, `next`
/// to `next_url`, `hub` to `hubs`, `related` to item `external_url` and `enclosure` to item `attachments`.
/// Media content that is not already an enclosure is added to `attachments`. Unmapped extensions are written back
/// from their raw JSON value when parsing kept one, otherwise from their text.
pub fn to_json_feed(feed: &Feed) -> serde_json::Result<String> {
    serde_json::to_string(&json_document(feed)?)
}

/// Like [`to_json_feed`], but indented for readability.
pub fn to_json_feed_pretty(feed: &Feed) -> serde_json::Result<String> {
    serde_json::to_string_pretty(&json_document(feed)?)
}

fn json_document(feed: &Feed) -> serde_json::Result<Value> {
    let mut value = serde_json::to_value(json_feed(feed))?;
    let Some(object) = value.as_object_mut() else { return Ok(value) };
    merge_extensions(object, &feed.extensions, JSON_FEED_KEYS);
    if let Some(items) = object.get_mut("items").and_then(Value::as_array_mut) {
        for (item, entry) in items.iter_mut().zip(&feed.entries) {
            if let Some(item) = item.as_object_mut() {
                merge_extensions(item, &entry.extensions, JSON_ITEM_KEYS);
            }
        }
    }
    Ok(value)
}

#[derive(Serialize)]
struct JsonFeedOut<'a> {
    version: &'static str,
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    home_page_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feed_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_comment: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    favicon: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    authors: Vec<JsonAuthorOut<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expired: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hubs: Vec<JsonHubOut<'a>>,
    items: Vec<JsonItemOut<'a>>,
}

#[derive(Serialize)]
struct JsonAuthorOut<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<&'a str>,
}

#[derive(Serialize)]
struct JsonHubOut<'a> {
    #[serde(rename = "type")]
    hub_type: &'a str,
    url: &'a str,
}

#[derive(Serialize)]
struct JsonItemOut<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_html: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    banner_image: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_published: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_modified: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    authors: Vec<JsonAuthorOut<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<JsonAttachmentOut<'a>>,
}

#[derive(Serialize)]
struct JsonAttachmentOut<'a> {
    url: &'a str,
    mime_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_in_bytes: Option<u64>,
}

fn link_with_rel<'a>(links: &'a [Link], rels: &[Option<&str>]) -> Option<&'a str> {
    links.iter().find(|l| rels.contains(&l.rel.as_deref())).map(|l| l.href.as_str())
}

fn json_authors(people: &[Person]) -> Vec<JsonAuthorOut<'_>> {
    people
        .iter()
        .filter(|p| p.name.is_some() || p.uri.is_some())
        .map(|p| JsonAuthorOut {
            name: p.name.as_deref(),
            url: p.uri.as_deref(),
        })
        .collect()
}

fn json_feed(feed: &Feed) -> JsonFeedOut<'_> {
    JsonFeedOut {
        version: JSON_FEED_VERSION,
        title: feed.title.as_ref().map(|t| t.content.as_str()).unwrap_or(""),
        home_page_url: link_with_rel(&feed.links, &[Some("alternate"), None]),
        feed_url: link_with_rel(&feed.links, &[Some("self")]),
        next_url: link_with_rel(&feed.links, &[Some("next")]),
        description: feed.description.as_ref().map(|t| t.content.as_str()),
        user_comment: feed.json_user_comment(),
        icon: feed.logo.as_ref().map(|i| i.uri.as_str()),
        favicon: feed.icon.as_ref().map(|i| i.uri.as_str()),
        authors: json_authors(&feed.authors),
        language: feed.language.as_deref(),
        expired: feed.expired,
        hubs: feed
            .links
            .iter()
            .filter(|l| l.rel.as_deref() == Some("hub"))
            .map(|l| JsonHubOut {
                hub_type: l.media_type.as_deref().unwrap_or("WebSub"),
                url: &l.href,
            })
            .collect(),
        items: feed.entries.iter().map(json_item).collect(),
    }
}

fn json_item(entry: &Entry) -> JsonItemOut<'_> {
    let (content_html, content_text) = match &entry.content {
        Some(content) => {
            let body = content.body.as_deref();
            if content.content_type.essence().to_string() == "text/html" {
                (body, None)
            } else {
                (None, body)
            }
        }
        None => (None, None),
    };
    JsonItemOut {
        id: &entry.id,
        url: link_with_rel(&entry.links, &[Some("alternate"), None]),
        external_url: link_with_rel(&entry.links, &[Some("related"), Some("via")]),
        title: entry.title.as_ref().map(|t| t.content.as_str()),
        content_html,
        content_text,
        summary: entry.summary.as_ref().map(|t| t.content.as_str()),
        image: thumbnail(entry, 0),
        banner_image: thumbnail(entry, 1),
        date_published: entry.published.as_ref().map(rfc3339),
        date_modified: entry.updated.as_ref().map(rfc3339),
        authors: json_authors(&entry.authors),
        tags: entry.categories.iter().map(|c| c.term.as_str()).collect(),
        language: entry.language.as_deref(),
        attachments: json_attachments(entry),
    }
}

fn thumbnail(entry: &Entry, index: usize) -> Option<&str> {
    entry
        .media
        .iter()
        .flat_map(|media| media.thumbnails.iter())
        .nth(index)
        .map(|thumb| thumb.image.uri.as_str())
}

fn json_attachments(entry: &Entry) -> Vec<JsonAttachmentOut<'_>> {
    let mut attachments: Vec<JsonAttachmentOut> = entry
        .links
        .iter()
        .filter(|link| link.rel.as_deref() == Some("enclosure"))
        .map(|link| JsonAttachmentOut {
            url: &link.href,
            mime_type: link.media_type.as_deref().unwrap_or("application/octet-stream"),
            title: link.title.as_deref(),
            size_in_bytes: link.length,
        })
        .collect();
    let mut seen: HashSet<&str> = attachments.iter().map(|attachment| attachment.url).collect();
    for media in &entry.media {
        for content in &media.content {
            let Some(url) = content.url.as_ref() else { continue };
            if !seen.insert(url.as_str()) {
                continue;
            }
            attachments.push(JsonAttachmentOut {
                url: url.as_str(),
                mime_type: content.content_type.as_ref().map(|mime| mime.as_str()).unwrap_or("application/octet-stream"),
                title: None,
                size_in_bytes: content.size,
            });
        }
    }
    attachments
}

// Names JSON Feed already uses. Writing an extension under one of these (for example itunes:author as "author")
// would make the document fail to parse, because those keys have a fixed shape.
const JSON_FEED_KEYS: &[&str] = &[
    "version",
    "title",
    "home_page_url",
    "feed_url",
    "description",
    "user_comment",
    "next_url",
    "icon",
    "favicon",
    "author",
    "authors",
    "language",
    "expired",
    "hubs",
    "items",
];
const JSON_ITEM_KEYS: &[&str] = &[
    "id",
    "url",
    "external_url",
    "title",
    "content_html",
    "content_text",
    "summary",
    "image",
    "banner_image",
    "date_published",
    "date_modified",
    "author",
    "authors",
    "tags",
    "language",
    "attachments",
];

fn merge_extensions(object: &mut Map<String, Value>, extensions: &[Extension], reserved: &[&str]) {
    for extension in extensions {
        if reserved.contains(&extension.name.as_str()) || object.contains_key(&extension.name) {
            continue;
        }
        if let Some(value) = &extension.value {
            object.insert(extension.name.clone(), value.clone());
        } else if let Some(text) = &extension.text {
            object.insert(extension.name.clone(), Value::String(text.clone()));
        }
    }
}

fn xmlns_decls(feed: &Feed) -> Vec<(String, String)> {
    xmlns_decls_plus(feed, &[])
}

fn xmlns_decls_plus(feed: &Feed, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut decls = Vec::new();
    for extension in feed.extensions.iter().chain(feed.entries.iter().flat_map(|entry| entry.extensions.iter())) {
        collect_xmlns(extension, &mut decls);
    }
    for (prefix, ns) in extra {
        if !decls.iter().any(|(existing, _)| existing == prefix) {
            decls.push(((*prefix).to_owned(), (*ns).to_owned()));
        }
    }
    decls
}

fn collect_xmlns(extension: &Extension, decls: &mut Vec<(String, String)>) {
    if let (Some(prefix), Some(ns)) = (&extension.prefix, &extension.namespace)
        && is_xml_name(prefix)
        && !decls.iter().any(|(existing, _)| existing == prefix)
    {
        decls.push((prefix.clone(), ns.clone()));
    }
    for attribute in &extension.attributes {
        if let (Some(prefix), Some(ns)) = (&attribute.prefix, &attribute.namespace)
            && prefix != "xml"
            && is_xml_name(prefix)
            && !decls.iter().any(|(existing, _)| existing == prefix)
        {
            decls.push((prefix.clone(), ns.clone()));
        }
    }
    for child in &extension.children {
        collect_xmlns(child, decls);
    }
}

fn push_xmlns(start: &mut BytesStart, decls: Vec<(String, String)>) {
    for (prefix, ns) in decls {
        if prefix == "xml" || !is_xml_name(&prefix) {
            continue;
        }
        let name = format!("xmlns:{prefix}");
        start.push_attribute((name.as_str(), ns.as_str()));
    }
}

// An extension whose name is a core element of the format being written would be parsed back as that field
// and overwrite the value taken from the model (RSS `<atom:id>` is the usual case).
fn atom_core_extension(extension: &Extension) -> bool {
    const NAMES: &[&str] = &[
        "id",
        "title",
        "subtitle",
        "updated",
        "published",
        "author",
        "contributor",
        "link",
        "category",
        "generator",
        "icon",
        "logo",
        "rights",
        "entry",
        "summary",
        "content",
        "name",
        "uri",
        "email",
        "source",
    ];
    if !NAMES.contains(&extension.name.as_str()) {
        return false;
    }
    // No prefix and no namespace is written into Atom's default xmlns, so it would be read back as a core element
    let prefix = extension.prefix.as_deref();
    let namespace = extension.namespace.as_deref();
    matches!((prefix, namespace), (Some("atom"), _) | (_, Some(ATOM_NS)) | (None, None))
}

fn rss_core_extension(extension: &Extension) -> bool {
    const NAMES: &[&str] = &[
        "title",
        "link",
        "description",
        "item",
        "guid",
        "pubDate",
        "category",
        "author",
        "enclosure",
        "image",
        "language",
        "copyright",
        "ttl",
        "generator",
        "lastBuildDate",
        "comments",
    ];
    extension.namespace.as_deref().is_none_or(str::is_empty) && extension.prefix.is_none() && NAMES.contains(&extension.name.as_str())
}

fn write_model_extensions(writer: &mut Writer<Vec<u8>>, extensions: &[Extension], skip: fn(&Extension) -> bool) {
    for extension in extensions {
        if skip(extension) {
            if let Some(tail) = &extension.tail {
                ok(writer.write_event(Event::Text(BytesText::new(tail))));
            }
            continue;
        }
        write_xml_extension(writer, extension);
        if let Some(tail) = &extension.tail {
            ok(writer.write_event(Event::Text(BytesText::new(tail))));
        }
    }
}

fn write_xml_extensions(writer: &mut Writer<Vec<u8>>, extensions: &[Extension]) {
    for extension in extensions {
        write_xml_extension(writer, extension);
        if let Some(tail) = &extension.tail {
            ok(writer.write_event(Event::Text(BytesText::new(tail))));
        }
    }
}

fn write_xml_extension(writer: &mut Writer<Vec<u8>>, extension: &Extension) {
    let Some(qname) = xml_name(extension.prefix.as_deref(), &extension.name) else {
        return;
    };
    let mut start = BytesStart::new(&qname);
    if extension.prefix.is_none()
        && let Some(ns) = &extension.namespace
    {
        start.push_attribute(("xmlns", ns.as_str()));
    }
    for attribute in &extension.attributes {
        let Some(name) = xml_name(attribute.prefix.as_deref(), &attribute.name) else {
            continue;
        };
        start.push_attribute((name.as_str(), attribute.value.as_str()));
    }
    if extension.text.is_none() && extension.children.is_empty() {
        ok(writer.write_event(Event::Empty(start)));
        return;
    }
    ok(writer.write_event(Event::Start(start)));
    if let Some(text) = &extension.text {
        ok(writer.write_event(Event::Text(BytesText::new(text))));
    }
    write_xml_extensions(writer, &extension.children);
    ok(writer.write_event(Event::End(BytesEnd::new(&qname))));
}

fn xml_name(prefix: Option<&str>, name: &str) -> Option<String> {
    if !is_xml_name(name) {
        return None;
    }
    match prefix {
        Some(prefix) if is_xml_name(prefix) => Some(format!("{prefix}:{name}")),
        _ => Some(name.to_owned()),
    }
}

fn is_xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'),
        _ => false,
    }
}
