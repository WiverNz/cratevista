//! Integration tests over the checked-in `sample_lib` rustdoc JSON fixture,
//! exercised through the public `normalize_json` API (no nightly, no network).

mod common;

use common::{assert_no_absolute_paths, entity, has_entity, relations_of_kind, sample_context};
use cratevista_rustdoc::normalize_json;

fn ingest() -> cratevista_rustdoc::CrateIngest {
    let json = common::fixture("sample_lib");
    normalize_json(&json, &sample_context()).expect("normalize sample_lib")
}

#[test]
fn crate_summary_reports_the_pinned_format() {
    let ingest = ingest();
    assert_eq!(ingest.summary.crate_name, "sample_lib");
    assert_eq!(ingest.summary.format_version, 60);
    assert_eq!(ingest.summary.toolchain, "nightly-2026-07-01");
    assert_eq!(ingest.summary.entity_count, ingest.entities.len());
}

#[test]
fn maps_the_core_items() {
    let ingest = ingest();
    // Root module.
    assert!(has_entity(&ingest, "module:sample_lib::sample_lib"));
    // Struct + trait + free function + submodule + nested function.
    assert!(has_entity(&ingest, "item:struct:sample_lib::Greeter"));
    assert!(has_entity(&ingest, "item:trait:sample_lib::Greetable"));
    assert!(has_entity(&ingest, "item:function:sample_lib::answer"));
    assert!(has_entity(&ingest, "module:sample_lib::util"));
    assert!(has_entity(
        &ingest,
        "item:function:sample_lib::util::double"
    ));
    // The struct field.
    assert!(has_entity(&ingest, "item:field:sample_lib::Greeter::name"));
}

#[test]
fn structs_have_impls_and_methods() {
    let ingest = ingest();
    // Two impls for Greeter: inherent + Greetable. Each has a distinct id.
    let impls: Vec<_> = ingest
        .entities
        .iter()
        .filter(|e| e.kind.as_str() == "impl")
        .collect();
    assert!(impls.len() >= 2, "expected >=2 impls, got {}", impls.len());
    assert!(
        impls
            .iter()
            .all(|e| e.id.as_str().starts_with("impl:sample_lib:")),
        "impl ids use the schema impl_block constructor"
    );

    // Methods (new/greet on the inherent impl, greeting on the trait impl).
    let methods: Vec<_> = ingest
        .entities
        .iter()
        .filter(|e| e.kind.as_str() == "method")
        .collect();
    let names: Vec<&str> = methods.iter().map(|e| e.label.default.as_str()).collect();
    assert!(names.contains(&"new"));
    assert!(names.contains(&"greet"));
    assert!(names.contains(&"greeting"));
}

#[test]
fn impl_relations_are_intra_crate() {
    let ingest = ingest();
    // The trait impl relates to both the trait and the self type.
    let implements = relations_of_kind(&ingest, "implements");
    assert!(
        implements
            .iter()
            .any(|r| r.to.as_str() == "item:trait:sample_lib::Greetable"),
        "an impl should `implements` the local trait"
    );
    let implemented_for = relations_of_kind(&ingest, "implemented_for");
    assert!(
        implemented_for
            .iter()
            .any(|r| r.to.as_str() == "item:struct:sample_lib::Greeter"),
        "an impl should be `implemented_for` the local struct"
    );
}

#[test]
fn documented_items_carry_doc_blocks() {
    let ingest = ingest();
    let greeter = entity(&ingest, "item:struct:sample_lib::Greeter");
    let docs = greeter.docs.as_ref().expect("Greeter is documented");
    assert!(docs.documented);
    assert!(docs.markdown.contains("greeter"));
    assert_eq!(greeter.attributes.get("visibility").unwrap(), "public");
}

#[test]
fn spans_are_repo_relative_and_no_absolute_paths_leak() {
    let ingest = ingest();
    let greeter = entity(&ingest, "item:struct:sample_lib::Greeter");
    let source = greeter.source.as_ref().expect("Greeter has a source span");
    assert_eq!(
        source.path.as_str(),
        "crates/cratevista-rustdoc/tests/fixtures/sample_lib/src/lib.rs"
    );
    assert!(source.span.is_some());
    assert_no_absolute_paths(&ingest);
}

