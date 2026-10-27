use std::time::Duration;

use super::*;
use crate::parser;

const PODCAST_FEED: &str = r#"<rss version="2.0"
    xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
    xmlns:podcast="https://podcastindex.org/namespace/1.0"
    xmlns:sy="http://purl.org/rss/1.0/modules/syndication/"
    xmlns:slash="http://purl.org/rss/1.0/modules/slash/"
    xmlns:georss="http://www.georss.org/georss">
  <channel>
    <title>Show</title>
    <cloud domain="rpc.example.com" port="80" path="/RPC2" registerProcedure="pingMe" protocol="soap"/>
    <sy:updatePeriod> Daily </sy:updatePeriod>
    <sy:updateFrequency>2</sy:updateFrequency>
    <georss:point>45.256 -71.92</georss:point>
    <itunes:new-feed-url>https://new.example/feed</itunes:new-feed-url>
    <itunes:type>Serial</itunes:type>
    <itunes:block>no</itunes:block>
    <itunes:complete>Yes</itunes:complete>
    <podcast:guid>917393e3-1b1e-5cef-ace4-edaa54e1f810</podcast:guid>
    <podcast:locked owner="me@example.com">yes</podcast:locked>
    <podcast:funding url="https://donate.example/"> Support the show </podcast:funding>
    <podcast:funding url="https://patreon.example/"/>
    <podcast:funding>no url</podcast:funding>
    <podcast:value type="lightning" method="keysend" suggested="0.00000005000">
      <podcast:valueRecipient name="Host" type="node" address="02d5c1bf" split="90"/>
      <podcast:valueRecipient name="App" type="node" address="03ae9f91" split="10" fee="true"/>
      <podcast:valueRecipient name="Bad split" type="node" address="x" split="lots"/>
    </podcast:value>
    <item>
      <title>Episode</title>
      <source url="http://other.example/rss.xml"> Other feed </source>
      <slash:comments>42</slash:comments>
      <georss:point>1000 0</georss:point>
      <itunes:explicit>true</itunes:explicit>
      <itunes:episodeType>Trailer</itunes:episodeType>
      <itunes:block>Yes</itunes:block>
      <podcast:chapters url="https://a/ch.json" type="application/json+chapters"/>
      <podcast:soundbite startTime="73.0" duration="60.5">Best bit</podcast:soundbite>
      <podcast:soundbite startTime="-1" duration="10"/>
    </item>
    <item><title>Plain</title></item>
  </channel>
</rss>"#;

#[test]
fn feed_accessors() {
    let feed = parser::parse(PODCAST_FEED.as_bytes()).unwrap();

    // F1 lookups
    assert!(feed.extension(Some(ns::ITUNES), "type").is_some());
    assert_eq!(feed.extensions_named(Some(ns::PODCAST), "funding").count(), 3);
    assert_eq!(feed.extensions_in(Some(ns::SYNDICATION)).count(), 2);
    assert_eq!(feed.extensions_in(None).count(), 1); // <cloud>
    assert_eq!(feed.extension_text(Some(ns::SYNDICATION), "updatePeriod"), Some("Daily"));
    assert!(feed.extension(Some(ns::ATOM), "nothing").is_none());

    // F2 typed views
    assert_eq!(feed.update_period().as_deref(), Some("daily"));
    assert_eq!(feed.update_frequency(), Some(2));
    assert_eq!(feed.update_interval(), Some(Duration::from_secs(43_200)));
    assert_eq!(
        feed.geo_point(),
        Some(GeoPoint {
            latitude: 45.256,
            longitude: -71.92
        })
    );
    let cloud = feed.cloud().unwrap();
    assert_eq!(
        (cloud.domain.as_str(), cloud.port, cloud.protocol.as_deref()),
        ("rpc.example.com", Some(80), Some("soap"))
    );
    assert_eq!(feed.itunes_new_feed_url(), Some("https://new.example/feed"));
    assert_eq!(feed.itunes_type().as_deref(), Some("serial"));
    assert_eq!(feed.itunes_block(), Some(false));
    assert_eq!(feed.itunes_complete(), Some(true));
    assert_eq!(feed.podcast_guid(), Some("917393e3-1b1e-5cef-ace4-edaa54e1f810"));
    assert_eq!(
        feed.podcast_locked(),
        Some(Locked {
            locked: true,
            owner: Some("me@example.com".into())
        })
    );
    assert_eq!(
        feed.podcast_funding(),
        vec![
            Funding {
                url: "https://donate.example/".into(),
                message: Some("Support the show".into())
            },
            Funding {
                url: "https://patreon.example/".into(),
                message: None
            },
        ]
    );
    let value = &feed.podcast_value()[0];
    assert_eq!((value.value_type.as_str(), value.method.as_str()), ("lightning", "keysend"));
    assert_eq!(value.suggested.as_deref(), Some("0.00000005000"));
    let splits: Vec<_> = value.recipients.iter().map(|r| (r.name.as_deref(), r.split, r.fee)).collect();
    assert_eq!(splits, vec![(Some("Host"), 90, false), (Some("App"), 10, true)]);
}

