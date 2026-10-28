//! Reads feed URLs from stdin (one per line), fetches and parses each one, and prints a summary.
//!
//! Blank lines and lines starting with `#` are ignored. A failure for one URL (network error, HTTP error status or
//! parse error) is reported and the run continues; the process exits with status 1 if any URL failed.
//! Equivalent to `better-feed check`.

use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    tools::check_urls(io::stdin().lock())
}
