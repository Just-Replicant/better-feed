//! Property tests: the lenient helpers and the parser must never panic, whatever the input (Q5).

use std::time::Duration;

use proptest::prelude::*;

use crate::model::Person;
use crate::parser;
use crate::parser::util::{parse_npt, parse_timestamp_lenient};

// Builds small XML-ish documents out of feed vocabulary so the generator reaches the format-specific parsers
fn feedish_xml() -> impl Strategy<Value = String> {
    let token = prop_oneof![
        Just("<rss version=\"2.0\">".to_string()),
        Just("<channel>".to_string()),
        Just("</channel>".to_string()),
        Just("<item>".to_string()),
        Just("</item>".to_string()),
        Just("<feed xmlns=\"http://www.w3.org/2005/Atom\">".to_string()),
        Just("<entry>".to_string()),
        Just("</entry>".to_string()),
        Just("<title>".to_string()),
        Just("</title>".to_string()),
        Just("<author>".to_string()),
        Just("</author>".to_string()),
        Just("<pubDate>".to_string()),
        Just("</pubDate>".to_string()),
        Just("<itunes:duration xmlns:itunes=\"http://www.itunes.com/dtds/podcast-1.0.dtd\">".to_string()),
        Just("</itunes:duration>".to_string()),
        Just("<media:content xmlns:media=\"http://search.yahoo.com/mrss/\" url=\"http://a/b\" duration=\"".to_string()),
        Just("\"/>".to_string()),
        Just("<enclosure url=\"x\" length=\"-1\"/>".to_string()),
        Just("<x:y xmlns:x=\"urn:x\" a=\"1\">".to_string()),
        Just("</x:y>".to_string()),
        Just("<![CDATA[".to_string()),
        Just("]]>".to_string()),
        Just("&amp;".to_string()),
        "[ -~]{0,12}",
        "\\PC{0,6}",
    ];
    prop::collection::vec(token, 0..40).prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn parse_npt_never_panics(s in "\\PC*") {
        let _ = parse_npt(&s);
    }

    #[test]
    fn parse_npt_round_trips_hms(h in 0u64..10_000, m in 0u64..60, sec in 0u64..60) {
        prop_assert_eq!(parse_npt(&format!("{h}:{m:02}:{sec:02}")), Some(Duration::from_secs(h * 3600 + m * 60 + sec)));
        prop_assert_eq!(parse_npt(&format!("{m}:{sec:02}")), Some(Duration::from_secs(m * 60 + sec)));
    }

    #[test]
    fn person_parse_never_panics(s in "\\PC*") {
        let _ = Person::parse(&s);
    }

    #[test]
    fn person_parse_with_email_never_panics(pre in "\\PC{0,8}", post in "\\PC{0,8}") {
        // The address is found whatever (possibly multi-byte) text surrounds it, as long as it is delimited
        prop_assume!(!pre.contains('@') && !post.contains('@'));
        let person = Person::parse(&format!("{pre} <someone@example.com> {post}"));
        prop_assert_eq!(person.email.as_deref(), Some("someone@example.com"));
    }

    #[test]
    fn timestamp_parse_never_panics(s in "\\PC*") {
        let _ = parse_timestamp_lenient(&s);
    }

    #[test]
    fn parse_arbitrary_bytes_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = parser::parse(bytes.as_slice());
    }

    #[test]
    fn parse_feedish_xml_never_panics(xml in feedish_xml()) {
        let _ = parser::parse(xml.as_bytes());
        let _ = parser::parse(format!("<rss version=\"2.0\"><channel>{xml}</channel></rss>").as_bytes());
    }

    #[test]
    fn parse_jsonish_never_panics(title in "\\PC{0,10}", id in prop_oneof![Just("1".to_string()), Just("\"a\"".to_string()), Just("null".to_string()), Just("[]".to_string())], url in "[ -~]{0,12}") {
        let json = format!(r#"{{"version":"https://jsonfeed.org/version/1.1","title":{},"items":[{{"id":{id},"url":{}}}]}}"#,
            serde_json::to_string(&title).unwrap(), serde_json::to_string(&url).unwrap());
        let _ = parser::parse(json.as_bytes());
    }
}
