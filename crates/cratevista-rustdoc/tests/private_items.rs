//! Public vs private-item modes, via the two checked-in fixtures.

mod common;

use common::sample_context;
use cratevista_rustdoc::normalize_json;

fn ingest(fixture: &str) -> cratevista_rustdoc::CrateIngest {
    let json = common::fixture(fixture);
    normalize_json(&json, &sample_context()).expect("normalize")
}

#[test]
fn private_method_only_appears_in_private_mode() {
    let public = ingest("sample_lib");
    let private = ingest("sample_lib_private");

    let has_private_method = |ingest: &cratevista_rustdoc::CrateIngest| {
        ingest
            .entities
            .iter()
            .any(|e| e.kind.as_str() == "method" && e.label.default == "private_len")
    };

    assert!(
        !has_private_method(&public),
        "public docs omit the private method"
    );
    assert!(
        has_private_method(&private),
        "private docs include the private method"
    );
    // The private crate documents at least as many entities.
    assert!(private.entities.len() >= public.entities.len());
}

#[test]
fn both_modes_are_absolute_path_free() {
    for fixture in ["sample_lib", "sample_lib_private"] {
        let ingest = ingest(fixture);
        common::assert_no_absolute_paths(&ingest);
    }
}
