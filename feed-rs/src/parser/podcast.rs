use crate::model::{Extension, Feed, Group, MediaObject, PodcastPerson, Role, Transcript};
use crate::parser::extension::capture;
use crate::parser::util::{if_some_then, parse_uri};
use crate::parser::{ParseFeedResult, common};
use crate::xml::{Element, NS};
use mediatype::MediaTypeBuf;
use std::io::BufRead;
use url::Url;

// Process <podcast> elements at channel level updating the Feed object as required
pub(crate) fn handle_podcast_channel_element<R: BufRead>(element: Element<R>, feed: &mut Feed, strict: bool) -> ParseFeedResult<()> {
    match element.ns_and_tag() {
        (NS::Podcast, "person") => if_some_then(handle_person(element, &mut feed.extensions, strict)?, |person| feed.people.push(person)),
        _ => feed.extensions.extend(capture(element)?),
    }

    Ok(())
}

/// Process <podcast> elements at entry level updating the MediaObject as required
pub(crate) fn handle_podcast_item_element<R: BufRead>(
    element: Element<R>,
    media_obj: &mut MediaObject,
    extensions: &mut Vec<Extension>,
    strict: bool,
) -> ParseFeedResult<()> {
    match element.ns_and_tag() {
        (NS::Podcast, "person") => if_some_then(handle_person(element, extensions, strict)?, |person| media_obj.people.push(person)),

        (NS::Podcast, "season") => if_some_then(common::handle_season(element), |season| media_obj.season = Some(season)),

        (NS::Podcast, "episode") => if_some_then(common::handle_episode(element), |episode| media_obj.episode = Some(episode)),

        (NS::Podcast, "transcript") => if_some_then(handle_transcript(element), |transcript| media_obj.transcripts.push(transcript)),

        _ => extensions.extend(capture(element)?),
    }

    Ok(())
}

// Handles <podcast:person>
//
// Per the spec the role defaults to "host" and the group to "cast". A role or group that is not in the
// taxonomy does not fail the feed; the element is kept as an extension instead so nothing is lost.
// `strict` restores the earlier behaviour and fails the feed on the first unknown value.
fn handle_person<R: BufRead>(element: Element<R>, extensions: &mut Vec<Extension>, strict: bool) -> ParseFeedResult<Option<PodcastPerson>> {
    let role = match element.attr_value("role") {
        Some(role) => match Role::parse(element.buffer_pos, role.trim()) {
            Ok(role) => Some(role),
            Err(err) if strict => return Err(err),
            Err(_) => None,
        },
        None => Some(Role::Host),
    };
    let group = match element.attr_value("group") {
        Some(group) => match Group::parse(element.buffer_pos, group.trim()) {
            Ok(group) => Some(group),
            Err(err) if strict => return Err(err),
            Err(_) => None,
        },
        None => Some(Group::default()),
    };

    let (Some(role), Some(group)) = (role, group) else {
        extensions.extend(capture(element)?);
        return Ok(None);
    };

    Ok(element.child_as_text().map(|name| PodcastPerson {
        name,
        role,
        group,
        img: element.attr_value("img").and_then(|img| Url::parse(&img).ok()),
        href: element.attr_value("href").and_then(|href| Url::parse(&href).ok()),
    }))
}

// Handles <podcast:transcript>
fn handle_transcript<R: BufRead>(element: Element<R>) -> Option<Transcript> {
    if let Some(url) = element.attr_value("url").and_then(|url| parse_uri(&url, None)) {
        if let Some(content_type) = element.attr_value("type").and_then(|content_type| content_type.parse::<MediaTypeBuf>().ok()) {
            return Some(Transcript {
                url,
                content_type,
                language: None,
                rel: element.attr_value("rel"),
            });
        }
    }

    None
}
