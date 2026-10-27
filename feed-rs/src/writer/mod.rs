//! Serialises a [`Feed`] back out as [Atom 1.0](https://www.rfc-editor.org/rfc/rfc4287) or
//! [JSON Feed 1.1](https://www.jsonfeed.org/version/1.1/).
//!
//! This is useful for converting between formats (e.g. normalising RSS to Atom) or re-publishing a filtered feed.
//! The output covers the core model: ids, titles, links, dates, people, categories, summaries, content, rights,
//! language, generator, icon and logo. Media objects (other than JSON Feed attachments derived from `enclosure`
//! links) and captured extensions are not written.
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
//! let again = parser::parse(atom.as_bytes()).unwrap();
//! assert_eq!(again.entries[0].title.as_ref().unwrap().content, "Hello");
//!
//! let json = writer::to_json_feed(&feed).unwrap();
//! assert!(json.contains(r#""title":"Hello""#));
//! ```

use chrono::{DateTime, SecondsFormat, Utc};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use serde::Serialize;

use crate::model::{Category, Content, Entry, Feed, Link, Person, Text};

#[cfg(test)]
mod tests;

const ATOM_NS: &str = "http://www.w3.org/2005/Atom";
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
// JSON Feed
// ---------------------------------------------------------------------------------------------------------------------

/// Serialises the feed as a JSON Feed 1.1 document.
///
/// Links are mapped by `rel`: `alternate` (or no rel) to `home_page_url` / item `url`, `self` to `feed_url`, `next`
/// to `next_url`, `hub` to `hubs`, `related` to item `external_url` and `enclosure` to item `attachments`.
pub fn to_json_feed(feed: &Feed) -> serde_json::Result<String> {
    serde_json::to_string(&json_feed(feed))
}

/// Like [`to_json_feed`], but indented for readability.
pub fn to_json_feed_pretty(feed: &Feed) -> serde_json::Result<String> {
    serde_json::to_string_pretty(&json_feed(feed))
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
    icon: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    favicon: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    authors: Vec<JsonAuthorOut<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
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
        icon: feed.logo.as_ref().map(|i| i.uri.as_str()),
        favicon: feed.icon.as_ref().map(|i| i.uri.as_str()),
        authors: json_authors(&feed.authors),
        language: feed.language.as_deref(),
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
        image: entry.media.iter().flat_map(|m| m.thumbnails.first()).map(|t| t.image.uri.as_str()).next(),
        date_published: entry.published.as_ref().map(rfc3339),
        date_modified: entry.updated.as_ref().map(rfc3339),
        authors: json_authors(&entry.authors),
        tags: entry.categories.iter().map(|c| c.term.as_str()).collect(),
        language: entry.language.as_deref(),
        attachments: entry
            .links
            .iter()
            .filter(|l| l.rel.as_deref() == Some("enclosure"))
            .map(|l| JsonAttachmentOut {
                url: &l.href,
                mime_type: l.media_type.as_deref().unwrap_or("application/octet-stream"),
                title: l.title.as_deref(),
                size_in_bytes: l.length,
            })
            .collect(),
    }
}
