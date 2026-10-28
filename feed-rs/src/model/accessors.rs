//! Convenience accessors over captured extensions (F1) and typed views of common extension elements (F2).
//!
//! Elements without a dedicated model field are kept as [`Extension`] trees. These helpers find them by namespace and
//! name, and expose well-known ones (Slash, Syndication, GeoRSS, RSS `source`/`cloud`, Podcasting 2.0 and item-level
//! iTunes tags) as typed values. Every typed accessor is lenient: malformed values yield `None` rather than an error.

use std::time::Duration;

use super::{Entry, Extension, Feed};

/// Namespace URIs for use with the extension accessors.
///
/// Elements without a namespace (e.g. RSS 2.0 `<source>` or `<cloud>`) have a namespace of `None`, and JSON Feed keys
/// use [`JSON_FEED`](crate::model::ns::JSON_FEED).
pub mod ns {
    /// Atom 1.0
    pub const ATOM: &str = "http://www.w3.org/2005/Atom";
    /// RSS 1.0
    pub const RSS1: &str = "http://purl.org/rss/1.0/";
    /// RDF (used by RSS 1.0)
    pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    /// RSS 1.0 content module (`content:encoded`)
    pub const CONTENT: &str = "http://purl.org/rss/1.0/modules/content/";
    /// Dublin Core elements
    pub const DUBLIN_CORE: &str = "http://purl.org/dc/elements/1.1/";
    /// Dublin Core terms
    pub const DUBLIN_CORE_TERMS: &str = "http://purl.org/dc/terms/";
    /// Media RSS
    pub const MEDIA_RSS: &str = "http://search.yahoo.com/mrss/";
    /// Apple Podcasts (iTunes)
    pub const ITUNES: &str = "http://www.itunes.com/dtds/podcast-1.0.dtd";
    /// Podcasting 2.0 (Podcast Index)
    pub const PODCAST: &str = "https://podcastindex.org/namespace/1.0";
    /// Google Play podcasts
    pub const GOOGLE_PLAY: &str = "http://www.google.com/schemas/play-podcasts/1.0";
    /// Well-Formed Web comment API (`wfw:commentRss`)
    pub const WFW: &str = "http://wellformedweb.org/CommentAPI/";
    /// Slash (`slash:comments`)
    pub const SLASH: &str = "http://purl.org/rss/1.0/modules/slash/";
    /// Syndication (`sy:updatePeriod`, `sy:updateFrequency`)
    pub const SYNDICATION: &str = "http://purl.org/rss/1.0/modules/syndication/";
    /// GeoRSS simple (`georss:point`)
    pub const GEORSS: &str = "http://www.georss.org/georss";
    /// Atom threading extensions (`thr:total`, `thr:in-reply-to`)
    pub const THREADING: &str = "http://purl.org/syndication/thread/1.0";
    /// Namespace given to unknown JSON Feed keys
    pub const JSON_FEED: &str = "https://jsonfeed.org";
}

// Shared lookups over a list of extensions
fn first<'a>(extensions: &'a [Extension], namespace: Option<&str>, name: &str) -> Option<&'a Extension> {
    extensions.iter().find(|e| e.is(namespace, name))
}

fn text<'a>(extensions: &'a [Extension], namespace: Option<&str>, name: &str) -> Option<&'a str> {
    first(extensions, namespace, name)
        .and_then(|e| e.text.as_deref())
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

// "yes"/"true" and "no"/"false" (any case); iTunes also uses "explicit"/"clean"
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "yes" | "true" | "explicit" => Some(true),
        "no" | "false" | "clean" => Some(false),
        _ => None,
    }
}

fn parse_seconds(value: &str) -> Option<Duration> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .and_then(|v| Duration::try_from_secs_f64(v).ok())
}

fn georss_point(extensions: &[Extension]) -> Option<GeoPoint> {
    let point = text(extensions, Some(ns::GEORSS), "point")?;
    let mut parts = point.split(|c: char| c.is_whitespace() || c == ',').filter(|p| !p.is_empty());
    let latitude = parts.next()?.parse::<f64>().ok()?;
    let longitude = parts.next()?.parse::<f64>().ok()?;
    ((-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude)).then_some(GeoPoint { latitude, longitude })
}

