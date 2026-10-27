use std::fs;
use std::path::Path;

use better_feed::model::Feed;

// Tool to convert the base fixture data (e.g. XML) into JSON via serde for regression tests.
//
// The fixtures are loaded the same way as the `serde_regression` test in `feed-rs/src/parser/tests.rs`:
// carriage returns are stripped (Windows checkouts are CRLF) and sanitisation is disabled.
fn main() {
    let fixture_root_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../feed-rs/fixture");

    find_fixture_files(&fixture_root_dir, &mut |path| {
        // Only regenerate snapshots that already exist, matching the regression test
        let json_path = serde_path(path);
        if !json_path.exists() {
            return;
        }

        println!("Processing {}", path.display());
        let mut data = fs::read(path).unwrap();
        data.retain(|byte| *byte != b'\r');

        let parser = better_feed::parser::Builder::new().sanitize_content(false).build();
        match parser.parse(data.as_slice()) {
            Ok(mut feed) => {
                // Generated (random) IDs are replaced by the test, so keep the old ones to avoid churn
                if let Ok(old) = fs::read(&json_path) {
                    if let Ok(old) = serde_json::from_slice::<Feed>(&old) {
                        keep_generated_ids(&old, &mut feed);
                    }
                }
                let json = serde_json::to_string(&feed).unwrap();
                fs::write(json_path, json).unwrap();
            }
            Err(e) => eprintln!("  skipped, failed to parse: {e}"),
        }
    })
}

/// Copies random UUID ids from the previous snapshot when both old and new ids are generated UUIDs
fn keep_generated_ids(old: &Feed, new: &mut Feed) {
    if is_uuid(&old.id) && is_uuid(&new.id) {
        new.id.clone_from(&old.id);
    }
    for (old_entry, new_entry) in old.entries.iter().zip(new.entries.iter_mut()) {
        if is_uuid(&old_entry.id) && is_uuid(&new_entry.id) {
            new_entry.id.clone_from(&old_entry.id);
        }
    }
}

/// True for a hyphenated UUID such as `d6ff7b38-5ce5-4889-9e78-fc08ff7462bc`
fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices()
            .all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { c == '-' } else { c.is_ascii_hexdigit() })
}

/// Returns true if the file is a serialised snapshot rather than a source fixture
fn is_snapshot(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".serde.json")
}

/// The snapshot path for a source fixture
fn serde_path(path: &Path) -> std::path::PathBuf {
    path.with_extension("serde.json")
}

fn find_fixture_files(fixture_root: &Path, callback: &mut dyn FnMut(&Path)) {
    let mut entries = fs::read_dir(fixture_root).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            find_fixture_files(&path, callback);
        } else if !is_snapshot(&path) {
            let name = path.to_string_lossy();
            if name.ends_with(".xml") || name.ends_with(".json") {
                callback(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_files_are_recognised() {
        // Regression: `Path::ends_with` compares whole components so the old check never matched
        assert!(is_snapshot(Path::new("fixture/rss2/rss_2.0_bbc.serde.json")));
        assert!(!is_snapshot(Path::new("fixture/rss2/rss_2.0_bbc.xml")));
        assert!(!is_snapshot(Path::new("fixture/jsonfeed/jsonfeed_spec_1.json")));
        assert_eq!(serde_path(Path::new("a/b.xml")), Path::new("a/b.serde.json"));
        assert!(is_uuid("d6ff7b38-5ce5-4889-9e78-fc08ff7462bc"));
        assert!(!is_uuid("urn:uuid:1225c695-cfb8-4ebb-aaaa-80da344efa6a"));
    }
}
