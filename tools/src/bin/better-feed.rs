//! Command line front end for the better-feed parser.
//!
//! ```text
//! better-feed parse [OPTIONS] <SOURCE>    parse a file, http(s) URL or '-' (stdin)
//! better-feed check                       fetch and parse URLs read from stdin, one per line
//! ```

use std::io;
use std::ops::ControlFlow;
use std::process::ExitCode;

use better_feed::model::{Entry, Feed};
use better_feed::parser::{Builder, ParseLimits};
use better_feed::writer;

const USAGE: &str = "\
Usage:
  better-feed parse [OPTIONS] <SOURCE>   Parse a feed from a file, an http(s) URL or '-' (stdin)
  better-feed check                      Fetch and parse URLs read from stdin (one per line)
  better-feed help                       Show this help

Parse output (default: a short summary):
  --json            The parsed model as JSON
  --pretty          Indent JSON output
  --atom            Re-serialise as Atom 1.0
  --json-feed       Re-serialise as JSON Feed 1.1
  --stream          Print each entry as soon as it is parsed (summary output only)

Parse options:
  --no-extensions   Do not capture unmapped elements
  --no-sanitize     Do not sanitise HTML (only relevant with the 'sanitize' feature)
  --untrusted       Apply ParseLimits::untrusted() (input size, extension depth/size)
  --base <URI>      Resolve relative links against URI (defaults to the source URL)";

#[derive(Debug, Default, PartialEq)]
enum Output {
    #[default]
    Summary,
    Json,
    Atom,
    JsonFeed,
}

#[derive(Debug, Default, PartialEq)]
struct ParseArgs {
    source: String,
    output: Output,
    pretty: bool,
    stream: bool,
    no_extensions: bool,
    no_sanitize: bool,
    untrusted: bool,
    base: Option<String>,
}

fn parse_args(args: &[String]) -> Result<ParseArgs, String> {
    let mut parsed = ParseArgs::default();
    let mut source = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => parsed.output = Output::Json,
            "--atom" => parsed.output = Output::Atom,
            "--json-feed" => parsed.output = Output::JsonFeed,
            "--pretty" => parsed.pretty = true,
            "--stream" => parsed.stream = true,
            "--no-extensions" => parsed.no_extensions = true,
            "--no-sanitize" => parsed.no_sanitize = true,
            "--untrusted" => parsed.untrusted = true,
            "--base" => parsed.base = Some(args.next().ok_or("--base needs a URI")?.clone()),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            value => {
                if source.replace(value.to_string()).is_some() {
                    return Err("only one source can be given".into());
                }
            }
        }
    }
    parsed.source = source.ok_or("missing <SOURCE>")?;
    if parsed.stream && parsed.output != Output::Summary {
        return Err("--stream only works with the summary output".into());
    }
    Ok(parsed)
}

fn entry_line(entry: &Entry) -> String {
    let date = entry
        .published
        .or(entry.updated)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "----------".into());
    let title = entry.title.as_ref().map(|t| t.content.as_str()).unwrap_or("(untitled)");
    let link = entry.links.first().map(|l| l.href.as_str()).unwrap_or("");
    format!("  {date}  {title}  {link}")
}

fn summary_header(feed: &Feed) -> String {
    let title = feed.title.as_ref().map(|t| t.content.as_str()).unwrap_or("(untitled)");
    let link = feed.links.first().map(|l| l.href.as_str()).unwrap_or("");
    format!("{:?}: {title}  {link}", feed.feed_type)
}

fn run_parse(args: ParseArgs) -> Result<(), String> {
    let data = tools::read_source(&args.source)?;

    let base = args.base.clone().or_else(|| tools::is_url(&args.source).then(|| args.source.clone()));
    let mut builder = Builder::new()
        .base_uri(base)
        .capture_extensions(!args.no_extensions)
        .sanitize_content(!args.no_sanitize);
    if args.untrusted {
        builder = builder.limits(ParseLimits::untrusted());
    }
    let parser = builder.build();

    if args.stream {
        let mut count = 0usize;
        let feed = parser
            .parse_with(data.as_slice(), |entry| {
                count += 1;
                println!("{}", entry_line(&entry));
                ControlFlow::Continue(())
            })
            .map_err(|e| e.to_string())?;
        println!("{}\n{count} entries", summary_header(&feed));
        return Ok(());
    }

    let feed = parser.parse(data.as_slice()).map_err(|e| e.to_string())?;
    let json = |r: serde_json::Result<String>| r.map_err(|e| e.to_string());
    let out = match args.output {
        Output::Summary => {
            let mut lines = vec![summary_header(&feed), format!("{} entries", feed.entries.len())];
            lines.extend(feed.entries.iter().map(entry_line));
            lines.join("\n")
        }
        Output::Json if args.pretty => json(feed.to_json_pretty())?,
        Output::Json => json(feed.to_json())?,
        Output::Atom => writer::to_atom(&feed),
        Output::JsonFeed if args.pretty => json(writer::to_json_feed_pretty(&feed))?,
        Output::JsonFeed => json(writer::to_json_feed(&feed))?,
    };
    println!("{out}");
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("parse") => match parse_args(&args[1..]).and_then(run_parse) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
        Some("check") => tools::check_urls(io::stdin().lock()),
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn parses_arguments() {
        let parsed = parse_args(&args("feed.xml --json --pretty --no-extensions --base https://a/")).unwrap();
        assert_eq!(
            parsed,
            ParseArgs {
                source: "feed.xml".into(),
                output: Output::Json,
                pretty: true,
                no_extensions: true,
                base: Some("https://a/".into()),
                ..Default::default()
            }
        );
        assert!(parse_args(&args("")).is_err());
        assert!(parse_args(&args("a b")).is_err());
        assert!(parse_args(&args("a --bogus")).is_err());
        assert!(parse_args(&args("a --base")).is_err());
        assert!(parse_args(&args("a --stream --atom")).is_err());
    }
}
