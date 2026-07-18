//! Determinism under a reordered rustdoc index: the same JSON with its `index`
//! object keys reordered must produce an identical `CrateIngest`.

mod common;

use common::sample_context;
use cratevista_rustdoc::normalize_json;
use serde_json::Value;

/// Rebuilds the JSON with the `index` map's entries in reverse key order, which
/// serde deserializes into the same `HashMap` regardless — proving the output
/// does not depend on JSON object ordering.
fn reordered(json: &str) -> String {
    let mut value: Value = serde_json::from_str(json).unwrap();
    if let Some(Value::Object(index)) = value.get_mut("index") {
        let mut entries: Vec<(String, Value)> =
            index.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        entries.reverse();
        let mut rebuilt = serde_json::Map::new();
        for (k, v) in entries {
            rebuilt.insert(k, v);
        }
        *index = rebuilt;
    }
    serde_json::to_string(&value).unwrap()
}

#[test]
fn reordered_index_produces_identical_output() {
    let json = common::fixture("sample_lib");
    let context = sample_context();
    let base = normalize_json(&json, &context).unwrap();
    let shuffled = normalize_json(&reordered(&json), &context).unwrap();

    assert_eq!(base.entities, shuffled.entities);
    assert_eq!(base.relations, shuffled.relations);
    assert_eq!(base.diagnostics, shuffled.diagnostics);
    assert_eq!(base.summary, shuffled.summary);
}
