//! Reads feed URLs from stdin (one per line), fetches and parses each one, and prints a summary.
//!
//! Blank lines and lines starting with `#` are ignored. A failure for one URL (network error, HTTP error status or
//! parse error) is reported and the run continues; the process exits with status 1 if any URL failed.

use std::io::{self, BufRead};
use std::process::ExitCode;
use std::time::Duration;

use better_feed::parser;

// How long to wait for each URL before giving up
const TIMEOUT: Duration = Duration::from_secs(30);

// How much of a failing response body to print
const PREVIEW_BYTES: usize = 512;

fn main() -> ExitCode {
    let client = match reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(concat!("better-feed-testurls/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("unable to create HTTP client: {error}");
            return ExitCode::FAILURE;
        }
    };

    let (mut ok, mut failed) = (0usize, Vec::new());
    for line in io::stdin().lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("unable to read stdin: {error}");
                return ExitCode::FAILURE;
            }
        };
        let Some(url) = url_from_line(&line) else { continue };

        print!("{url}  ... ");
        match check(&client, url) {
            Ok(entries) => {
                ok += 1;
                println!("ok ({entries} entries)");
            }
            Err(error) => {
                println!("failed: {error}\n{}", "-".repeat(61));
                failed.push(url.to_string());
            }
        }
    }

    println!("\n{} ok, {} failed", ok, failed.len());
    for url in &failed {
        println!("  failed: {url}");
    }

    if failed.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

// Fetches and parses one URL, returning the number of entries or a description of the failure
fn check(client: &reqwest::blocking::Client, url: &str) -> Result<usize, String> {
    let response = client.get(url).send().map_err(|e| format!("fetch error: {e}"))?;
    let status = response.status();
    let body = response.bytes().map_err(|e| format!("error reading body: {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}\n{}", preview(&body)));
    }

    let parser = parser::Builder::new().base_uri(Some(url)).build();
    parser
        .parse(body.as_ref())
        .map(|feed| feed.entries.len())
        .map_err(|e| format!("{e}\n{}", preview(&body)))
}

// Returns the URL on this line, or None for blank lines and comments
fn url_from_line(line: &str) -> Option<&str> {
    let line = line.trim();
    (!line.is_empty() && !line.starts_with('#')).then_some(line)
}

// A short, printable preview of a response body
fn preview(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(&body[..body.len().min(PREVIEW_BYTES)]).into_owned();
    if body.len() > PREVIEW_BYTES {
        format!("{text}... ({} bytes total)", body.len())
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_blank_lines_and_comments() {
        assert_eq!(url_from_line("  https://a/feed  "), Some("https://a/feed"));
        assert_eq!(url_from_line("   "), None);
        assert_eq!(url_from_line("# comment"), None);
    }

    #[test]
    fn preview_is_truncated() {
        assert_eq!(preview(b"short"), "short");
        let long = vec![b'x'; 2000];
        let p = preview(&long);
        assert!(p.starts_with(&"x".repeat(PREVIEW_BYTES)));
        assert!(p.ends_with("... (2000 bytes total)"));
        // Invalid UTF-8 (including a multi-byte char cut at the boundary) does not panic
        let mut bytes = vec![b'a'; PREVIEW_BYTES - 1];
        bytes.extend("é and more".as_bytes());
        preview(&bytes);
        preview(&[0xff, 0xfe]);
    }
}
