//! The additive PRD-04 bridge contract exercised over the checked-in
//! `sample_lib` fixture through the public API (stable, no nightly, no network):
//! stable crate identities on `CrateSummary`, and structured cross-crate
//! `UnresolvedTypeRef`s. This file imports **no** `rustdoc_types` — the public
//! surface is entirely CrateVista-owned + `cratevista-schema` types.

mod common;

use common::{assert_no_absolute_paths, sample_context};
use cratevista_rustdoc::{TypeReferenceRole, normalize_json};

fn ingest() -> cratevista_rustdoc::CrateIngest {
    let json = common::fixture("sample_lib");
    normalize_json(&json, &sample_context()).expect("normalize sample_lib")
}

#[test]
fn crate_summary_carries_stable_identities() {
    let ingest = ingest();
    let s = &ingest.summary;
    assert_eq!(s.package_id.as_str(), "package:sample_lib");
    assert_eq!(s.target_id.as_str(), "target:sample_lib:lib:sample_lib");
    assert_eq!(s.root_module_id.as_str(), "module:sample_lib::sample_lib");
    // The root module id is a real emitted entity.
    assert!(
        ingest.entities.iter().any(|e| e.id == s.root_module_id),
        "root_module_id must reference an emitted entity"
    );
}

#[test]
fn external_reference_is_fully_structured() {
    let ingest = ingest();
    // The auto/blanket impls reference `core::any::Any` (a trait) — structured.
    let any = ingest
        .summary
        .unresolved_refs
        .iter()
        .find(|r| {
            r.role == TypeReferenceRole::ImplTrait
                && r.canonical_path
                    .as_ref()
                    .is_some_and(|p| p.last().map(String::as_str) == Some("Any"))
        })
        .expect("a structured `core::any::Any` impl_trait reference");
    assert_eq!(any.crate_name.as_deref(), Some("core"));
    assert_eq!(any.item_kind.as_ref().map(|k| k.as_str()), Some("trait"));
    assert_eq!(
        any.canonical_path.as_deref(),
        Some(&["core".to_string(), "any".to_string(), "Any".to_string()][..])
    );
}

#[test]
fn string_field_reference_is_structured() {
    let ingest = ingest();
    // `Greeter.name: String` → external `alloc::…::String`, preserved (never an
    // invented `has_field_type` edge).
    let string_ref = ingest
        .summary
        .unresolved_refs
        .iter()
        .find(|r| r.display.contains("String") && r.role == TypeReferenceRole::Field)
        .expect("a structured String field reference");
    assert_eq!(string_ref.crate_name.as_deref(), Some("alloc"));
    assert_eq!(
        string_ref.item_kind.as_ref().map(|k| k.as_str()),
        Some("struct")
    );
    assert!(
        string_ref
            .canonical_path
            .as_ref()
            .is_some_and(|p| p.last().map(String::as_str) == Some("String"))
    );
}

#[test]
fn references_are_deterministic_and_leak_no_numeric_ids_or_paths() {
    let a = ingest();
    let b = ingest();
    assert_eq!(a.summary.unresolved_refs, b.summary.unresolved_refs);
    // Sorted.
    let mut sorted = a.summary.unresolved_refs.clone();
    sorted.sort();
    assert_eq!(a.summary.unresolved_refs, sorted);
    // No absolute path in identities or references.
    assert_no_absolute_paths(&a);
    // Canonical path components are names, never numeric rustdoc ids.
    for reference in &a.summary.unresolved_refs {
        if let Some(path) = &reference.canonical_path {
            for component in path {
                assert!(
                    component.parse::<u64>().is_err(),
                    "canonical path component `{component}` looks like a numeric id"
                );
            }
        }
    }
}

#[test]
fn only_reliable_roles_are_preserved() {
    let ingest = ingest();
    // Every preserved reference is one of the reliable roles (no references_type).
    for reference in &ingest.summary.unresolved_refs {
        assert!(matches!(
            reference.role,
            TypeReferenceRole::Field
                | TypeReferenceRole::Parameter
                | TypeReferenceRole::Return
                | TypeReferenceRole::Error
                | TypeReferenceRole::AssociatedType
                | TypeReferenceRole::ImplFor
                | TypeReferenceRole::ImplTrait
        ));
    }
}
