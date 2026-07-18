//! The single canonical JSON serializer used for every artifact and the JSON
//! Schema.
//!
//! It recursively orders object keys, preserves array order, uses stable pretty
//! formatting (two-space indent, via `serde_json`), writes UTF-8, and terminates
//! the output with exactly one newline. Using one serializer everywhere makes
//! `document.json` / `diagnostics.json` byte-stable and keeps the JSON Schema
//! artifact and its drift test in lockstep.

use serde::Serialize;
use serde_json::Value;

/// Serializes any value to canonical JSON (see the module docs).
pub fn to_canonical_string<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(value)?;
    let canonical = canonicalize(value);
    let mut text = serde_json::to_string_pretty(&canonical)?;
    // Normalize to exactly one trailing newline.
    while text.ends_with('\n') {
        text.pop();
    }
    text.push('\n');
    Ok(text)
}

/// Recursively rebuilds a JSON value with object keys sorted; array order is
/// preserved.
fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .into_iter()
                .map(|(key, val)| (key, canonicalize(val)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = serde_json::Map::new();
            for (key, val) in entries {
                out.insert(key, val);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonicalize).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_sorted_recursively_and_arrays_preserved() {
        let value = serde_json::json!({
            "b": 1,
            "a": { "z": [3, 1, 2], "y": 0 },
        });
        let text = to_canonical_string(&value).unwrap();
        let expected = "{\n  \"a\": {\n    \"y\": 0,\n    \"z\": [\n      3,\n      1,\n      2\n    ]\n  },\n  \"b\": 1\n}\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn exactly_one_trailing_newline() {
        let text = to_canonical_string(&serde_json::json!({"a": 1})).unwrap();
        assert!(text.ends_with("}\n"));
        assert!(!text.ends_with("\n\n"));
    }
}
