//! Stable-identity behavior: impl disambiguation and same-named method
//! distinctness, exercised through the public `normalize_json`.

mod common;

use common::sample_context;
use cratevista_rustdoc::normalize_json;

fn ingest() -> cratevista_rustdoc::CrateIngest {
    let json = common::fixture("sample_lib");
    normalize_json(&json, &sample_context()).expect("normalize")
}

#[test]
fn every_impl_id_is_unique() {
    let ingest = ingest();
    let impl_ids: Vec<&str> = ingest
        .entities
        .iter()
        .filter(|e| e.kind.as_str() == "impl")
        .map(|e| e.id.as_str())
        .collect();
    let mut unique = impl_ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(impl_ids.len(), unique.len(), "impl ids must not collide");
}

#[test]
fn impl_ids_carry_a_signature_discriminator() {
    let ingest = ingest();
    // impl_block id form: impl:{crate}:{trait_or_inherent}:{for}:{disc}
    for entity in ingest.entities.iter().filter(|e| e.kind.as_str() == "impl") {
        let parts: Vec<&str> = entity.id.as_str().split(':').collect();
        assert!(
            parts.len() >= 5,
            "impl id has a discriminator segment: {}",
            entity.id
        );
        let disc = parts.last().unwrap();
        assert_eq!(
            disc.len(),
            32,
            "discriminator is a 128-bit hex: {}",
            entity.id
        );
        assert!(disc.chars().all(|c| c.is_ascii_hexdigit()));
    }
}

#[test]
fn same_named_methods_in_different_impls_are_distinct() {
    let ingest = ingest();
    // `From::from`, `Into::into`, etc. exist across many blanket impls; each is a
    // distinct entity scoped under its impl.
    let from_methods: Vec<&str> = ingest
        .entities
        .iter()
        .filter(|e| e.kind.as_str() == "method" && e.label.default == "from")
        .map(|e| e.id.as_str())
        .collect();
    let mut unique = from_methods.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        from_methods.len(),
        unique.len(),
        "same-named methods in different impls get distinct ids"
    );
    // Each method id is scoped under an impl id.
    for id in &from_methods {
        assert!(id.starts_with("impl:sample_lib:"));
    }
}

#[test]
fn methods_are_contained_by_their_impl() {
    let ingest = ingest();
    let greet_method = ingest
        .entities
        .iter()
        .find(|e| e.kind.as_str() == "method" && e.label.default == "greet")
        .expect("greet method");
    let parent = greet_method
        .parent
        .as_ref()
        .expect("greet has a parent impl");
    assert!(
        parent
            .as_str()
            .starts_with("impl:sample_lib:inherent:Greeter:")
    );
    // And a `contains` relation exists from the impl to the method.
    assert!(ingest.relations.iter().any(|r| {
        r.kind.as_str() == "contains"
            && r.from.as_str() == parent.as_str()
            && r.to.as_str() == greet_method.id.as_str()
    }));
}
