use std::io::Read;

use mediatype::{MediaTypeBuf, names};

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use url::Url;

use crate::model::{Category, Content, Entry, Feed, FeedType, Image, Link, MediaObject, MediaThumbnail, Person, Text};
use crate::parser::extension::{CaptureConfig, from_json};
use crate::parser::util::{self, if_some_then};
use crate::parser::{EntrySink, ParseFeedError, ParseFeedResult, Parser};

#[cfg(test)]
mod tests;

/// Parses a JSON feed into our model
pub(crate) fn parse<R: Read>(parser: &Parser, stream: R, sink: &mut EntrySink) -> ParseFeedResult<Feed> {
    let parsed = serde_json::from_reader(stream);
    if let Ok(json_feed) = parsed {
        convert(parser, json_feed, sink)
    } else {
        // Unable to parse the JSON
        Err(ParseFeedError::JsonSerde(parsed.err().unwrap()))
    }
}

// Convert the JSON Feed into our standard model
fn convert(parser: &Parser, jf: JsonFeed, sink: &mut EntrySink) -> ParseFeedResult<Feed> {
    let mut feed = Feed::new(FeedType::JSON);

    // If the version exists, it should be something we support
    if !jf.version.starts_with("https://jsonfeed.org/version/1") {
        return ParseFeedResult::Err(ParseFeedError::JsonUnsupportedVersion(jf.version));
    }

    // Relative URLs are resolved against the configured base URI, or failing that the (absolute) feed_url
    let base = parser
        .base_uri
        .as_deref()
        .and_then(|b| Url::parse(b).ok())
        .or_else(|| jf.feed_url.as_deref().and_then(|f| Url::parse(f).ok()));
    let ctx = Ctx {
        parser,
        base: base.as_ref(),
        capture: parser.capture_config(),
    };

    // Convert feed level fields
    feed.title = Some(Text::new(jf.title));

    if_some_then(jf.home_page_url, |uri| feed.links.push(ctx.link(uri, Some("alternate"))));

    if_some_then(jf.feed_url, |uri| feed.links.push(ctx.link(uri, Some("self"))));

    if_some_then(jf.next_url, |uri| feed.links.push(ctx.link(uri, Some("next"))));

    if_some_then(jf.hubs, |hubs| {
        hubs.into_iter().for_each(|hub| {
            let mut link = ctx.link(hub.url, Some("hub"));
            link.media_type = hub.hub_type;
            feed.links.push(link);
        })
    });

    let mut extensions = jf.extensions;
    feed.expired = take_expired(&mut extensions);

    if_some_then(jf.description, |text| feed.description = Some(Text::new(text)));

    if_some_then(jf.icon, |uri| feed.logo = Some(Image::new(ctx.uri(uri))));

    if_some_then(jf.language, |language| feed.language = Some(language));

    if_some_then(jf.favicon, |uri| feed.icon = Some(Image::new(ctx.uri(uri))));

    handle_authors(&ctx, &mut feed.authors, &jf.author, &jf.authors);

    feed.extensions = from_json(extensions, &ctx.capture);

    // Convert items within the JSON feed
    for ji in jf.items {
        let mut entry = handle_item(&ctx, ji);

        // Per the spec, any items without an author inherit the feed
        if entry.authors.is_empty() {
            entry.authors.clone_from(&feed.authors);
        }

        if sink.push(&mut feed, entry).is_break() {
            break;
        }
    }

    Ok(feed)
}

// Shared state for the conversion
struct Ctx<'a> {
    parser: &'a Parser,
    base: Option<&'a Url>,
    capture: CaptureConfig,
}

impl Ctx<'_> {
    // Resolves a (potentially relative) URL, leaving it untouched if it cannot be resolved
    fn uri(&self, uri: String) -> String {
        match util::parse_uri(uri.trim(), self.base) {
            Some(url) => url.to_string(),
            None => uri,
        }
    }

    fn link(&self, uri: String, rel: Option<&str>) -> Link {
        let mut link = Link::new(uri, self.base);
        link.rel = rel.map(str::to_string);
        link
    }
}

fn accumulate_author(ctx: &Ctx, authors: &mut Vec<Person>, ja: &JsonAuthor) {
    // Only add if we haven't already seen this person
    if let Some(name) = &ja.name {
        if !authors.iter().any(|a| a.name.is_some() && a.name.as_ref().unwrap() == name) {
            let mut person = Person::parse(name).role("author");
            person.uri = ja.url.clone().map(|url| ctx.uri(url));

            authors.push(person);
        }
    }
}

// Handles an attachment
fn handle_attachment(ctx: &Ctx, attachment: JsonAttachment) -> Link {
    let mut link = ctx.link(attachment.url, Some("enclosure"));

    link.media_type = Some(attachment.mime_type);
    link.title = attachment.title;
    link.length = attachment.size_in_bytes;

    link
}

// Converts author / authors objects into our model
fn handle_authors(ctx: &Ctx, accumulated: &mut Vec<Person>, author: &Option<JsonAuthor>, authors: &Option<Vec<JsonAuthor>>) {
    if let Some(ja) = author {
        accumulate_author(ctx, accumulated, ja);
    }

    if let Some(jas) = authors {
        for ja in jas {
            accumulate_author(ctx, accumulated, ja);
        }
    }
}

