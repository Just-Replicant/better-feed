use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::model::Feed;
use crate::parser;
use crate::parser::{ParseErrorKind, ParseFeedError};
use crate::util::test;

// Regression test for the default ID generator
#[test]
fn id_generator_default() {
    let test_data = test::fixture_as_raw("rss2/rss_2.0_kdist.xml");
    let feed = parser::parse(test_data.as_slice()).unwrap();
    assert_eq!("354331764be7571efc15c7a1bad13d54", feed.id);
}

// Verifies failure uncovered by fuzzing is now fixed
#[test]
fn fuzz_parse() {
    let data: Vec<u8> = vec![
        0xdb, 0x3b, 0x3c, 0x66, 0x65, 0x65, 0x64, 0x3e, 0x00, 0xfe, 0xff, 0x00, 0x00, 0x00, 0x3c, 0x1b, 0x3b, 0x64, 0x22, 0x22, 0x0d, 0x78, 0x6d, 0x6c, 0x3a,
        0x62, 0x61, 0x73, 0x65, 0x0d, 0x0d, 0x3d, 0x0a, 0x22, 0x0a, 0x0d, 0x0a, 0x0a, 0x0d, 0x66, 0x69, 0x6c, 0x65, 0x3a, 0xff, 0x3b, 0xbf, 0x5b, 0xbf, 0xbf,
        0xbc, 0xff, 0xff, 0x0a, 0x53, 0x53, 0x2b, 0x78, 0x3b, 0x22, 0x3c, 0x64, 0x3e, 0x2b, 0x00, 0x00, 0x2b, 0x3c, 0xdb, 0x3b, 0x32, 0x65, 0x64, 0x22, 0x22,
        0x0d, 0x78, 0x6d, 0x6c, 0x3a, 0x62, 0x61, 0x73, 0x65, 0x0d, 0x0d, 0x3d, 0x22, 0x75, 0x7c, 0x3f, 0x0a, 0x34, 0x0a, 0xff, 0x22, 0x34, 0x3a, 0xb5, 0x2f,
        0x3c, 0x66, 0x65, 0x64, 0x3e, 0x2b, 0x3c, 0xdb, 0x3b, 0x32, 0x65, 0x0d, 0x78, 0x6d, 0x6c, 0x3a, 0x62, 0x61, 0x73, 0x65, 0x0d, 0x0d, 0x3d, 0x22, 0x2e,
        0x2e, 0x3f, 0x0a, 0x3c, 0x3f, 0xff, 0x22, 0x34, 0x3a, 0xb5, 0x2f, 0x2f, 0xff, 0xff, 0xfe, 0x01, 0xdb, 0x3b, 0x3c, 0x66, 0x65,
    ];

    let result = parser::parse(data.as_slice());
    assert!(result.is_err());
}

// Verifies that a round-trip through the parser + serde works correctly over time
// If you legitimately need to regenerate the serialised fixture data then run: cargo run --bin fixture_serde
#[test]
fn serde_regression() {
    let fixture_root_dir = test::fixture_dir();
    find_fixture_files(&fixture_root_dir, |source_path, json_path| {
        // Parse the original fixture file
        let mut data = fs::read(source_path).unwrap();
        // Working copies on Windows are CRLF. The fixtures were written for LF.
        data.retain(|byte| *byte != b'\r');
        let parser = parser::Builder::default().sanitize_content(false).build();
        let mut feed = parser.parse(data.as_slice()).unwrap();

        // Parse the previously serialised form
        let serde_data = fs::read(json_path).unwrap();
        let mut serde_feed = serde_json::from_slice(&serde_data).unwrap();

        // Basic check, and then try with replaced IDs too
        if feed != serde_feed {
            // Replace the IDs in the serialised form if UUIDs
            replace_ids(&mut feed, &mut serde_feed);

            assert_eq!(feed, serde_feed);
        }
    });
}