#[test]
fn entry_accessors() {
    let feed = parser::parse(PODCAST_FEED.as_bytes()).unwrap();
    let entry = &feed.entries[0];

    assert_eq!(entry.comment_count(), Some(42));
    assert_eq!(entry.geo_point(), None); // out of range latitude
    assert_eq!(
        entry.rss_source(),
        Some(Source {
            url: "http://other.example/rss.xml".into(),
            title: Some("Other feed".into())
        })
    );
    assert_eq!(entry.itunes_explicit(), Some(true));
    assert_eq!(entry.itunes_episode_type().as_deref(), Some("trailer"));
    assert_eq!(entry.itunes_block(), Some(true));
    assert_eq!(
        entry.podcast_chapters(),
        Some(Chapters {
            url: "https://a/ch.json".into(),
            content_type: "application/json+chapters".into()
        })
    );
    assert_eq!(
        entry.podcast_soundbites(),
        vec![Soundbite {
            start: Duration::from_secs(73),
            duration: Duration::from_millis(60_500),
            title: Some("Best bit".into())
        }]
    );
    assert!(entry.podcast_value().is_empty());

    // Absent extensions are None / empty rather than errors
    let plain = &feed.entries[1];
    assert_eq!(plain.comment_count(), None);
    assert_eq!(plain.rss_source(), None);
    assert_eq!(plain.itunes_explicit(), None);
    assert!(plain.podcast_soundbites().is_empty());
}

#[test]
fn missing_frequency_defaults_to_once_per_period() {
    let xml = r#"<rss version="2.0" xmlns:sy="http://purl.org/rss/1.0/modules/syndication/"><channel><title>t</title>
        <sy:updatePeriod>hourly</sy:updatePeriod></channel></rss>"#;
    let feed = parser::parse(xml.as_bytes()).unwrap();
    assert_eq!(feed.update_frequency(), Some(1));
    assert_eq!(feed.update_interval(), Some(Duration::from_secs(3600)));

    let xml = r#"<rss version="2.0" xmlns:sy="http://purl.org/rss/1.0/modules/syndication/"><channel><title>t</title>
        <sy:updatePeriod>sometimes</sy:updatePeriod><sy:updateFrequency>0</sy:updateFrequency></channel></rss>"#;
    let feed = parser::parse(xml.as_bytes()).unwrap();
    assert_eq!(feed.update_interval(), None);
}

#[test]
fn json_feed_extensions_use_the_json_feed_namespace() {
    let json = r#"{"version":"https://jsonfeed.org/version/1.1","title":"t","_custom":{"a":1},"items":[]}"#;
    let feed = parser::parse(json.as_bytes()).unwrap();
    assert!(feed.extension(Some(ns::JSON_FEED), "_custom").is_some());
}