macro_rules! extension_lookups {
    ($ty:ty) => {
        impl $ty {
            /// The first extension with the given namespace URI (see [`ns`](crate::model::ns)) and local name.
            pub fn extension(&self, namespace: Option<&str>, name: &str) -> Option<&Extension> {
                first(&self.extensions, namespace, name)
            }

            /// All extensions with the given namespace URI and local name.
            pub fn extensions_named<'a>(&'a self, namespace: Option<&'a str>, name: &'a str) -> impl Iterator<Item = &'a Extension> + 'a {
                self.extensions.iter().filter(move |e| e.is(namespace, name))
            }

            /// All extensions in the given namespace (`None` for elements without a namespace).
            pub fn extensions_in<'a>(&'a self, namespace: Option<&'a str>) -> impl Iterator<Item = &'a Extension> + 'a {
                self.extensions.iter().filter(move |e| e.namespace.as_deref() == namespace)
            }

            /// The trimmed, non-empty text of the first matching extension.
            pub fn extension_text(&self, namespace: Option<&str>, name: &str) -> Option<&str> {
                text(&self.extensions, namespace, name)
            }

            /// GeoRSS simple `georss:point` ("lat lon").
            pub fn geo_point(&self) -> Option<GeoPoint> {
                georss_point(&self.extensions)
            }
        }
    };
}

extension_lookups!(Feed);
extension_lookups!(Entry);

/// A latitude / longitude pair (WGS84 degrees) from `georss:point`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct GeoPoint {
    pub latitude: f64,
    pub longitude: f64,
}

/// RSS 2.0 `<cloud>` (rssCloud notification endpoint).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Cloud {
    pub domain: String,
    pub port: Option<u16>,
    pub path: Option<String>,
    pub register_procedure: Option<String>,
    pub protocol: Option<String>,
}

/// RSS 2.0 item `<source>`: the channel the item came from.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Source {
    pub url: String,
    pub title: Option<String>,
}

/// Podcasting 2.0 `<podcast:locked>`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Locked {
    /// Whether other platforms may import the feed (`yes` means locked)
    pub locked: bool,
    pub owner: Option<String>,
}

/// Podcasting 2.0 `<podcast:funding>`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Funding {
    pub url: String,
    pub message: Option<String>,
}

/// Podcasting 2.0 `<podcast:value>` block.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ValueBlock {
    pub value_type: String,
    pub method: String,
    pub suggested: Option<String>,
    pub recipients: Vec<ValueRecipient>,
}

/// Podcasting 2.0 `<podcast:valueRecipient>`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ValueRecipient {
    pub name: Option<String>,
    pub recipient_type: String,
    pub address: String,
    pub split: u32,
    pub fee: bool,
    pub custom_key: Option<String>,
    pub custom_value: Option<String>,
}

/// Podcasting 2.0 `<podcast:chapters>`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Chapters {
    pub url: String,
    pub content_type: String,
}

/// Podcasting 2.0 `<podcast:alternateEnclosure>`: another version of the episode media (different format, bitrate,
/// language, ...), available from one or more sources.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AlternateEnclosure {
    pub mime_type: String,
    pub length: Option<u64>,
    pub bitrate: Option<f64>,
    pub height: Option<u32>,
    pub lang: Option<String>,
    pub title: Option<String>,
    pub rel: Option<String>,
    pub codecs: Option<String>,
    pub default: bool,
    /// URIs from the `<podcast:source>` children, in document order
    pub sources: Vec<String>,
}

/// Podcasting 2.0 `<podcast:soundbite>`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Soundbite {
    pub start: Duration,
    pub duration: Duration,
    pub title: Option<String>,
}

