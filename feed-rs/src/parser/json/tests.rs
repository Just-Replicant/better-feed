use crate::model::{Content, Entry, Feed, FeedType, Image, Link, Person, Text};
use crate::parser;
use crate::util::test;

// Verify we can parse a more complete feed
#[test]
fn test_example_1() {
    // Parse the feed
    let test_data = test::fixture_as_string("jsonfeed/jsonfeed_example_1.json");
    let actual = parser::parse(test_data.as_bytes()).unwrap();

    // Expected feed
    let mut expected = Feed::new(FeedType::JSON)
        .id(&actual.id)                 // not in test content
        .updated(actual.updated)        // not in test content
        .title(Text::new("Daring Fireball".into()))
        .link(Link::new("https://daringfireball.net/", None).rel("alternate"))
        .link(Link::new("https://daringfireball.net/feeds/json", None).rel("self"))
        .author(Person::parse("John Gruber")
            .role("author")
            .uri("https://twitter.com/gruber"))
        .logo(Image::new("https://daringfireball.net/graphics/apple-touch-icon.png".into()))
        .icon(Image::new("https://daringfireball.net/graphics/favicon-64.png".into()))
        .entry(Entry::default()
            .title(Text::new("How Jeff Bezos’s iPhone X Was Hacked".into()))
            .published("2020-01-24T23:46:57Z")
            .updated_parsed("2020-01-24T23:46:57Z")
            .id("https://daringfireball.net/linked/2020/01/24/bezos-iphone-x")
            .link(Link::new("https://daringfireball.net/linked/2020/01/24/bezos-iphone-x", None).rel("alternate"))
            .link(Link::new("https://www.nytimes.com/2020/01/22/technology/jeff-bezos-hack-iphone.html", None).rel("related"))
            .author(Person::parse("John Gruber").role("author"))
            .content(Content::default()
                .body(r#"<p>Good summary from The New York Times. Until this week’s news, I don’t believe we knew what type of phone Bezos was using when he was hacked. Now we know: an iPhone X.</p>"#)
                .content_type("text/html")
                .length(177)))
        .entry(Entry::default()
            .title(Text::new("Instagram for Windows 95".into()))
            .published("2020-01-21T01:07:00Z")
            .updated_parsed("2020-01-21T20:58:36Z")
            .id("https://daringfireball.net/linked/2020/01/20/instagram-for-win95")
            .link(Link::new("https://daringfireball.net/linked/2020/01/20/instagram-for-win95", None).rel("alternate"))
            .link(Link::new("https://www.behance.net/gallery/41023081/Instagram-for-Win95?utm_source=morning_brew", None).rel("related"))
            .author(Person::parse("John Gruber").role("author"))
            .content(Content::default()
                .body(r#"<p>Delightful work by Petrick Studio.</p>"#)
                .content_type("text/html")
                .length(41)));

    test::copy_extensions(&actual, &mut expected);
    assert_eq!(actual, expected);
}

// Verify we can parse the specification example
#[test]
fn test_spec_1() {
    // Parse the feed; note that the result with sanitization active differs from the expected,
    // so we will explicitly disable sanitization for this test.
    let test_data = test::fixture_as_string("jsonfeed/jsonfeed_spec_1.json");
    let p = parser::Builder::new().sanitize_content(false).build();
    let actual = p.parse(test_data.as_bytes()).unwrap();

    // Expected feed
    let mut expected = Feed::new(FeedType::JSON)
        .id(&actual.id)                 // not in test content
        .updated(actual.updated)        // not in test content
        .title(Text::new("JSON Feed".into()))
        .description(Text::new("JSON Feed is a pragmatic syndication format for blogs, microblogs, and other time-based content.".into()))
        .link(Link::new("https://jsonfeed.org/", None).rel("alternate"))
        .link(Link::new("https://jsonfeed.org/feed.json", None).rel("self"))
        .author(Person::parse("Brent Simmons and Manton Reece")
            .role("author")
            .uri("https://jsonfeed.org/"))
        .entry(Entry::default()
            .updated(actual.entries[0].updated)             // not in test content
            .id("https://jsonfeed.org/2017/05/17/announcing_json_feed")
            .title(Text::new("Announcing JSON Feed".into()))
            .published("2017-05-17T08:02:12-07:00")
            .link(Link::new("https://jsonfeed.org/2017/05/17/announcing_json_feed", None).rel("alternate"))
            .author(Person::parse("Brent Simmons and Manton Reece")
                .role("author")
                .uri("https://jsonfeed.org/"))
            .content(Content::default()
                .body(r#"<p>We — Manton Reece and Brent Simmons — have noticed that JSON has become the developers’ choice for APIs, and that developers will often go out of their way to avoid XML. JSON is simpler to read and write, and it’s less prone to bugs.</p>
<p>So we developed JSON Feed, a format similar to <a href="http://cyber.harvard.edu/rss/rss.html">RSS</a> and <a href="https://tools.ietf.org/html/rfc4287">Atom</a> but in JSON. It reflects the lessons learned from our years of work reading and publishing feeds.</p>..."#)
                .content_type("text/html")
                .length(518)));

    test::copy_extensions(&actual, &mut expected);
    assert_eq!(actual, expected);
}

// Even though the spec is clear its mandatory, we still have feeds that do not supply the ID
#[test]
fn test_optional_entry_id() {
    // Parse the feed
    let test_data = test::fixture_as_string("jsonfeed/jsonfeed_elastic_1.1.json");
    let actual = parser::parse(test_data.as_bytes()).unwrap();

    assert!(!actual.id.is_empty());
    for entry in actual.entries {
        assert!(!entry.id.is_empty());
    }
}

// Verify we can parse the v1.1 specific payload
#[test]
fn test_elastic_v1_1() {
    // Parse the feed
    let test_data = test::fixture_as_string("jsonfeed/jsonfeed_elastic_1.1.json");
    let actual = parser::parse(test_data.as_bytes()).unwrap();

    // Check language was extracted correctly for the feed
    assert_eq!("en-US", actual.language.unwrap());

    // Check feed authors (should combine both deprecated and new fields)
    assert_eq!(
        actual.authors,
        vec!(Person::parse("Fake Author 3").role("author"), Person::parse("Fake Author 4").role("author"))
    );

    // Check first item - combine author + authors
    let mut entries = actual.entries.iter();
    let entry = entries.next().unwrap();
    let chris = Person::empty()
        .name("Chris Churilo")
        .role("author")
        .uri("https://www.influxdata.com/blog/author/chrisc/");
    assert_eq!(
        entry.authors,
        vec!(
            Person::parse("Fake Author 2").role("author"),
            chris.clone(),
            Person::parse("Fake Author 1").role("author")
        )
    );

    // Second item migrates from old author to new authors field
    let entry = entries.next().unwrap();
    assert_eq!(entry.authors, vec!(chris.clone()));

    // Third item inherits feed authors (per the spec)
    let entry = entries.next().unwrap();
    assert_eq!(
        entry.authors,
        vec!(Person::parse("Fake Author 3").role("author"), Person::parse("Fake Author 4").role("author"))
    );
}

#[test]
fn test_unknown_keys_are_kept() {
    let json = r#"{
      "version": "https://jsonfeed.org/version/1.1",
      "title": "Example",
      "user_comment": "hello",
      "items": [{ "id": "1", "image": "https://example.com/a.png" }]
    }"#;
    let feed = parser::parse(json.as_bytes()).unwrap();
    assert_eq!(feed.extensions[0].name, "user_comment");
    assert_eq!(feed.extensions[0].namespace.as_deref(), Some("https://jsonfeed.org"));
    assert_eq!(feed.extensions[0].text.as_deref(), Some("hello"));
    assert_eq!(feed.json_user_comment(), Some("hello"));
    // "image" is a known item field, so it is mapped into the model rather than kept as an extension
    assert!(feed.entries[0].extensions.is_empty());
    assert_eq!(feed.entries[0].media[0].thumbnails[0].image.uri, "https://example.com/a.png");
}

// Regression (B6): numeric ids failed the whole feed, relative URLs were not resolved and several
// spec fields (image, banner_image, language, next_url, hubs) were ignored
#[test]
fn test_spec_fields_ids_and_relative_urls() {
    let json = r#"{
      "version": "https://jsonfeed.org/version/1.1",
      "title": "Example",
      "home_page_url": "/",
      "feed_url": "https://example.com/blog/feed.json",
      "next_url": "feed.json?page=2",
      "icon": "icon.png",
      "hubs": [{ "type": "WebSub", "url": "https://hub.example.com/" }],
      "items": [
        { "id": 123, "url": "posts/1", "image": "img/1.png", "banner_image": "img/1-wide.png", "language": "fr-FR",
          "content_text": "hi", "attachments": [{ "url": "audio/1.mp3", "mime_type": "audio/mpeg" }] },
        { "id": 4.5, "content_text": "float" },
        { "id": null, "content_text": "null" }
      ]
    }"#;
    let feed = parser::parse(json.as_bytes()).unwrap();

    let links: Vec<_> = feed.links.iter().map(|l| (l.rel.as_deref(), l.href.as_str())).collect();
    assert_eq!(
        links,
        vec![
            (Some("alternate"), "https://example.com/"),
            (Some("self"), "https://example.com/blog/feed.json"),
            (Some("next"), "https://example.com/blog/feed.json?page=2"),
            (Some("hub"), "https://hub.example.com/"),
        ]
    );
    assert_eq!(feed.links[3].media_type.as_deref(), Some("WebSub"));
    assert_eq!(feed.logo.as_ref().unwrap().uri, "https://example.com/blog/icon.png");

    let entry = &feed.entries[0];
    assert_eq!(entry.id, "123");
    assert_eq!(entry.language.as_deref(), Some("fr-FR"));
    assert_eq!(entry.links[0].href, "https://example.com/blog/posts/1");
    assert_eq!(entry.links[1].href, "https://example.com/blog/audio/1.mp3");
    assert_eq!(entry.links[1].rel.as_deref(), Some("enclosure"));
    let thumbs: Vec<_> = entry.media[0].thumbnails.iter().map(|t| t.image.uri.as_str()).collect();
    assert_eq!(thumbs, vec!["https://example.com/blog/img/1.png", "https://example.com/blog/img/1-wide.png"]);
    assert!(entry.extensions.is_empty());

    assert_eq!(feed.entries[1].id, "4.5");
    // A null id gets a generated one
    assert!(!feed.entries[2].id.is_empty());

    let expired = r#"{"version":"https://jsonfeed.org/version/1.1","title":"t","expired":true,"user_comment":"raw note","items":[]}"#;
    let expired_feed = parser::parse(expired.as_bytes()).unwrap();
    assert_eq!(expired_feed.expired, Some(true));
    assert_eq!(expired_feed.json_user_comment(), Some("raw note"));
    assert!(expired_feed.extensions.iter().all(|extension| extension.name != "expired"));

    // A non-boolean expired value must not fail the feed, and must not be dropped
    let odd = r#"{"version":"https://jsonfeed.org/version/1.1","title":"t","expired":"maybe","items":[]}"#;
    let odd_feed = parser::parse(odd.as_bytes()).unwrap();
    assert_eq!(odd_feed.expired, None);
    assert_eq!(odd_feed.extension_text(Some(crate::model::ns::JSON_FEED), "expired"), Some("maybe"));

    // An explicit base URI takes precedence over feed_url
    let json = r#"{"version": "https://jsonfeed.org/version/1", "title": "t", "items": [{ "id": "1", "url": "a" }]}"#;
    let feed = parser::Builder::new()
        .base_uri(Some("https://other.example/x/"))
        .build()
        .parse(json.as_bytes())
        .unwrap();
    assert_eq!(feed.entries[0].links[0].href, "https://other.example/x/a");

    // Ids that are neither strings nor numbers are still an error
    let json = r#"{"version": "https://jsonfeed.org/version/1", "title": "t", "items": [{ "id": [1] }]}"#;
    assert!(parser::parse(json.as_bytes()).is_err());
}
