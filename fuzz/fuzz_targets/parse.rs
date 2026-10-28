//! Feeds arbitrary bytes to the default parser; any panic is a bug.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = better_feed::parser::parse(data);
});
