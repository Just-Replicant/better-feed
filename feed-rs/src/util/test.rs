use std::fs;
use std::path::PathBuf;

use crate::model::Feed;

/// Copies unknown tags onto a hand-written expectation.
///
/// Those tags are locked by the serde fixtures, so mapped-field tests can stay focused on the fields they name.
pub(crate) fn copy_extensions(actual: &Feed, expected: &mut Feed) {
    expected.extensions.clone_from(&actual.extensions);
    for (expected_entry, actual_entry) in expected.entries.iter_mut().zip(actual.entries.iter()) {
        expected_entry.extensions.clone_from(&actual_entry.extensions);
    }
}

/// Load a test file as bytes
pub(crate) fn fixture_as_raw(resource: &str) -> Vec<u8> {
    let path = fixture_filename(resource);
    let mut bytes = fs::read(path).unwrap();
    // Working copies on Windows are CRLF. The fixtures were written for LF.
    bytes.retain(|byte| *byte != b'\r');
    bytes
}

/// Load a test file and return it as a String
pub(crate) fn fixture_as_string(resource: &str) -> String {
    String::from_utf8(fixture_as_raw(resource)).unwrap()
}

/// Return the path to our test data directory
pub(crate) fn fixture_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("fixture");
    dir
}

/// Return the path to a file within the test data directory
pub(crate) fn fixture_filename(filename: &str) -> String {
    let mut dir = fixture_dir();
    dir.push(filename);
    dir.to_str().unwrap().to_owned()
}
