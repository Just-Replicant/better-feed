//! Checks that the public API can be used as intended from outside the crate (Q3).

use better_feed::model::{Entry, Feed, FeedType, Text};
use better_feed::parser::{self, ParseErrorKind, ParseFeedError};

#[test]
fn feeds_can_be_built_outside_the_crate() {
    let mut feed = Feed::new(FeedType::Atom);
    feed.title = Some(Text::new("built".into()));
    let mut entry = Entry::default();
    entry.id = "1".into();
    feed.entries.push(entry);

    let json = feed.to_json().unwrap();
    let back: Feed = serde_json::from_str(&json).unwrap();
    assert_eq!(back, feed);
}

#[test]
fn errors_can_be_matched_with_a_wildcard() {
    let err = parser::parse("<html/>".as_bytes()).unwrap_err();
    let described = match &err {
        ParseFeedError::ParseError(ParseErrorKind::NoFeedRoot) => "no root",
        ParseFeedError::ParseError(_) => "other parse error",
        _ => "other",
    };
    assert_eq!(described, "no root");
}