// Handles HTML or plain text content
fn handle_content(content: Option<String>, content_type: MediaTypeBuf) -> Option<Content> {
    content.map(|body| Content {
        length: Some(body.len() as u64),
        body: Some(body.trim().into()),
        content_type,
        ..Default::default()
    })
}

// Converts a JSON feed item into our model
fn handle_item(ctx: &Ctx, ji: JsonItem) -> Entry {
    let parser = ctx.parser;
    let mut entry = Entry {
        id: ji.id.unwrap_or_default(),
        ..Default::default()
    };

    if_some_then(ji.url, |uri| entry.links.push(ctx.link(uri, Some("alternate"))));

    if_some_then(ji.external_url, |uri| entry.links.push(ctx.link(uri, Some("related"))));

    if_some_then(ji.language, |language| entry.language = Some(language));

    // The item image (and banner image) are exposed as thumbnails of a media object
    let thumbnails = [ji.image, ji.banner_image]
        .into_iter()
        .flatten()
        .map(|uri| MediaThumbnail::new(Image::new(ctx.uri(uri))))
        .collect::<Vec<_>>();
    if !thumbnails.is_empty() {
        entry.media.push(MediaObject {
            thumbnails,
            ..Default::default()
        });
    }

    if_some_then(ji.title, |text| entry.title = Some(Text::new(text)));

    // Content HTML, content text and summary are mapped across to our model with the preference toward HTML and explicit summary fields
    entry.content = handle_content(ji.content_html, MediaTypeBuf::new(names::TEXT, names::HTML));
    entry.summary = ji.summary.map(Text::new);
    if let Some(content_text) = handle_content(ji.content_text, MediaTypeBuf::new(names::TEXT, names::PLAIN)) {
        // If we don't have HTML content, use the text content as the entry content
        // otherwise, if the summary was not provided, we push the text there

        if entry.content.is_none() {
            entry.content = Some(content_text);
        } else if entry.summary.is_none() {
            entry.summary = content_text.body.map(Text::new);
        }
    }

    if_some_then(ji.date_published, |published| entry.published = parser.parse_timestamp(&published));

    if_some_then(ji.date_modified, |modified| entry.updated = parser.parse_timestamp(&modified));

    handle_authors(ctx, &mut entry.authors, &ji.author, &ji.authors);

    if_some_then(ji.tags, |tags| {
        tags.into_iter().map(|t| Category::new(&t)).for_each(|category| entry.categories.push(category))
    });

    if_some_then(ji.attachments, |attachments| {
        attachments
            .into_iter()
            .map(|a| handle_attachment(ctx, a))
            .for_each(|link| entry.links.push(link))
    });

    entry.extensions = from_json(ji.extensions, &ctx.capture);

    entry
}

#[derive(Debug, Deserialize)]
struct JsonFeed {
    pub version: String,
    pub title: String,
    pub home_page_url: Option<String>,
    pub feed_url: Option<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub favicon: Option<String>,
    pub author: Option<JsonAuthor>,
    pub authors: Option<Vec<JsonAuthor>>,
    pub next_url: Option<String>,
    pub hubs: Option<Vec<JsonHub>>,
    pub items: Vec<JsonItem>,
    #[serde(default, flatten)]
    extensions: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct JsonAttachment {
    url: String,
    mime_type: String,
    title: Option<String>,
    size_in_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct JsonHub {
    #[serde(rename = "type")]
    hub_type: Option<String>,
    url: String,
}

#[derive(Debug, Deserialize)]
struct JsonAuthor {
    name: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonItem {
    // The spec requires a string, but numbers are common in the wild
    #[serde(default, deserialize_with = "string_or_number")]
    pub id: Option<String>,
    pub url: Option<String>,
    pub external_url: Option<String>,
    pub title: Option<String>,
    pub image: Option<String>,
    pub banner_image: Option<String>,
    pub language: Option<String>,
    pub content_html: Option<String>,
    pub content_text: Option<String>,
    pub summary: Option<String>,
    pub date_published: Option<String>,
    pub date_modified: Option<String>,
    pub author: Option<JsonAuthor>,
    pub authors: Option<Vec<JsonAuthor>>,
    pub tags: Option<Vec<String>>,
    pub attachments: Option<Vec<JsonAttachment>>,
    #[serde(default, flatten)]
    extensions: Map<String, Value>,
}

// JSON Feed `expired` is a boolean. A recognised value is mapped onto the model; anything else is put back
// so it stays an extension and one odd value cannot fail the feed.
fn take_expired(extensions: &mut Map<String, Value>) -> Option<bool> {
    let value = extensions.remove("expired")?;
    let flag = match &value {
        Value::Bool(flag) => Some(*flag),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" => Some(true),
            "false" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    };
    match flag {
        Some(flag) => Some(flag),
        None => {
            extensions.insert("expired".into(), value);
            None
        }
    }
}

// Accepts a string or a number (converted to its string form) for an optional field
fn string_or_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(deserializer)? {
        Some(Value::String(s)) => Some(s),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(Value::Null) | None => None,
        Some(other) => return Err(serde::de::Error::custom(format!("invalid item id: {other}"))),
    })
}
