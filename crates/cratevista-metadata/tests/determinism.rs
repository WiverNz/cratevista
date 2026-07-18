//! Output is deterministic regardless of input package/node ordering.

mod common;

use std::path::PathBuf;

use cratevista_metadata::{MetadataOptions, normalize};

fn load_value(name: &str) -> serde_json::Value {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("fixtures");
    path.push(format!("{name}.metadata.json"));
    let text = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn reversed(mut value: serde_json::Value) -> cargo_metadata::Metadata {
    if let Some(packages) = value["packages"].as_array_mut() {
        packages.reverse();
    }
    if let Some(nodes) = value
        .get_mut("resolve")
        .and_then(|r| r.get_mut("nodes"))
        .and_then(|n| n.as_array_mut())
    {
        nodes.reverse();
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn reordered_input_produces_identical_output() {
    let options = MetadataOptions::default();

    let normal: cargo_metadata::Metadata =
        serde_json::from_value(load_value("workspace_deps")).unwrap();
    let shuffled = reversed(load_value("workspace_deps"));

    let a = normalize(&normal, &options).unwrap();
    let b = normalize(&shuffled, &options).unwrap();

    assert_eq!(a.entities, b.entities, "entities must be order-independent");
    assert_eq!(
        a.relations, b.relations,
        "relations must be order-independent"
    );
    assert_eq!(a.diagnostics, b.diagnostics);
}

#[test]
fn repeated_normalization_is_stable() {
    let metadata = common::load("workspace_deps");
    let options = MetadataOptions::default();
    assert_eq!(
        normalize(&metadata, &options).unwrap(),
        normalize(&metadata, &options).unwrap()
    );
}
