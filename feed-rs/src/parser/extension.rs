use std::cell::Cell;
use std::io::BufRead;

use serde_json::Value;

use crate::model::{Extension, ExtensionAttribute};
use crate::parser::{ParseFeedResult, ParseLimits};
use crate::xml::{Element, ElementIter};

const JSON_FEED: &str = "https://jsonfeed.org";

/// Settings and running budget for capturing unmapped elements during one parse.
#[derive(Debug)]
pub(crate) struct CaptureConfig {
    enabled: bool,
    max_depth: usize,
    max_nodes: usize,
    max_text_bytes: usize,
    used_nodes: Cell<usize>,
}

impl CaptureConfig {
    pub(crate) fn new(enabled: bool, limits: &ParseLimits) -> CaptureConfig {
        CaptureConfig {
            enabled,
            max_depth: limits.max_extension_depth.max(1),
            max_nodes: limits.max_extension_nodes,
            max_text_bytes: limits.max_extension_text_bytes,
            used_nodes: Cell::new(0),
        }
    }

    /// Capture configuration that keeps nothing (used where no parser configuration is available)
    pub(crate) fn disabled() -> CaptureConfig {
        CaptureConfig::new(false, &ParseLimits::default())
    }

    /// Reserve space for one more node, returning false once the budget is spent
    fn take_node(&self) -> bool {
        let used = self.used_nodes.get();
        if used >= self.max_nodes {
            false
        } else {
            self.used_nodes.set(used + 1);
            true
        }
    }

    /// Trims and truncates text to the configured maximum (at a character boundary)
    fn text(&self, text: Option<String>) -> Option<String> {
        let text = text?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(truncate(trimmed, self.max_text_bytes).to_owned())
    }
}

/// Truncates the string to at most `max` bytes, on a character boundary
fn truncate(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Keeps an element the dedicated fields do not cover, including its children.
///
/// The tree is walked iteratively (not recursively), so deeply nested input cannot overflow the stack.
/// Children deeper than the configured maximum depth, or beyond the node budget, are skipped.
/// Returns `None` when extension capture is disabled or the budget is exhausted; the element's subtree
/// is then skipped by the caller's iterator.
pub(crate) fn capture<R: BufRead>(element: Element<'_, R>) -> ParseFeedResult<Option<Extension>> {
    let config = element.capture_config();
    if !config.enabled || !config.take_node() {
        return Ok(None);
    }

    let root = start(&element, config);
    let mut stack: Vec<(Extension, ElementIter<'_, R>)> = vec![(root, element.into_children())];
    loop {
        let (_, iter) = stack.last_mut().expect("capture stack is never empty here");
        match iter.next() {
            Some(Err(e)) => return Err(e.into()),
            Some(Ok(child)) => {
                // Too deep or over budget: drop the child, its subtree is skipped by the iterator
                if stack.len() >= config.max_depth || !config.take_node() {
                    continue;
                }
                let extension = start(&child, config);
                stack.push((extension, child.into_children()));
            }
            None => {
                let (mut done, _) = stack.pop().expect("capture stack is never empty here");
                // Vec growth leaves spare capacity; extensions are kept for the lifetime of the feed
                done.children.shrink_to_fit();
                match stack.last_mut() {
                    Some((parent, iter)) => {
                        // Text that follows this element inside its parent (mixed content)
                        done.tail = config.text(iter.text());
                        parent.children.push(done);
                    }
                    None => return Ok(Some(done)),
                }
            }
        }
    }
}

// Builds the extension for an element from its name, attributes and leading text
fn start<R: BufRead>(element: &Element<'_, R>, config: &CaptureConfig) -> Extension {
    let attributes = element
        .attributes
        .iter()
        .filter(|attr| attr.name != "xmlns" && !attr.name.starts_with("xmlns:"))
        .map(|attr| {
            let (prefix, name) = split_qname(&attr.name);
            ExtensionAttribute {
                name: name.to_owned(),
                value: attr.value.clone(),
                prefix: prefix.map(str::to_owned),
                namespace: attr.namespace_uri.clone(),
            }
        })
        .collect();
    Extension {
        namespace: element.namespace_uri.clone(),
        prefix: element.prefix.clone(),
        name: element.name.clone(),
        attributes,
        text: config.text(element.child_as_text()),
        tail: None,
        children: Vec::new(),
        value: None,
    }
}

// Splits "prefix:local" into its parts
fn split_qname(qname: &str) -> (Option<&str>, &str) {
    match qname.split_once(':') {
        Some((prefix, local)) => (Some(prefix), local),
        None => (None, qname),
    }
}

/// Converts the unmapped properties of a JSON Feed object into extensions.
///
/// Each top-level property becomes one extension whose `value` holds the raw JSON, with `text` / `children`
/// populated as a convenience view.
pub(crate) fn from_json(values: serde_json::Map<String, Value>, config: &CaptureConfig) -> Vec<Extension> {
    if !config.enabled {
        return Vec::new();
    }
    let mut extensions = Vec::new();
    for (name, value) in values {
        if !config.take_node() {
            break;
        }
        let mut extension = json_value(name, value.clone(), config, 1);
        extension.value = Some(value);
        extensions.push(extension);
    }
    extensions
}

fn json_value(name: String, value: Value, config: &CaptureConfig, depth: usize) -> Extension {
    match value {
        Value::Null => Extension::named(Some(JSON_FEED), &name, None),
        Value::String(text) => leaf(name, text, config),
        Value::Number(number) => leaf(name, number.to_string(), config),
        Value::Bool(flag) => leaf(name, flag.to_string(), config),
        Value::Array(items) if items.iter().all(|item| !item.is_array() && !item.is_object()) => {
            let text = items.into_iter().filter_map(scalar).collect::<Vec<_>>().join(", ");
            leaf(name, text, config)
        }
        Value::Array(items) => branch(name, children(items.into_iter().map(|item| ("item".to_owned(), item)), config, depth)),
        Value::Object(object) => branch(name, children(object, config, depth)),
    }
}

fn children(values: impl IntoIterator<Item = (String, Value)>, config: &CaptureConfig, depth: usize) -> Vec<Extension> {
    if depth >= config.max_depth {
        return Vec::new();
    }
    let mut children = Vec::new();
    for (key, value) in values {
        if !config.take_node() {
            break;
        }
        children.push(json_value(key, value, config, depth + 1));
    }
    children
}

fn scalar(value: Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn leaf(name: String, text: String, config: &CaptureConfig) -> Extension {
    Extension::named(Some(JSON_FEED), &name, config.text(Some(text)))
}

fn branch(name: String, children: Vec<Extension>) -> Extension {
    let mut extension = Extension::named(Some(JSON_FEED), &name, None);
    extension.children = children;
    extension
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("héllo", 2), "h");
        assert_eq!(truncate("héllo", 3), "hé");
        assert_eq!(truncate("abc", 10), "abc");
    }

    #[test]
    fn split_qname_handles_prefixes() {
        assert_eq!(split_qname("xml:lang"), (Some("xml"), "lang"));
        assert_eq!(split_qname("href"), (None, "href"));
    }
}
