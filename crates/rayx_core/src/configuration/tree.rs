//! The configuration tree: paths, key matching and merging.

use serde_json::{Map, Value};

/// A path's segments: `:` separates them (`"Feed:Retry:Limit"`); empty segments are skipped.
pub(crate) fn segments(path: &str) -> impl Iterator<Item = &str> {
    path.split(':').filter(|segment| !segment.is_empty())
}

/// `path` with its empty segments removed.
pub(crate) fn canonical(path: &str) -> String {
    segments(path).collect::<Vec<_>>().join(":")
}

pub(crate) fn join(parent: &str, child: &str) -> String {
    match (parent.is_empty(), canonical(child)) {
        (_, child) if child.is_empty() => parent.to_owned(),
        (true, child) => child,
        (false, child) => format!("{parent}:{child}"),
    }
}

/// Whether two keys name the same entry: ASCII case, `_` and `-` are ignored, so
/// `connection_string`, `connectionString`, `ConnectionString` and `connection-string` match.
pub(crate) fn keys_match(left: &str, right: &str) -> bool {
    let significant = |key: &str| {
        key.bytes()
            .filter(|byte| !matches!(byte, b'_' | b'-'))
            .map(|byte| byte.to_ascii_lowercase())
            .collect::<Vec<_>>()
    };
    left == right || significant(left) == significant(right)
}

/// The entry of `object` that `key` names: the exact key first, else a matching one.
fn entry<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object.get(key).or_else(|| {
        object
            .iter()
            .find_map(|(existing, value)| keys_match(existing, key).then_some(value))
    })
}

fn entry_key(object: &Map<String, Value>, key: &str) -> Option<String> {
    if object.contains_key(key) {
        return Some(key.to_owned());
    }
    object
        .keys()
        .find(|existing| keys_match(existing, key))
        .cloned()
}

/// The value at `path`: objects by key, arrays by index (`"Items:0"`).
pub(crate) fn lookup<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    segments(path).try_fold(root, |current, segment| match current {
        Value::Object(object) => entry(object, segment),
        Value::Array(items) => segment
            .parse::<usize>()
            .ok()
            .and_then(|index| items.get(index)),
        _ => None,
    })
}

/// Sets the value at `path`, making the objects on the way (an array element by index when it
/// exists). An empty path replaces `root`.
pub(crate) fn set(root: &mut Value, path: &str, value: Value) {
    let mut current = root;
    for segment in segments(path) {
        let element = match (&*current, segment.parse::<usize>()) {
            (Value::Array(items), Ok(index)) if index < items.len() => Some(index),
            _ => None,
        };
        current = match (current, element) {
            (Value::Array(items), Some(index)) => &mut items[index],
            (other, _) => {
                if !other.is_object() {
                    *other = Value::Object(Map::new());
                }
                let Value::Object(object) = other else {
                    return;
                };
                let key = entry_key(object, segment).unwrap_or_else(|| segment.to_owned());
                object.entry(key).or_insert(Value::Null)
            }
        };
    }
    *current = value;
}

/// Merges a later source's tree over an earlier one's: objects merge key by key (keys matched
/// by [`keys_match`], the earlier spelling kept), anything else (an array, a scalar, `null`)
/// replaces what was there.
pub(crate) fn merge(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                match entry_key(base, key) {
                    Some(existing) => {
                        if let Some(slot) = base.get_mut(&existing) {
                            merge(slot, value);
                        }
                    }
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, overlay) => *base = overlay.clone(),
    }
}

/// Overlays a configuration section on an options value's serialized form, for binding: as
/// [`merge`], except that `null` keeps the value's own (a key with no value binds nothing).
pub(crate) fn overlay(base: &mut Value, section: &Value) {
    match (base, section) {
        (_, Value::Null) => {}
        (Value::Object(base), Value::Object(section)) => {
            for (key, value) in section {
                match entry_key(base, key) {
                    Some(existing) => {
                        if let Some(slot) = base.get_mut(&existing) {
                            overlay(slot, value);
                        }
                    }
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, section) => *base = section.clone(),
    }
}

pub(crate) fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_match_across_case_and_word_separators() {
        assert!(keys_match("connection_string", "ConnectionString"));
        assert!(keys_match("connection-string", "connectionString"));
        assert!(!keys_match("connection", "connections"));
    }

    #[test]
    fn lookup_follows_objects_and_array_indexes() {
        let root = json!({ "Feed": { "Hosts": ["a", "b"], "retry_limit": 3 } });
        assert_eq!(lookup(&root, "feed:hosts:1"), Some(&json!("b")));
        assert_eq!(lookup(&root, "Feed:RetryLimit"), Some(&json!(3)));
        assert_eq!(lookup(&root, ""), Some(&root));
        assert_eq!(lookup(&root, "Feed:Hosts:2"), None);
    }

    #[test]
    fn set_makes_objects_and_keeps_existing_spelling() {
        let mut root = json!({ "Feed": { "Label": "a" } });
        set(&mut root, "feed:label", json!("b"));
        set(&mut root, "Feed:Retry:Limit", json!(2));
        assert_eq!(
            root,
            json!({ "Feed": { "Label": "b", "Retry": { "Limit": 2 } } })
        );
    }

    #[test]
    fn merge_replaces_arrays_and_overlay_skips_nulls() {
        let mut merged = json!({ "a": [1, 2, 3], "b": { "c": 1 } });
        merge(&mut merged, &json!({ "A": [4], "b": { "d": null } }));
        assert_eq!(merged, json!({ "a": [4], "b": { "c": 1, "d": null } }));

        let mut bound = json!({ "label": "x", "hosts": ["a"] });
        overlay(&mut bound, &json!({ "Label": null, "Hosts": ["b", "c"] }));
        assert_eq!(bound, json!({ "label": "x", "hosts": ["b", "c"] }));
    }
}
