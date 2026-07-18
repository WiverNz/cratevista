//! CrateVista rustdoc JSON adapter.
//!
//! Executes a pre-resolved [`RustdocPlan`] by invoking `cargo rustdoc …
//! --output-format json` under a separately-pinned nightly, deserializes the
//! output via `rustdoc-types` behind a compatibility gate, and maps items into
//! deterministically-ordered `cratevista-schema` entities, relations, and
//! [`cratevista_schema::DocumentDiagnostic`]s, plus a thin per-crate companion
//! ([`CrateSummary`]) of unresolved cross-crate references for issue 05.
//!
//! Package/target **selection** is owned by issue 05/core, not here: this crate
//! validates the plan but never enumerates packages or depends on
//! `cratevista-metadata`. Raw `rustdoc-types` never appear in a public
//! signature: [`normalize_json`]/[`load_and_normalize`] take JSON/paths, and the
//! raw layer (`load_raw`/`normalize_raw`) is crate-private.
//!
//! The workspace builds/tests/lints on stable Rust 1.97.1 (ADR-0010); `ingest`
//! invokes a separately-pinned nightly only at runtime. `normalize_json` and all
//! non-gated tests run on stable without nightly.
//!
//! See `PRD/issue_04_rustdoc_json.md` and `docs/adr/0004-rustdoc-toolchain-policy.md`.
#![forbid(unsafe_code)]

pub mod cache;
pub mod compat;
pub mod diagnostics;
pub mod error;
mod ids;
mod invoke;
mod load;
mod normalize;
pub mod options;
pub mod result;
mod spans;
mod toolchain;
mod types;

pub use cache::cache_key;
pub use error::RustdocError;
pub use load::{load_and_normalize, normalize_json};
pub use options::{
    FeatureSelection, NetworkMode, NormalizeContext, RustdocOptions, RustdocPlan, RustdocTarget,
    RustdocTargetKind,
};
pub use result::{
    CompatibilityTuple, CrateIngest, CrateSummary, RustdocIngest, RustdocSummary, TargetOutcome,
    TypeReferenceRole, UnresolvedTypeRef,
};

use cratevista_schema::{DocumentDiagnostic, Entity, Relation};

use crate::diagnostics::{code, warn};

/// Executes `plan` and returns the aggregated normalized result.
///
/// Validates the plan (paths under `workspace_root`; no duplicate targets) and
/// options, resolves the toolchain, then documents each target **sequentially**:
/// invoke → load → normalize → aggregate. Default behavior fails on any failed
/// target; `options.keep_going` turns per-target failures into `target_failed`
/// diagnostics and marks the summary `partial`. A run where no target succeeds
/// is fatal even under keep-going.
pub fn ingest(plan: &RustdocPlan, options: &RustdocOptions) -> Result<RustdocIngest, RustdocError> {
    plan.validate()?;
    options.validate()?;

    let toolchain = toolchain::resolve_toolchain(options);
    let compat = CompatibilityTuple::current(toolchain.clone());
    let dir = invoke::target_dir(plan, options);

    let mut entities: Vec<Entity> = Vec::new();
    let mut relations: Vec<Relation> = Vec::new();
    let mut diagnostics: Vec<DocumentDiagnostic> = Vec::new();
    let mut crates: Vec<CrateSummary> = Vec::new();
    let mut outcomes: Vec<TargetOutcome> = Vec::new();

    for target in &plan.targets {
        let context = NormalizeContext {
            workspace_root: plan.workspace_root.clone(),
            package_root: target.package_root.clone(),
            package_id: target.package_id.clone(),
            target_id: target.target_id.clone(),
            package_name: target.package_name.clone(),
            crate_name: target.crate_name.clone(),
            target_name: target.target_name.clone(),
            target_kind: target.target_kind.clone(),
            toolchain: toolchain.clone(),
        };

        let result = document_and_normalize(&toolchain, target, options, &dir, &context);
        match result {
            Ok(ingest) => {
                entities.extend(ingest.entities);
                relations.extend(ingest.relations);
                diagnostics.extend(ingest.diagnostics);
                crates.push(ingest.summary);
                outcomes.push(outcome(target, true));
            }
            Err(error) => {
                if options.keep_going {
                    diagnostics.push(warn(
                        code::TARGET_FAILED,
                        format!(
                            "target `{}::{}` failed: {error}",
                            target.package_name, target.target_name
                        ),
                        None,
                    ));
                    outcomes.push(outcome(target, false));
                } else {
                    return Err(error);
                }
            }
        }
    }

    let succeeded = outcomes.iter().filter(|o| o.succeeded).count();
    if succeeded == 0 {
        return Err(RustdocError::NoTargetSucceeded);
    }
    let failed = outcomes.len() - succeeded;

    // Determinism: aggregate ordering across crates.
    entities.sort_by(|a, b| a.id.cmp(&b.id));
    entities.dedup_by(|a, b| a.id == b.id);
    relations.sort_by(|a, b| a.id.cmp(&b.id));
    relations.dedup_by(|a, b| a.id == b.id);
    diagnostics.sort();

    let summary = RustdocSummary {
        documented_crate_count: crates.len(),
        entity_count: entities.len(),
        relation_count: relations.len(),
        succeeded_target_count: succeeded,
        failed_target_count: failed,
        partial: failed > 0,
        include_private: options.include_private,
        features: options.normalized_features(),
        network: options.network,
        compat,
        targets: outcomes,
    };

    Ok(RustdocIngest {
        crates,
        entities,
        relations,
        diagnostics,
        summary,
    })
}

/// Documents one target and normalizes its JSON. A `RustdocTargetKind::Other`
/// is a fatal `UnsupportedTargetKind` here; `ingest` downgrades it (like any
/// other target failure) to a diagnostic under keep-going.
fn document_and_normalize(
    toolchain: &str,
    target: &RustdocTarget,
    options: &RustdocOptions,
    dir: &std::path::Path,
    context: &NormalizeContext,
) -> Result<CrateIngest, RustdocError> {
    if matches!(target.target_kind, RustdocTargetKind::Other(_)) {
        return Err(RustdocError::UnsupportedTargetKind(
            target.target_kind.as_str().to_string(),
        ));
    }
    let path = invoke::document_target(toolchain, target, options, dir)?;
    load_and_normalize(&path, context)
}

fn outcome(target: &RustdocTarget, succeeded: bool) -> TargetOutcome {
    TargetOutcome {
        target_id: target.target_id.clone(),
        package_name: target.package_name.clone(),
        target_name: target.target_name.clone(),
        target_kind: target.target_kind.clone(),
        succeeded,
    }
}