#[test]
fn external_types_are_unresolved_not_invented() {
    let ingest = ingest();
    // `name: String` and `-> String` reference alloc::string::String (external):
    // preserved as unresolved refs, never emitted as has_field_type/returns_type.
    assert!(
        ingest
            .summary
            .unresolved_refs
            .iter()
            .any(|r| r.display.contains("String")),
        "String references should be preserved as unresolved"
    );
    // No relation points at a non-existent String entity.
    assert!(!has_entity(&ingest, "item:struct:sample_lib::String"));
}

#[test]
fn enums_variants_aliases_consts_statics_macros() {
    let ingest = ingest();
    assert!(has_entity(&ingest, "item:enum:sample_lib::Greeting"));
    assert!(has_entity(
        &ingest,
        "item:variant:sample_lib::Greeting::Hello"
    ));
    assert!(has_entity(
        &ingest,
        "item:variant:sample_lib::Greeting::Named"
    ));
    assert!(has_entity(
        &ingest,
        "item:field:sample_lib::Greeting::Named::who"
    ));
    assert!(has_entity(&ingest, "item:type_alias:sample_lib::Name"));
    assert!(has_entity(&ingest, "item:constant:sample_lib::GREETING"));
    assert!(has_entity(&ingest, "item:static:sample_lib::COUNT"));
    assert!(has_entity(&ingest, "item:macro:sample_lib::shout"));
}

#[test]
fn intra_crate_field_and_param_and_error_edges() {
    let ingest = ingest();
    let has_field = relations_of_kind(&ingest, "has_field_type");
    assert!(has_field.iter().any(|r| {
        r.from.as_str() == "item:field:sample_lib::Pair::greeter"
            && r.to.as_str() == "item:struct:sample_lib::Greeter"
    }));
    let accepts = relations_of_kind(&ingest, "accepts_type");
    assert!(accepts.iter().any(|r| {
        r.from.as_str() == "item:function:sample_lib::describe"
            && r.to.as_str() == "item:struct:sample_lib::Pair"
    }));
    let errors = relations_of_kind(&ingest, "error_type");
    assert!(errors.iter().any(|r| {
        r.from.as_str() == "item:function:sample_lib::try_build"
            && r.to.as_str() == "item:enum:sample_lib::BuildError"
    }));
}

#[test]
fn reexport_makes_one_canonical_entity_plus_relation() {
    let ingest = ingest();
    // The canonical Helper entity lives at its true path.
    assert!(has_entity(&ingest, "item:struct:sample_lib::util::Helper"));
    // No duplicate entity at the re-export site.
    assert!(!has_entity(&ingest, "item:struct:sample_lib::Helper"));
    // Exactly one re_exports relation from the root module to the canonical entity.
    let reexports = relations_of_kind(&ingest, "re_exports");
    assert_eq!(
        reexports
            .iter()
            .filter(|r| r.to.as_str() == "item:struct:sample_lib::util::Helper")
            .count(),
        1
    );
    // The alias is recorded on the canonical entity.
    let helper = entity(&ingest, "item:struct:sample_lib::util::Helper");
    assert!(helper.attributes.contains_key("aliases"));
}

#[test]
fn synthetic_impls_are_marked() {
    let ingest = ingest();
    // Auto-trait / blanket impls (Send, Sync, From, …) are marked synthetic so
    // issue 07 can exclude them from default views.
    let synthetic = ingest
        .entities
        .iter()
        .filter(|e| e.kind.as_str() == "impl")
        .filter(|e| e.attributes.contains_key("synthetic"))
        .count();
    assert!(synthetic > 0, "expected some synthetic impls to be marked");
    // The user's own inherent/trait impls are NOT synthetic.
    let inherent = ingest
        .entities
        .iter()
        .find(|e| e.id.as_str().contains(":inherent:Greeter:"))
        .expect("inherent Greeter impl");
    assert!(!inherent.attributes.contains_key("synthetic"));
}

#[test]
fn function_signature_attributes_present() {
    let ingest = ingest();
    let try_build = entity(&ingest, "item:function:sample_lib::try_build");
    assert!(try_build.attributes.contains_key("inputs"));
    assert_eq!(
        try_build.attributes.get("output").unwrap(),
        "Result<Greeter, BuildError>"
    );
    assert_eq!(
        try_build.attributes.get("is_result").unwrap(),
        &serde_json::Value::Bool(true)
    );
}

#[test]
fn output_is_deterministic() {
    let a = ingest();
    let b = ingest();
    assert_eq!(a.entities, b.entities);
    assert_eq!(a.relations, b.relations);
    assert_eq!(a.diagnostics, b.diagnostics);
    // Ordering is by id.
    let ids: Vec<_> = a.entities.iter().map(|e| e.id.clone()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
}