fn value_block(e: &Extension) -> Option<ValueBlock> {
    Some(ValueBlock {
        value_type: e.attr("type")?.to_string(),
        method: e.attr("method")?.to_string(),
        suggested: e.attr("suggested").map(str::to_string),
        recipients: e
            .children_named(Some(ns::PODCAST), "valueRecipient")
            .filter_map(|r| {
                Some(ValueRecipient {
                    name: r.attr("name").map(str::to_string),
                    recipient_type: r.attr("type")?.to_string(),
                    address: r.attr("address")?.to_string(),
                    split: r.attr("split")?.trim().parse().ok()?,
                    fee: r.attr("fee").and_then(parse_bool).unwrap_or(false),
                    custom_key: r.attr("customKey").map(str::to_string),
                    custom_value: r.attr("customValue").map(str::to_string),
                })
            })
            .collect(),
    })
}

impl Feed {
    /// JSON Feed `user_comment`: a note for someone reading the raw JSON.
    ///
    /// The JSON Feed spec says this is not for display in a reader, so it is kept as an extension rather than a field.
    pub fn json_user_comment(&self) -> Option<&str> {
        self.extension_text(Some(ns::JSON_FEED), "user_comment")
    }

    /// `sy:updatePeriod` (hourly, daily, weekly, monthly or yearly), lower-cased.
    pub fn update_period(&self) -> Option<String> {
        self.extension_text(Some(ns::SYNDICATION), "updatePeriod").map(str::to_ascii_lowercase)
    }

    /// `sy:updateFrequency`: how many times per period the feed updates (defaults to 1 if a period is given).
    pub fn update_frequency(&self) -> Option<u32> {
        match self.extension_text(Some(ns::SYNDICATION), "updateFrequency") {
            Some(f) => f.parse().ok().filter(|f| *f > 0),
            None => self.update_period().map(|_| 1),
        }
    }

    /// The expected interval between updates, from `sy:updatePeriod` / `sy:updateFrequency` (months and years are
    /// approximated as 30 and 365 days).
    pub fn update_interval(&self) -> Option<Duration> {
        let period: u64 = match self.update_period()?.as_str() {
            "hourly" => 3600,
            "daily" => 86_400,
            "weekly" => 7 * 86_400,
            "monthly" => 30 * 86_400,
            "yearly" => 365 * 86_400,
            _ => return None,
        };
        Some(Duration::from_secs(period / u64::from(self.update_frequency()?)))
    }

    /// RSS 2.0 `<cloud>`.
    pub fn cloud(&self) -> Option<Cloud> {
        let e = self.extension(None, "cloud")?;
        Some(Cloud {
            domain: e.attr("domain")?.to_string(),
            port: e.attr("port").and_then(|p| p.trim().parse().ok()),
            path: e.attr("path").map(str::to_string),
            register_procedure: e.attr("registerProcedure").map(str::to_string),
            protocol: e.attr("protocol").map(str::to_string),
        })
    }

    /// `itunes:new-feed-url`: the feed has moved here.
    pub fn itunes_new_feed_url(&self) -> Option<&str> {
        self.extension_text(Some(ns::ITUNES), "new-feed-url")
    }

    /// `itunes:type` ("episodic" or "serial"), lower-cased.
    pub fn itunes_type(&self) -> Option<String> {
        self.extension_text(Some(ns::ITUNES), "type").map(str::to_ascii_lowercase)
    }

    /// `itunes:block` at channel level.
    pub fn itunes_block(&self) -> Option<bool> {
        self.extension_text(Some(ns::ITUNES), "block").and_then(parse_bool)
    }

    /// `itunes:complete`: no more episodes will be published.
    pub fn itunes_complete(&self) -> Option<bool> {
        self.extension_text(Some(ns::ITUNES), "complete").and_then(parse_bool)
    }

    /// `podcast:guid`: the podcast's globally unique identifier.
    pub fn podcast_guid(&self) -> Option<&str> {
        self.extension_text(Some(ns::PODCAST), "guid")
    }

    /// `podcast:locked`.
    pub fn podcast_locked(&self) -> Option<Locked> {
        let e = self.extension(Some(ns::PODCAST), "locked")?;
        Some(Locked {
            locked: e.text.as_deref().and_then(parse_bool)?,
            owner: e.attr("owner").map(str::to_string),
        })
    }

