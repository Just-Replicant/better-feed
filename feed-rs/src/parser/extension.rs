use std::io::BufRead;

use serde_json::Value;

use crate::model::{Extension, ExtensionAttribute};
use crate::parser::ParseFeedResult;
use crate::xml::Element;

const JSON_FEED: &str = "https://jsonfeed.org";

/// Keeps an element the dedicated fields do not cover, including its children.
pub(crate) fn capture<R: BufRead>(element: Element<R>) -> ParseFeedResult<Extension> {
    let namespace = element.namespace_uri.clone();
    let name = element.name.clone();
    let attributes = element
        .attributes
        .iter()
        .filter(|attr| attr.name != "xmlns" && !attr.name.starts_with("xmlns:"))
        .map(|attr| ExtensionAttribute {
            name: attr.name.clone(),
            value: attr.value.clone(),
        })
        .collect();
    let text = element.child_as_text().map(|text| text.trim().to_owned()).filter(|text| !text.is_empty());
    let mut children = Vec::new();
    for child in element.children() {
        children.push(capture(child?)?);
    }
    Ok(Extension {
        namespace,
        name,
        attributes,
        text,
        children,
    })
}

pub(crate) fn from_json(values: serde_json::Map<String, Value>) -> Vec<Extension> {
    values.into_iter().filter_map(|(name, value)| json_value(name, value)).collect()
}

fn json_value(name: String, value: Value) -> Option<Extension> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(leaf(name, text)),
        Value::Number(number) => Some(leaf(name, number.to_string())),
        Value::Bool(flag) => Some(leaf(name, if flag { "true" } else { "false" }.to_owned())),
        Value::Array(items) if items.iter().all(|item| !item.is_array() && !item.is_object()) => {
            let text = items.into_iter().filter_map(scalar).collect::<Vec<_>>().join(", ");
            if text.is_empty() { None } else { Some(leaf(name, text)) }
        }
        Value::Array(items) => {
            let children = items.into_iter().filter_map(|item| json_value("item".to_owned(), item)).collect();
            Some(branch(name, children))
        }
        Value::Object(object) => {
            let children = object.into_iter().filter_map(|(key, value)| json_value(key, value)).collect();
            Some(branch(name, children))
        }
    }
}

fn scalar(value: Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(if flag { "true" } else { "false" }.to_owned()),
        _ => None,
    }
}

fn leaf(name: String, text: String) -> Extension {
    let text = text.trim().to_owned();
    Extension {
        namespace: Some(JSON_FEED.to_owned()),
        name,
        attributes: Vec::new(),
        text: if text.is_empty() { None } else { Some(text) },
        children: Vec::new(),
    }
}

fn branch(name: String, children: Vec<Extension>) -> Extension {
    Extension {
        namespace: Some(JSON_FEED.to_owned()),
        name,
        attributes: Vec::new(),
        text: None,
        children,
    }
}
