use chrono::{DateTime, Utc};

use super::*;
use crate::model::FeedType;
use crate::parser;

// The parts of the model both writers promise to preserve
#[derive(Debug, PartialEq)]
struct Core {
    id: String,
    title: Option<String>,
    updated: Option<DateTime<Utc>>,
    links: Vec<String>,
    authors: Vec<Option<String>>,
    entries: Vec<CoreEntry>,
}

#[derive(Debug, PartialEq)]
struct CoreEntry {
    id: String,
    title: Option<String>,
    published: Option<DateTime<Utc>>,
    links: Vec<String>,
    authors: Vec<Option<String>>,
    categories: Vec<String>,
    summary: Option<String>,
    content: Option<String>,
}

fn core(feed: &Feed, with_updated: bool, with_categories: bool) -> Core {
    let mut links: Vec<_> = feed.links.iter().map(|l| l.href.clone()).collect();
    links.sort();
    links.dedup();
    Core {
        id: feed.id.clone(),
        // Atom and JSON Feed require a title, so a missing one is written as empty
        title: feed.title.as_ref().map(|t| t.content.clone()).filter(|t| !t.is_empty()),
        updated: if with_updated { feed.updated } else { None },
        links,
        authors: feed.authors.iter().map(|p| p.name.clone()).collect(),
        entries: feed
            .entries
            .iter()
            .map(|e| {
                let mut links: Vec<_> = e.links.iter().map(|l| l.href.clone()).collect();
                links.sort();
                links.dedup();
                CoreEntry {
                    id: e.id.clone(),
                    title: e.title.as_ref().map(|t| t.content.clone()).filter(|t| !t.is_empty()),
                    published: e.published,
                    links,
                    authors: e.authors.iter().map(|p| p.name.clone()).collect(),
                    categories: if with_categories {
                        e.categories.iter().map(|c| c.term.clone()).collect()
                    } else {
                        vec![]
                    },
                    summary: e.summary.as_ref().map(|t| t.content.clone()),
                    // Parsers trim content bodies, so surrounding whitespace is not significant
                    content: e.content.as_ref().and_then(|c| c.body.as_deref()).map(|b| b.trim().to_string()),
                }
            })
            .collect(),
    }
}

fn fixtures() -> Vec<(String, Feed)> {
    let parser = parser::Builder::new().sanitize_content(false).build();
    let mut feeds = Vec::new();
    for dir in ["atom", "jsonfeed", "rss0", "rss1", "rss2"] {
        for path in std::fs::read_dir(format!("fixture/{dir}")).unwrap().map(|e| e.unwrap().path()) {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if name.ends_with(".serde.json") {
                continue;
            }
            if let Ok(feed) = parser.parse(std::fs::read(&path).unwrap().as_slice()) {
                feeds.push((name, feed));
            }
        }
    }
    assert!(feeds.len() > 50);
    feeds
}

#[test]
fn atom_round_trip_preserves_core_fields_for_all_fixtures() {
    let parser = parser::Builder::new().sanitize_content(false).build();
    for (name, feed) in fixtures() {
        let atom = to_atom(&feed);
        let again = parser.parse(atom.as_bytes()).unwrap_or_else(|e| panic!("{name}: {e}\n{atom}"));
        assert_eq!(again.feed_type, FeedType::Atom);
        assert_eq!(core(&again, feed.updated.is_some(), true), core(&feed, feed.updated.is_some(), true), "{name}");
    }
}

#[test]
fn json_round_trip_preserves_core_fields_for_all_fixtures() {
    let parser = parser::Builder::new().sanitize_content(false).build();
    for (name, feed) in fixtures() {
        let json = to_json_feed(&feed).unwrap();
        let again = parser.parse(json.as_bytes()).unwrap_or_else(|e| panic!("{name}: {e}\n{json}"));
        assert_eq!(again.feed_type, FeedType::JSON);

        // JSON Feed has no feed id or updated date, keeps one home page / feed URL, and turns all content into
        // either HTML or text, so compare only the entry-level fields it carries
        let (expected, actual) = (core(&feed, false, true), core(&again, false, true));
        assert_eq!(actual.title, expected.title, "{name}");
        assert_eq!(actual.entries.len(), expected.entries.len(), "{name}");
        for (a, e) in actual.entries.iter().zip(&expected.entries) {
            assert_eq!(a.id, e.id, "{name}");
            assert_eq!(a.title, e.title, "{name}");
            assert_eq!(a.published, e.published, "{name}");
            assert_eq!(a.categories, e.categories, "{name}");
            assert_eq!(a.content, e.content, "{name}");
        }
    }
}

#[test]
fn atom_output_is_escaped_and_well_formed() {
    let mut feed = Feed::new(FeedType::RSS2);
    feed.id = "urn:x".into();
    feed.title = Some(Text::new(r#"Tom & "Jerry" <3"#.into()));
    feed.entries.push(Entry {
        id: "1".into(),
        title: Some(Text::html("<b>bold</b> &amp; more".into())),
        updated: Some("2024-01-02T03:04:05Z".parse().unwrap()),
        ..Default::default()
    });

    let atom = to_atom(&feed);
    assert!(atom.starts_with(r#"<?xml version="1.0" encoding="utf-8"?>"#), "{atom}");
    assert!(atom.contains("Tom &amp; &quot;Jerry&quot; &lt;3"), "{atom}");
    assert!(atom.contains(r#"<title type="html">&lt;b&gt;bold&lt;/b&gt; &amp;amp; more</title>"#), "{atom}");
    // The feed's updated date falls back to the newest entry
    assert!(atom.contains("<updated>2024-01-02T03:04:05Z</updated>"), "{atom}");

    let again = parser::Builder::new().sanitize_content(false).build().parse(atom.as_bytes()).unwrap();
    assert_eq!(again.title.unwrap().content, r#"Tom & "Jerry" <3"#);
    assert_eq!(again.entries[0].title.as_ref().unwrap().content, "<b>bold</b> &amp; more");
}

#[test]
fn json_feed_maps_links_by_rel() {
    let json = r#"{"version":"https://jsonfeed.org/version/1.1","title":"t","home_page_url":"https://a/","feed_url":"https://a/feed.json",
        "next_url":"https://a/feed.json?p=2","hubs":[{"type":"WebSub","url":"https://hub/"}],
        "items":[{"id":"1","url":"https://a/1","external_url":"https://b/","image":"https://a/1.png","language":"de",
                  "attachments":[{"url":"https://a/1.mp3","mime_type":"audio/mpeg","size_in_bytes":10}]}]}"#;
    let feed = parser::parse(json.as_bytes()).unwrap();
    let out: serde_json::Value = serde_json::from_str(&to_json_feed(&feed).unwrap()).unwrap();
    let input: serde_json::Value = serde_json::from_str(json).unwrap();
    for key in ["home_page_url", "feed_url", "next_url", "hubs"] {
        assert_eq!(out[key], input[key], "{key}");
    }
    for key in ["id", "url", "external_url", "image", "language", "attachments"] {
        assert_eq!(out["items"][0][key], input["items"][0][key], "{key}");
    }
    assert!(
        to_json_feed_pretty(&feed)
            .unwrap()
            .contains("\n  \"version\": \"https://jsonfeed.org/version/1.1\"")
    );
}