    /// All `podcast:funding` links.
    pub fn podcast_funding(&self) -> Vec<Funding> {
        self.extensions_named(Some(ns::PODCAST), "funding")
            .filter_map(|e| {
                Some(Funding {
                    url: e.attr("url")?.trim().to_string(),
                    message: e.text.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string),
                })
            })
            .collect()
    }

    /// Channel-level `podcast:value` blocks.
    pub fn podcast_value(&self) -> Vec<ValueBlock> {
        self.extensions_named(Some(ns::PODCAST), "value").filter_map(value_block).collect()
    }
}

impl Entry {
    /// `slash:comments`: the number of comments.
    pub fn comment_count(&self) -> Option<u64> {
        self.extension_text(Some(ns::SLASH), "comments").and_then(|c| c.parse().ok())
    }

    /// RSS 2.0 `<source url="...">title</source>`.
    ///
    /// (Named `rss_source` to avoid confusion with the [`Entry::source`] field, which is reserved for the Atom
    /// `<source>` element.)
    pub fn rss_source(&self) -> Option<Source> {
        let e = self.extension(None, "source")?;
        Some(Source {
            url: e.attr("url")?.trim().to_string(),
            title: e.text.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string),
        })
    }

    /// Item-level `itunes:explicit`.
    pub fn itunes_explicit(&self) -> Option<bool> {
        self.extension_text(Some(ns::ITUNES), "explicit").and_then(parse_bool)
    }

    /// `itunes:episodeType` ("full", "trailer" or "bonus"), lower-cased.
    pub fn itunes_episode_type(&self) -> Option<String> {
        self.extension_text(Some(ns::ITUNES), "episodeType").map(str::to_ascii_lowercase)
    }

    /// Item-level `itunes:block`.
    pub fn itunes_block(&self) -> Option<bool> {
        self.extension_text(Some(ns::ITUNES), "block").and_then(parse_bool)
    }

    /// `podcast:chapters`.
    pub fn podcast_chapters(&self) -> Option<Chapters> {
        let e = self.extension(Some(ns::PODCAST), "chapters")?;
        Some(Chapters {
            url: e.attr("url")?.trim().to_string(),
            content_type: e.attr("type")?.trim().to_string(),
        })
    }

    /// All `podcast:soundbite` elements.
    pub fn podcast_soundbites(&self) -> Vec<Soundbite> {
        self.extensions_named(Some(ns::PODCAST), "soundbite")
            .filter_map(|e| {
                Some(Soundbite {
                    start: parse_seconds(e.attr("startTime")?)?,
                    duration: parse_seconds(e.attr("duration")?)?,
                    title: e.text.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string),
                })
            })
            .collect()
    }

    /// Item-level `podcast:value` blocks.
    pub fn podcast_value(&self) -> Vec<ValueBlock> {
        self.extensions_named(Some(ns::PODCAST), "value").filter_map(value_block).collect()
    }

    /// All `podcast:alternateEnclosure` elements (those without a `type` are skipped).
    pub fn podcast_alternate_enclosures(&self) -> Vec<AlternateEnclosure> {
        self.extensions_named(Some(ns::PODCAST), "alternateEnclosure")
            .filter_map(|e| {
                Some(AlternateEnclosure {
                    mime_type: e.attr("type")?.trim().to_string(),
                    length: e.attr("length").and_then(|v| v.trim().parse().ok()),
                    bitrate: e.attr("bitrate").and_then(|v| v.trim().parse().ok()),
                    height: e.attr("height").and_then(|v| v.trim().parse().ok()),
                    lang: e.attr("lang").map(str::to_string),
                    title: e.attr("title").map(str::to_string),
                    rel: e.attr("rel").map(str::to_string),
                    codecs: e.attr("codecs").map(str::to_string),
                    default: e.attr("default").and_then(parse_bool).unwrap_or(false),
                    sources: e
                        .children_named(Some(ns::PODCAST), "source")
                        .filter_map(|s| s.attr("uri"))
                        .map(|u| u.trim().to_string())
                        .collect(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
