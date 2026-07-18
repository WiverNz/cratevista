//! Plan validation and failure/partial policy that require **no** nightly and
//! **no** cargo invocation (undocumentable kinds and empty plans short-circuit
//! before any process is spawned).

use std::path::PathBuf;

use cratevista_rustdoc::{RustdocOptions, RustdocPlan, RustdocTarget, RustdocTargetKind, ingest};
use cratevista_schema::EntityId;

fn target(kind: RustdocTargetKind, manifest: &str, root: &str) -> RustdocTarget {
    RustdocTarget {
        package_id: EntityId::package("sample"),
        target_id: EntityId::target("sample", kind.as_str(), "sample"),
        package_name: "sample".into(),
        target_name: "sample".into(),
        crate_name: "sample".into(),
        target_kind: kind,
        manifest_path: PathBuf::from(manifest),
        package_root: PathBuf::from(root),
    }
}

#[test]
fn path_outside_workspace_is_invalid_plan() {
    let plan = RustdocPlan {
        workspace_root: PathBuf::from("/w"),
        targets: vec![target(
            RustdocTargetKind::Library,
            "/elsewhere/Cargo.toml",
            "/elsewhere",
        )],
    };
    let error = ingest(&plan, &RustdocOptions::default()).unwrap_err();
    assert_eq!(error.code(), "invalid_plan");
}

#[test]
fn duplicate_target_is_invalid_plan() {
    let plan = RustdocPlan {
        workspace_root: PathBuf::from("/w"),
        targets: vec![
            target(RustdocTargetKind::Library, "/w/Cargo.toml", "/w"),
            target(RustdocTargetKind::Library, "/w/Cargo.toml", "/w"),
        ],
    };
    let error = ingest(&plan, &RustdocOptions::default()).unwrap_err();
    assert_eq!(error.code(), "invalid_plan");
}

#[test]
fn empty_plan_is_no_target_succeeded() {
    let plan = RustdocPlan {
        workspace_root: PathBuf::from("/w"),
        targets: vec![],
    };
    let error = ingest(&plan, &RustdocOptions::default()).unwrap_err();
    assert_eq!(error.code(), "no_target_succeeded");
}

#[test]
fn unsupported_kind_is_fatal_in_fail_fast() {
    let plan = RustdocPlan {
        workspace_root: PathBuf::from("/w"),
        targets: vec![target(
            RustdocTargetKind::Other("cdylib-thing".into()),
            "/w/Cargo.toml",
            "/w",
        )],
    };
    let error = ingest(&plan, &RustdocOptions::default()).unwrap_err();
    assert_eq!(error.code(), "unsupported_target_kind");
}

#[test]
fn unsupported_kind_under_keep_going_yields_no_target_succeeded() {
    let plan = RustdocPlan {
        workspace_root: PathBuf::from("/w"),
        targets: vec![target(
            RustdocTargetKind::Other("cdylib-thing".into()),
            "/w/Cargo.toml",
            "/w",
        )],
    };
    let options = RustdocOptions {
        keep_going: true,
        ..Default::default()
    };
    // Every target failed → fatal even under keep-going.
    let error = ingest(&plan, &options).unwrap_err();
    assert_eq!(error.code(), "no_target_succeeded");
}
