//! Per-format tests that elements without a dedicated model field are kept as extensions (Q4).

use crate::model::Extension;
use crate::parser;

const DC: &str = "http://purl.org/dc/elements/1.1/";
const ITUNES: &str = "http://www.itunes.com/dtds/podcast-1.0.dtd";
const PODCAST: &str = "https://podcastindex.org/namespace/1.0";
const MEDIA: &str = "http://search.yahoo.com/mrss/";
const CUSTOM: &str = "urn:example:custom";

fn find<'a>(extensions: &'a [Extension], ns: &str, name: &str) -> &'a Extension {
    extensions.iter().find(|e| e.is(Some(ns), name)).unwrap_or_else(|| {
        panic!(
            "no {{{ns}}}{name} in {:?}",
            extensions.iter().map(|e| (&e.namespace, &e.name)).collect::<Vec<_>>()
        )
    })
}

#[test]
fn atom_extensions() {
    let xml = format!(
        r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:c="{CUSTOM}">
            <title>t</title>
            <c:feedLevel kind="a"><c:child>x</c:child></c:feedLevel>
            <entry><id>1</id><title>e</title><c:entryLevel>y</c:entryLevel></entry>
        </feed>"#
    );
    let feed = parser::parse(xml.as_bytes()).unwrap();
    let ext = find(&feed.extensions, CUSTOM, "feedLevel");
    assert_eq!(ext.prefix.as_deref(), Some("c"));
    assert_eq!(ext.attr("kind"), Some("a"));
    assert_eq!(ext.child(Some(CUSTOM), "child").unwrap().text.as_deref(), Some("x"));
    assert_eq!(find(&feed.entries[0].extensions, CUSTOM, "entryLevel").text.as_deref(), Some("y"));
}

#[test]
fn rss1_extensions() {
    let xml = format!(
        r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns="http://purl.org/rss/1.0/" xmlns:c="{CUSTOM}">
            <channel rdf:about="http://a/"><title>t</title><link>http://a/</link><c:channelLevel>x</c:channelLevel></channel>
            <item rdf:about="http://a/1"><title>i</title><link>http://a/1</link><c:itemLevel>y</c:itemLevel></item>
        </rdf:RDF>"#
    );
    let feed = parser::parse(xml.as_bytes()).unwrap();
    assert_eq!(find(&feed.extensions, CUSTOM, "channelLevel").text.as_deref(), Some("x"));
    assert_eq!(find(&feed.entries[0].extensions, CUSTOM, "itemLevel").text.as_deref(), Some("y"));
}

#[test]
fn rss2_dublin_core_and_custom_extensions() {
    let xml = format!(
        r#"<rss version="2.0" xmlns:dc="{DC}" xmlns:c="{CUSTOM}"><channel><title>t</title>
            <c:channelLevel>x</c:channelLevel>
            <item><title>i</title><dc:coverage>world</dc:coverage><c:itemLevel n="1"/></item>
        </channel></rss>"#
    );
    let feed = parser::parse(xml.as_bytes()).unwrap();
    assert_eq!(find(&feed.extensions, CUSTOM, "channelLevel").text.as_deref(), Some("x"));
    let entry = &feed.entries[0];
    assert_eq!(find(&entry.extensions, DC, "coverage").text.as_deref(), Some("world"));
    assert_eq!(find(&entry.extensions, CUSTOM, "itemLevel").attr("n"), Some("1"));
}

#[test]
fn itunes_extensions() {
    let xml = format!(
        r#"<rss version="2.0" xmlns:itunes="{ITUNES}"><channel><title>t</title>
            <itunes:new-feed-url>https://new.example/feed</itunes:new-feed-url>
            <itunes:type>serial</itunes:type>
            <item><title>i</title><itunes:episodeType>trailer</itunes:episodeType><itunes:block>Yes</itunes:block></item>
        </channel></rss>"#
    );
    let feed = parser::parse(xml.as_bytes()).unwrap();
    assert_eq!(find(&feed.extensions, ITUNES, "new-feed-url").text.as_deref(), Some("https://new.example/feed"));
    assert_eq!(find(&feed.extensions, ITUNES, "type").text.as_deref(), Some("serial"));
    let entry = &feed.entries[0];
    assert_eq!(find(&entry.extensions, ITUNES, "episodeType").text.as_deref(), Some("trailer"));
    assert_eq!(find(&entry.extensions, ITUNES, "block").text.as_deref(), Some("Yes"));
}

#[test]
fn podcast_extensions() {
    let xml = format!(
        r#"<rss version="2.0" xmlns:podcast="{PODCAST}"><channel><title>t</title>
            <podcast:locked owner="me@example.com">yes</podcast:locked>
            <podcast:funding url="https://donate.example/">Support us</podcast:funding>
            <podcast:value type="lightning" method="keysend"><podcast:valueRecipient name="host" split="100" address="abc"/></podcast:value>
            <item><title>i</title><podcast:chapters url="https://a/ch.json" type="application/json+chapters"/>
                <podcast:soundbite startTime="10" duration="30">Best bit</podcast:soundbite></item>
        </channel></rss>"#
    );
    let feed = parser::parse(xml.as_bytes()).unwrap();
    let locked = find(&feed.extensions, PODCAST, "locked");
    assert_eq!((locked.attr("owner"), locked.text.as_deref()), (Some("me@example.com"), Some("yes")));
    assert_eq!(find(&feed.extensions, PODCAST, "funding").attr("url"), Some("https://donate.example/"));
    let value = find(&feed.extensions, PODCAST, "value");
    assert_eq!(value.child(Some(PODCAST), "valueRecipient").unwrap().attr("split"), Some("100"));
    let entry = &feed.entries[0];
    assert_eq!(find(&entry.extensions, PODCAST, "chapters").attr("url"), Some("https://a/ch.json"));
    assert_eq!(find(&entry.extensions, PODCAST, "soundbite").attr("startTime"), Some("10"));
}

#[test]
fn mediarss_group_extensions() {
    let xml = format!(
        r#"<rss version="2.0" xmlns:media="{MEDIA}"><channel><title>t</title>
            <item><title>i</title>
              <media:group>
                <media:content url="http://a/1.mp4" type="video/mp4"/>
                <media:restriction relationship="allow" type="country">us ca</media:restriction>
                <media:license type="text/html" href="http://a/license">CC</media:license>
              </media:group>
              <media:status state="active"/>
            </item>
        </channel></rss>"#
    );
    let feed = parser::parse(xml.as_bytes()).unwrap();
    let entry = &feed.entries[0];
    assert_eq!(entry.media.len(), 1);
    assert_eq!(entry.media[0].content.len(), 1);
    assert_eq!(find(&entry.extensions, MEDIA, "restriction").text.as_deref(), Some("us ca"));
    assert_eq!(find(&entry.extensions, MEDIA, "license").attr("href"), Some("http://a/license"));
    assert_eq!(find(&entry.extensions, MEDIA, "status").attr("state"), Some("active"));
}
