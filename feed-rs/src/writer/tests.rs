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
    // Enclosure links written for media objects are not part of the original link list
    let mut links: Vec<_> = feed
        .links
        .iter()
        .filter(|l| l.rel.as_deref() != Some("enclosure"))
        .map(|l| l.href.clone())
        .collect();
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
                let mut links: Vec<_> = e
                    .links
                    .iter()
                    .filter(|l| l.rel.as_deref() != Some("enclosure"))
                    .map(|l| l.href.clone())
                    .collect();
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
        // Extensions that Atom also understands (for example a captured <link>) may add links on the way back.
        // Every original core value must still be there.
        let (left, right) = (core(&again, feed.updated.is_some(), true), core(&feed, feed.updated.is_some(), true));
        assert_eq!(left.id, right.id, "{name}");
        assert_eq!(left.title, right.title, "{name}");
        assert_eq!(left.updated, right.updated, "{name}");
        assert_eq!(left.authors, right.authors, "{name}");
        for link in &right.links {
            assert!(left.links.contains(link), "{name}: lost feed link {link}");
        }
        assert_eq!(left.entries.len(), right.entries.len(), "{name}");
        for (l, r) in left.entries.iter().zip(&right.entries) {
            assert_eq!(
                (&l.id, &l.title, &l.published, &l.authors, &l.categories, &l.summary, &l.content),
                (&r.id, &r.title, &r.published, &r.authors, &r.categories, &r.summary, &r.content),
                "{name}"
            );
            for link in &r.links {
                assert!(l.links.contains(link), "{name}: lost entry link {link}");
            }
        }
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

#[test]
fn rss_round_trip_keeps_items_media_and_extensions() {
    use crate::model::{Extension, ExtensionAttribute, MediaContent, MediaObject};

    let xml = r#"<rss version="2.0" xmlns:x="urn:x"><channel>
        <title>Channel</title><link>https://example.com/</link><description>About</description>
        <x:note lang="en">hello</x:note>
        <item>
          <title>Hello</title><link>https://example.com/hello</link>
          <description>Sum</description>
          <content:encoded xmlns:content="http://purl.org/rss/1.0/modules/content/">&lt;p&gt;Body&lt;/p&gt;</content:encoded>
          <enclosure url="https://example.com/a.mp3" length="12" type="audio/mpeg"/>
          <guid>https://example.com/hello</guid>
          <x:flag>1</x:flag>
        </item>
      </channel></rss>"#;
    let parser = parser::Builder::new().sanitize_content(false).build();
    let feed = parser.parse(xml.as_bytes()).unwrap();
    let rss = to_rss(&feed);
    let again = parser.parse(rss.as_bytes()).unwrap_or_else(|e| panic!("{e}\n{rss}"));

    assert_eq!(again.title.as_ref().unwrap().content, "Channel");
    assert_eq!(again.description.as_ref().unwrap().content, "About");
    assert_eq!(again.extension(Some("urn:x"), "note").unwrap().text.as_deref(), Some("hello"));
    assert_eq!(again.extension(Some("urn:x"), "note").unwrap().attr("lang"), Some("en"));
    let entry = &again.entries[0];
    assert_eq!(entry.title.as_ref().unwrap().content, "Hello");
    assert_eq!(entry.summary.as_ref().unwrap().content, "Sum");
    assert_eq!(entry.content.as_ref().unwrap().body.as_deref(), Some("<p>Body</p>"));
    assert_eq!(entry.id, "https://example.com/hello");
    assert_eq!(entry.extension(Some("urn:x"), "flag").unwrap().text.as_deref(), Some("1"));
    let enclosure = entry
        .media
        .iter()
        .flat_map(|media| media.content.iter())
        .find_map(|content| content.url.as_ref())
        .unwrap();
    assert_eq!(enclosure.as_str(), "https://example.com/a.mp3");
    assert!(rss.contains("media:content"), "{rss}");

    // A media object that arrived as a field (not an enclosure link) is still written
    let mut content = MediaContent::new();
    content.url = Some("https://cdn.example/v.mp4".parse().unwrap());
    content.content_type = Some("video/mp4".parse().unwrap());
    content.size = Some(9);
    let media = MediaObject {
        title: Some(crate::model::Text::new("Clip".into())),
        content: vec![content],
        ..Default::default()
    };
    let mut built = crate::model::Feed::new(FeedType::RSS2);
    built.title = Some(crate::model::Text::new("t".into()));
    built.entries.push(crate::model::Entry {
        id: "1".into(),
        title: Some(crate::model::Text::new("item".into())),
        media: vec![media],
        extensions: vec![{
            let mut extension = Extension::named(Some("urn:x"), "extra", Some("z".into()));
            extension.prefix = Some("x".into());
            extension.attributes.push(ExtensionAttribute {
                name: "k".into(),
                value: "v".into(),
                prefix: None,
                namespace: None,
            });
            extension
        }],
        ..Default::default()
    });
    let rss = to_rss(&built);
    let again = parser.parse(rss.as_bytes()).unwrap_or_else(|e| panic!("{e}\n{rss}"));
    assert_eq!(again.entries[0].media[0].content[0].url.as_ref().unwrap().as_str(), "https://cdn.example/v.mp4");
    assert_eq!(again.entries[0].media[0].title.as_ref().unwrap().content, "Clip");
    assert_eq!(again.entries[0].extension(Some("urn:x"), "extra").unwrap().attr("k"), Some("v"));
}