fn find_fixture_files(fixture_root: &PathBuf, callback: fn(&Path, &Path)) {
    fs::read_dir(fixture_root).unwrap().map(|entry| entry.unwrap()).for_each(|entry| {
        let source_path = entry.path();
        if source_path.is_dir() {
            find_fixture_files(&source_path, callback);
        } else {
            // Process the xml + json base files
            let path_str = source_path.to_str().unwrap();
            if path_str.ends_with(".xml") || path_str.ends_with(".json") {
                // Ignore if no serde companion file
                let json_path = source_path.with_extension("serde.json");
                if json_path.exists() {
                    callback(&source_path, &json_path);
                }
            }
        }
    });
}

fn replace_ids(expected: &mut Feed, actual: &mut Feed) {
    if Uuid::parse_str(&expected.id).is_ok() {
        actual.id = expected.id.clone();
    }

    for (expected_entry, actual_entry) in expected.entries.iter().zip(actual.entries.iter_mut()) {
        if Uuid::parse_str(&expected_entry.id).is_ok() {
            actual_entry.id = expected_entry.id.clone();
        }
    }
}

#[test]
fn test_broken() {
    let broken_rss = r#"
        <?xml version="1.0" encoding="UTF-8"?><?xml-stylesheet href="stylesheet.xsl" type="text/xsl"?><rss version="2.0" xmlns:podcast="https://podcastindex.org/namespace/1.0"><channel><podcast:person role="ThisIsNotAValidRole" href="https://ahl.dtrace.org/">Adam Leventhal</podcast:person></channel></rss>
    "#;
    match parser::parse(broken_rss.as_bytes()) {
        Err(ParseFeedError::ParseError(kind)) => match kind {
            ParseErrorKind::UnknownEnumVariant {
                position,
                reference: _reference,
            } => {
                assert_eq!(position, 260)
            }
            _ => panic!("Incorrect error kind {:?}", kind),
        },
        _ => panic!("Should not have been able to parse broken feed"),
    }
}

// Regression (C4): a parse error part-way through an item left MediaRSS state behind,
// so the next parse with the same parser panicked on an assertion
#[test]
fn parser_reusable_after_error() {
    let parser = parser::Builder::new().build();

    let bad = r#"<rss version="2.0"><channel><item><title>a</title><enclosure url="x"></item></channel></rss>"#;
    assert!(parser.parse(bad.as_bytes()).is_err());

    let bad_atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><title>a</title><link></entry></feed>"#;
    assert!(parser.parse(bad_atom.as_bytes()).is_err());

    let good = r#"<rss version="2.0"><channel><item><title>a</title></item></channel></rss>"#;
    let feed = parser.parse(good.as_bytes()).unwrap();
    assert_eq!(feed.entries.len(), 1);
}

