//! Feeds arbitrary bytes to a parser configured for untrusted input (limits, sanitised extensions); any panic is a bug.
#![no_main]

use better_feed::parser::{Builder, ParseLimits};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let parser = Builder::new().limits(ParseLimits::untrusted()).sanitize_extensions(true).build();
    if let Ok(feed) = parser.parse(data) {
        // Serialisation of whatever was parsed must also succeed
        feed.to_json().expect("parsed feeds serialise to JSON");
    }
});
