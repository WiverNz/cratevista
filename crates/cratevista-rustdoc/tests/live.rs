//! Gated end-to-end test: runs real `cargo +<nightly> rustdoc --output-format
//! json` on the tiny path-only fixture crate with the approved compatibility
//! tuple, then loads and normalizes it.
//!
//! Ignored by default (needs the pinned nightly installed); **no network**
//! (the fixture crate is path-only). Run with:
//!
//! ```text
//! cargo test -p cratevista-rustdoc --test live -- --ignored
//! ```

use std::path::PathBuf;

use cratevista_rustdoc::{RustdocOptions, RustdocPlan, RustdocTarget, RustdocTargetKind, ingest};
use cratevista_schema::EntityId;

fn fixture_crate_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path.push("sample_lib");
    path
}

#[test]
#[ignore = "requires the pinned nightly toolchain; run with --ignored"]
fn live_rustdoc_generates_and_normalizes() {
    let crate_dir = fixture_crate_dir();
    let target_dir = std::env::temp_dir().join("cratevista-live-rustdoc");
    let _ = std::fs::remove_dir_all(&target_dir);

    let plan = RustdocPlan {
        // The fixture crate is standalone; treat it as its own workspace.
        workspace_root: crate_dir.clone(),
        targets: vec![RustdocTarget {
            package_id: EntityId::package("sample_lib"),
            target_id: EntityId::target("sample_lib", "lib", "sample_lib"),
            package_name: "sample_lib".into(),
            target_name: "sample_lib".into(),
            crate_name: "sample_lib".into(),
            target_kind: RustdocTargetKind::Library,
            manifest_path: crate_dir.join("Cargo.toml"),
            package_root: crate_dir.clone(),
        }],
    };
    let options = RustdocOptions {
        target_dir: Some(target_dir),
        ..Default::default()
    };

    let result = ingest(&plan, &options).expect("live rustdoc ingest");
    assert_eq!(result.summary.documented_crate_count, 1);
    assert_eq!(result.summary.succeeded_target_count, 1);
    assert!(!result.summary.partial);
    assert_eq!(result.summary.compat.format_version, 60);
    assert_eq!(result.summary.compat.nightly, "nightly-2026-07-01");

    // The normalized crate carries the stable identities for graph linking.
    let summary = &result.crates[0];
    assert_eq!(summary.package_id.as_str(), "package:sample_lib");
    assert_eq!(
        summary.target_id.as_str(),
        "target:sample_lib:lib:sample_lib"
    );
    assert_eq!(
        summary.root_module_id.as_str(),
        "module:sample_lib::sample_lib"
    );
    // And the outcome references the same target id.
    assert_eq!(
        result.summary.targets[0].target_id.as_str(),
        "target:sample_lib:lib:sample_lib"
    );

    // The real output contains the fixture's own items.
    assert!(
        result
            .entities
            .iter()
            .any(|e| e.id.as_str() == "item:struct:sample_lib::Greeter")
    );

    // No absolute path leaks into the summary or entities.
    for entity in &result.entities {
        if let Some(source) = &entity.source {
            assert!(!source.path.as_str().contains(':'), "no drive-letter path");
        }
    }
}