// Regression (Q1): the parser can be shared between threads
#[test]
fn parser_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<parser::Parser>();

    let parser = std::sync::Arc::new(parser::Builder::new().build());
    let handles = (0..4)
        .map(|i| {
            let parser = parser.clone();
            std::thread::spawn(move || {
                let xml = format!(r#"<rss version="2.0"><channel><item><title>{i}</title></item></channel></rss>"#);
                parser.parse(xml.as_bytes()).unwrap().entries.len()
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        assert_eq!(handle.join().unwrap(), 1);
    }
}

// Builds an RSS item containing `depth` nested unknown elements
fn deeply_nested_rss(depth: usize) -> String {
    let mut xml = String::from(r#"<rss version="2.0"><channel><item><title>t</title><x:a xmlns:x="urn:x">"#);
    for _ in 0..depth {
        xml.push_str("<x:a>");
    }
    for _ in 0..depth {
        xml.push_str("</x:a>");
    }
    xml.push_str("</x:a></item></channel></rss>");
    xml
}

// Depth of the deepest nested extension
fn extension_depth(extension: &crate::model::Extension) -> usize {
    1 + extension.children.iter().map(extension_depth).max().unwrap_or(0)
}

// Regression (C1): extension capture recursed once per nesting level, so ~50k nested unknown
// elements overflowed the stack and aborted the process. Upstream feed-rs returned an error.
#[test]
fn deeply_nested_extensions_do_not_overflow() {
    // Run on a small stack to prove the walk is iterative
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let xml = deeply_nested_rss(50_000);
            let feed = parser::parse(xml.as_bytes()).unwrap();
            let entry = &feed.entries[0];
            assert_eq!(entry.title.as_ref().unwrap().content, "t");
            assert_eq!(entry.extensions.len(), 1);
            // Default depth limit is 32
            assert_eq!(extension_depth(&entry.extensions[0]), 32);
        })
        .unwrap()
        .join()
        .unwrap();
}

// Beyond quick-xml's own nesting limit we still get an error rather than a crash
#[test]
fn nesting_beyond_xml_limit_is_an_error() {
    let xml = deeply_nested_rss(70_000);
    assert!(parser::parse(xml.as_bytes()).is_err());
}

#[test]
fn extension_limits_are_applied() {
    let limits = parser::ParseLimits::default()
        .with_max_extension_depth(2)
        .with_max_extension_nodes(3)
        .with_max_extension_text_bytes(4);
    let parser = parser::Builder::new().limits(limits).build();

    let xml = r#"<rss version="2.0" xmlns:x="urn:x"><channel><item>
        <x:a>héllo world<x:b><x:c>deep</x:c></x:b></x:a>
        <x:d>second</x:d>
        <x:e>over budget</x:e>
    </item></channel></rss>"#;
    let feed = parser.parse(xml.as_bytes()).unwrap();
    let extensions = &feed.entries[0].extensions;
    // a, b (depth 2) and d use the 3-node budget; c is too deep and e is over budget
    assert_eq!(extensions.len(), 2);
    assert_eq!(extensions[0].name, "a");
    // Truncated to 4 bytes at a character boundary ("hé" is 3 bytes, "l" makes 4)
    assert_eq!(extensions[0].text.as_deref(), Some("hél"));
    assert_eq!(extensions[0].children.len(), 1);
    assert!(extensions[0].children[0].children.is_empty());
    assert_eq!(extensions[1].name, "d");
}

#[test]
fn extension_capture_can_be_disabled() {
    let parser = parser::Builder::new().capture_extensions(false).build();
    let xml = r#"<rss version="2.0" xmlns:x="urn:x"><channel><x:feedlevel>1</x:feedlevel><title>t</title>
        <item><x:a><x:b>1</x:b></x:a><title>i</title></item></channel></rss>"#;
    let feed = parser.parse(xml.as_bytes()).unwrap();
    assert!(feed.extensions.is_empty());
    assert!(feed.entries[0].extensions.is_empty());
    // Mapped fields after the skipped elements are still parsed
    assert_eq!(feed.title.unwrap().content, "t");
    assert_eq!(feed.entries[0].title.as_ref().unwrap().content, "i");

    let json = r#"{"version":"https://jsonfeed.org/version/1.1","title":"t","_x":1,"items":[{"id":"1","_y":2}]}"#;
    let feed = parser.parse(json.as_bytes()).unwrap();
    assert!(feed.extensions.is_empty());
    assert!(feed.entries[0].extensions.is_empty());
}

#[test]
fn max_input_bytes_is_enforced() {
    let xml = r#"<rss version="2.0"><channel><title>t</title></channel></rss>"#;

    let limits = parser::ParseLimits::default().with_max_input_bytes(Some(xml.len() as u64));
    let parser = parser::Builder::new().limits(limits.clone()).build();
    assert!(parser.parse(xml.as_bytes()).is_ok());

    let limits = limits.with_max_input_bytes(Some(xml.len() as u64 - 1));
    let parser = parser::Builder::new().limits(limits).build();
    match parser.parse(xml.as_bytes()) {
        Err(ParseFeedError::ParseError(ParseErrorKind::InputTooLarge { limit })) => assert_eq!(limit, xml.len() as u64 - 1),
        other => panic!("expected InputTooLarge, got {:?}", other),
    }
}

// B8: extensions keep the prefix, attribute namespaces and mixed-content tails
#[test]
fn extension_model_is_lossless_for_xml() {
    let xml = r#"<rss version="2.0" xmlns:x="urn:x" xmlns:y="urn:y"><channel><item>
        <x:note y:kind="a" plain="b" xml:lang="en">Hello <x:b>bold</x:b> world <x:i>it</x:i>!</x:note>
    </item></channel></rss>"#;
    let feed = parser::parse(xml.as_bytes()).unwrap();
    let note = &feed.entries[0].extensions[0];
    assert!(note.is(Some("urn:x"), "note"));
    assert_eq!(note.prefix.as_deref(), Some("x"));
    assert_eq!(note.text.as_deref(), Some("Hello"));
    assert_eq!(note.children[0].text.as_deref(), Some("bold"));
    assert_eq!(note.children[0].tail.as_deref(), Some("world"));
    assert_eq!(note.children[1].tail.as_deref(), Some("!"));
    assert_eq!(note.all_text(), "Hello bold world it !");

    assert_eq!(note.attr("kind"), Some("a"));
    assert_eq!(note.attr_ns(Some("urn:y"), "kind"), Some("a"));
    assert_eq!(note.attr_ns(None, "plain"), Some("b"));
    let lang = note.attributes.iter().find(|a| a.name == "lang").unwrap();
    assert_eq!(lang.prefix.as_deref(), Some("xml"));
    assert_eq!(lang.namespace.as_deref(), Some("http://www.w3.org/XML/1998/namespace"));
}

// B8: JSON extensions keep the raw value, so arrays and nulls are not lost
#[test]
fn extension_model_is_lossless_for_json() {
    let json = r#"{"version":"https://jsonfeed.org/version/1.1","title":"t","_x":{"a":["c, d"],"b":["c","d"],"n":null},"_null":null,"items":[]}"#;
    let feed = parser::parse(json.as_bytes()).unwrap();
    let x = feed.extensions.iter().find(|e| e.name == "_x").unwrap();
    let value = x.value.as_ref().unwrap();
    assert_eq!(value["a"], serde_json::json!(["c, d"]));
    assert_eq!(value["b"], serde_json::json!(["c", "d"]));
    assert!(value["n"].is_null());
    let null = feed.extensions.iter().find(|e| e.name == "_null").unwrap();
    assert_eq!(null.value, Some(serde_json::Value::Null));
}

// Regression (C2): an author with a non-ASCII separator next to the email panicked
#[test]
fn author_with_multibyte_separator_does_not_panic() {
    for author in ["Name\u{00A0}a@b.com", "a@b.com\u{00A0}Name"] {
        let xml = format!(r#"<rss version="2.0"><channel><item><author>{author}</author></item></channel></rss>"#);
        let feed = parser::parse(xml.as_bytes()).unwrap();
        let person = &feed.entries[0].authors[0];
        assert_eq!(person.name.as_deref(), Some("Name"));
        assert_eq!(person.email.as_deref(), Some("a@b.com"));
    }
}

// Regression (C3): huge durations panicked instead of being ignored
#[test]
fn huge_durations_are_ignored() {
    for duration in ["99999999999999999999999", "9999999999999999:00:00"] {
        let xml = format!(
            r#"<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd" xmlns:media="http://search.yahoo.com/mrss/"><channel><item>
            <itunes:title>t</itunes:title><itunes:duration>{duration}</itunes:duration>
            <media:content url="http://a/b.mp4"><media:text start="{duration}">x</media:text></media:content>
            </item></channel></rss>"#
        );
        let feed = parser::parse(xml.as_bytes()).unwrap();
        assert!(feed.entries[0].media.iter().all(|m| m.duration.is_none()));
    }
}

// Regression (C5): attribute values in re-serialised XHTML / HTML content were not escaped,
// producing malformed markup and allowing attribute injection
#[test]
fn serialised_content_escapes_attribute_values() {
    let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"><id>1</id><entry><id>e</id>
        <content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml"><a href="http://a/?x=1&amp;y=&quot;2&quot;" title="&lt;b&gt;">t</a><img src="x&quot; onerror=&quot;alert(1)"/></div></content>
    </entry></feed>"#;
    let parser = parser::Builder::new().sanitize_content(false).build();
    let feed = parser.parse(xml.as_bytes()).unwrap();
    let body = feed.entries[0].content.as_ref().unwrap().body.clone().unwrap();
    assert_eq!(
        body,
        r#"<a href="http://a/?x=1&amp;y=&quot;2&quot;" title="&lt;b&gt;">t</a><img src="x&quot; onerror=&quot;alert(1)"></img>"#
    );
    assert!(!body.contains(r#"" onerror=""#));
}

// Regression (B1): iTunes data was dropped unless the item also had itunes:title or itunes:summary
#[test]
fn itunes_only_metadata_is_kept() {
    let xml = r#"<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"><channel><item>
        <title>Episode</title>
        <itunes:duration>12:34</itunes:duration>
        <itunes:image href="http://a/cover.jpg"/>
        <itunes:author>Host</itunes:author>
        <itunes:episode>7</itunes:episode>
        <enclosure url="http://a/b.mp3" type="audio/mpeg" length="1"/>
    </item></channel></rss>"#;
    let feed = parser::parse(xml.as_bytes()).unwrap();
    let media = &feed.entries[0].media;
    let itunes = media.iter().find(|m| m.duration.is_some()).expect("iTunes media object");
    assert_eq!(itunes.duration, Some(std::time::Duration::from_secs(12 * 60 + 34)));
    assert_eq!(itunes.thumbnails[0].image.uri, "http://a/cover.jpg");
    assert_eq!(itunes.credits[0].entity, "Host");
    assert_eq!(itunes.episode.as_ref().unwrap().number, 7.0);
    // The enclosure is still reported
    assert!(
        media
            .iter()
            .any(|m| m.content.iter().any(|c| c.url.as_ref().unwrap().as_str() == "http://a/b.mp3"))
    );
}

// Regression (B3): RSS without a version attribute, or with an unexpected one, was rejected
#[test]
fn rss_version_is_lenient() {
    use crate::model::FeedType;
    for (root, expected) in [
        (r#"<rss>"#, FeedType::RSS2),
        (r#"<rss version="2.01">"#, FeedType::RSS2),
        (r#"<rss version=" 2.0">"#, FeedType::RSS2),
        (r#"<rss version="2">"#, FeedType::RSS2),
        (r#"<rss version="0.91">"#, FeedType::RSS0),
        (r#"<rss version="0.93">"#, FeedType::RSS0),
        (r#"<rss version="0.94">"#, FeedType::RSS0),
    ] {
        let xml = format!(r#"{root}<channel><title>t</title></channel></rss>"#);
        let feed = parser::parse(xml.as_bytes()).unwrap_or_else(|e| panic!("{root}: {e}"));
        assert_eq!(feed.feed_type, expected, "{root}");
        assert_eq!(feed.title.unwrap().content, "t");
    }
}

// Regression (B4): XML errors while reading the root element were reported as "no root element"
#[test]
fn root_xml_errors_are_reported() {
    let xml = "<?xml version=\"1.0\"?>\n<rss version=\"2.0\" <channel></channel></rss>";
    match parser::parse(xml.as_bytes()) {
        Err(ParseFeedError::XmlReader(_)) => {}
        other => panic!("expected an XML error, got {:?}", other),
    }

    // A well-formed document with an unknown root is still NoFeedRoot
    match parser::parse("<html><body/></html>".as_bytes()) {
        Err(ParseFeedError::ParseError(ParseErrorKind::NoFeedRoot)) => {}
        other => panic!("expected NoFeedRoot, got {:?}", other),
    }
}

// Regression (B5): only the first buffer (8 KiB) was examined when detecting XML vs JSON
#[test]
fn large_leading_whitespace_is_skipped() {
    let mut xml = " \n".repeat(10_000);
    xml.push_str(r#"<rss version="2.0"><channel><title>t</title></channel></rss>"#);
    assert_eq!(parser::parse(xml.as_bytes()).unwrap().title.unwrap().content, "t");

    let mut json = "\n".repeat(20_000);
    json.push_str(r#"{"version":"https://jsonfeed.org/version/1.1","title":"j","items":[]}"#);
    assert_eq!(parser::parse(json.as_bytes()).unwrap().title.unwrap().content, "j");

    assert!(parser::parse(" ".repeat(20_000).as_bytes()).is_err());
}
