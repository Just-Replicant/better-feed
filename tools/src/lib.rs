//! Shared helpers for the development tools and the `better-feed` command line.

use std::io::{self, BufRead, Read};
use std::process::ExitCode;
use std::time::Duration;

use better_feed::parser;

/// How long to wait for each URL before giving up
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// How much of a failing response body to print
pub const PREVIEW_BYTES: usize = 512;

/// A blocking HTTP client with a timeout and user agent
pub fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(concat!("better-feed-tools/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("unable to create HTTP client: {e}"))
}

/// Fetches a URL, failing on network errors and non-2xx statuses
pub fn fetch(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client.get(url).send().map_err(|e| format!("fetch error: {e}"))?;
    let status = response.status();
    let body = response.bytes().map_err(|e| format!("error reading body: {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}\n{}", preview(&body)));
    }
    Ok(body.to_vec())
}

/// Reads a source: an http(s) URL, `-` for stdin, or a file path
pub fn read_source(source: &str) -> Result<Vec<u8>, String> {
    if is_url(source) {
        fetch(&http_client()?, source)
    } else if source == "-" {
        let mut data = Vec::new();
        io::stdin().read_to_end(&mut data).map_err(|e| format!("unable to read stdin: {e}"))?;
        Ok(data)
    } else {
        std::fs::read(source).map_err(|e| format!("unable to read {source}: {e}"))
    }
}

/// True for http and https URLs
pub fn is_url(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Returns the URL on this line, or None for blank lines and comments
pub fn url_from_line(line: &str) -> Option<&str> {
    let line = line.trim();
    (!line.is_empty() && !line.starts_with('#')).then_some(line)
}

/// A short, printable preview of a response body
pub fn preview(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(&body[..body.len().min(PREVIEW_BYTES)]).into_owned();
    if body.len() > PREVIEW_BYTES {
        format!("{text}... ({} bytes total)", body.len())
    } else {
        text
    }
}

/// Fetches and parses every URL read from `input` (one per line), printing a line per URL and a summary.
/// Failures are reported and the run continues; the exit code is 1 if any URL failed.
pub fn check_urls(input: impl BufRead) -> ExitCode {
    let client = match http_client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let (mut ok, mut failed) = (0usize, Vec::new());
    for line in input.lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("unable to read input: {error}");
                return ExitCode::FAILURE;
            }
        };
        let Some(url) = url_from_line(&line) else { continue };

        print!("{url}  ... ");
        let result = fetch(&client, url).and_then(|body| {
            parser::Builder::new()
                .base_uri(Some(url))
                .build()
                .parse(body.as_slice())
                .map(|feed| feed.entries.len())
                .map_err(|e| format!("{e}\n{}", preview(&body)))
        });
        match result {
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

    #[test]
    fn sources() {
        assert!(is_url("https://a/"));
        assert!(is_url("HTTP://a/"));
        assert!(!is_url("feed.xml"));
        assert!(read_source("/definitely/not/here.xml").unwrap_err().contains("unable to read"));
    }
}
