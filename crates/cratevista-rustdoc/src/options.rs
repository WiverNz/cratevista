//! The execution plan, per-run options, and the pure-normalization context.
//!
//! Package/target **selection** is not owned here: the orchestrator (issue
//! 05/core) resolves it from `MetadataIngest` and hands this crate a concrete,
//! pre-resolved [`RustdocPlan`]. `cratevista-rustdoc` validates the plan but
//! never enumerates packages or reconstructs Cargo topology.

use std::path::{Path, PathBuf};

use cratevista_schema::EntityId;

use crate::error::RustdocError;

/// The kind of a target to document. Open-ended: unknown kinds are handled
/// safely (fatal `unsupported_target_kind` in fail-fast, recoverable under
/// keep-going) rather than crashing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustdocTargetKind {
    /// A `lib` target (documented via `--lib`).
    Library,
    /// A `proc-macro` target (a library with `proc-macro = true`; `--lib`).
    ProcMacro,
    /// A `bin` target (documented via `--bin <name>`; opt-in by the orchestrator).
    Binary,
    /// Any other/unknown target kind.
    Other(String),
}

impl RustdocTargetKind {
    /// A stable string form (used in plan-validation keys and diagnostics).
    pub fn as_str(&self) -> &str {
        match self {
            RustdocTargetKind::Library => "lib",
            RustdocTargetKind::ProcMacro => "proc-macro",
            RustdocTargetKind::Binary => "bin",
            RustdocTargetKind::Other(kind) => kind,
        }
    }
}

/// Feature selection forwarded to `cargo rustdoc`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeatureSelection {
    /// Named features to enable (`--features a,b`).
    pub features: Vec<String>,
    /// Enable all features (`--all-features`).
    pub all_features: bool,
    /// Disable default features (`--no-default-features`).
    pub no_default_features: bool,
}

/// Cargo network mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetworkMode {
    /// Inherit the environment (no extra flag).
    #[default]
    Inherit,
    /// `--offline`.
    Offline,
    /// `--frozen`.
    Frozen,
    /// `--locked`.
    Locked,
}

/// One concrete target the orchestrator (issue 05/core) selected to document.
///
/// Carries the **stable `cratevista-schema` identities** the planner already
/// knows (`package_id`, `target_id`) so the graph builder can link every
/// normalized rustdoc crate back to exactly one Cargo target without
/// reconstructing ownership from strings. `crate_name` is the target's **actual**
/// Cargo crate name (from target metadata), not a value guessed by blindly
/// replacing `-` with `_`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustdocTarget {
    /// The metadata **package** entity id (`package:{name}`). Same id emitted by
    /// `cratevista-metadata`.
    pub package_id: EntityId,
    /// The metadata **target** entity id (`target:{package}:{kind}:{name}`). Same
    /// id emitted by `cratevista-metadata`.
    pub target_id: EntityId,
    /// The Cargo package name.
    pub package_name: String,
    /// The Cargo target name.
    pub target_name: String,
    /// The actual Cargo target crate name (used for the invocation, output
    /// discovery, and normalized entity ids).
    pub crate_name: String,
    /// The kind of target to document.
    pub target_kind: RustdocTargetKind,
    /// The package manifest (used to build the `cargo rustdoc` invocation).
    pub manifest_path: PathBuf,
    /// The package directory (used to resolve relative spans).
    pub package_root: PathBuf,
}

/// The concrete, pre-resolved plan prepared by issue 05/core from `MetadataIngest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustdocPlan {
    /// The workspace root; every plan path must live under it.
    pub workspace_root: PathBuf,
    /// The concrete targets to document.
    pub targets: Vec<RustdocTarget>,
}

impl RustdocPlan {
    /// Validates the plan without any package discovery.
    ///
    /// Every `manifest_path`/`package_root` must be under `workspace_root`;
    /// `package_id`/`target_id` must be the right kind of schema id and carry no
    /// path separator (no absolute path may enter a public id); package/target/
    /// crate names must be non-empty; and no two targets may share a semantic
    /// `target_id`. An empty plan is *not* rejected here — a run that documents
    /// nothing is a fatal [`RustdocError::NoTargetSucceeded`] surfaced by `ingest`.
    pub fn validate(&self) -> Result<(), RustdocError> {
        let root = &self.workspace_root;
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for target in &self.targets {
            if !path_under(root, &target.manifest_path) {
                return Err(RustdocError::InvalidPlan(format!(
                    "manifest of `{}` is outside the workspace root",
                    target.package_name
                )));
            }
            if !path_under(root, &target.package_root) {
                return Err(RustdocError::InvalidPlan(format!(
                    "package root of `{}` is outside the workspace root",
                    target.package_name
                )));
            }
            if target.package_name.is_empty()
                || target.target_name.is_empty()
                || target.crate_name.is_empty()
            {
                return Err(RustdocError::InvalidPlan(
                    "package, target, and crate names must be non-empty".to_string(),
                ));
            }
            if !target.package_id.as_str().starts_with("package:") {
                return Err(RustdocError::InvalidPlan(format!(
                    "package_id `{}` is not a package entity id",
                    target.package_id
                )));
            }
            if !target.target_id.as_str().starts_with("target:") {
                return Err(RustdocError::InvalidPlan(format!(
                    "target_id `{}` is not a target entity id",
                    target.target_id
                )));
            }
            if id_has_path_separator(target.package_id.as_str())
                || id_has_path_separator(target.target_id.as_str())
            {
                return Err(RustdocError::InvalidPlan(format!(
                    "an absolute path leaked into an entity id for `{}`",
                    target.target_name
                )));
            }
            if !seen.insert(target.target_id.as_str().to_string()) {
                return Err(RustdocError::InvalidPlan(format!(
                    "duplicate target id `{}` in the plan",
                    target.target_id
                )));
            }
        }
        Ok(())
    }
}

/// Whether a public entity id string contains a path separator. Schema ids use
/// `:`/`::`/`@` and never `/` or `\\`, so a separator means an absolute path
/// leaked in.
fn id_has_path_separator(id: &str) -> bool {
    id.contains('/') || id.contains('\\')
}

/// Whether `candidate` is `root` or nested under it (purely lexical; both paths
/// are compared component-wise so no filesystem access or canonicalization is
/// required).
fn path_under(root: &Path, candidate: &Path) -> bool {
    let mut root_components = root.components();
    let mut candidate_components = candidate.components();
    loop {
        match root_components.next() {
            None => return true,
            Some(root_component) => match candidate_components.next() {
                Some(candidate_component) if candidate_component == root_component => continue,
                _ => return false,
            },
        }
    }
}

/// Options that are not per-target (features, private mode, toolchain, network).
#[derive(Debug, Clone, Default)]
pub struct RustdocOptions {
    /// Feature selection.
    pub features: FeatureSelection,
    /// Whether to pass `--document-private-items`.
    pub include_private: bool,
    /// Continue past a failed target, marking the result partial.
    pub keep_going: bool,
    /// Override toolchain; else the pinned/detected nightly.
    pub toolchain: Option<String>,
    /// Isolated rustdoc output directory (default `<workspace>/target/cratevista/rustdoc`).
    pub target_dir: Option<PathBuf>,
    /// Cargo network mode.
    pub network: NetworkMode,
}

impl RustdocOptions {
    /// Validates internally contradictory option combinations.
    pub fn validate(&self) -> Result<(), RustdocError> {
        if self.features.all_features && !self.features.features.is_empty() {
            return Err(RustdocError::InvalidPlan(
                "`all_features` cannot be combined with explicit `features`".to_string(),
            ));
        }
        Ok(())
    }

    /// The normalized (sorted) feature names for the summary.
    pub fn normalized_features(&self) -> Vec<String> {
        let mut features = self.features.features.clone();
        features.sort();
        features.dedup();
        features
    }
}

/// The context needed to purely normalize one crate's rustdoc JSON.
///
/// Carries **both** roots so `spans` can resolve absolute *and* relative
/// rustdoc `filename`s without any absolute path escaping into an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizeContext {
    /// The workspace root (spans are made repo-relative to this).
    pub workspace_root: PathBuf,
    /// The documented package's directory (relative spans resolve from here).
    pub package_root: PathBuf,
    /// The metadata **package** entity id (recorded in `CrateSummary`).
    pub package_id: EntityId,
    /// The metadata **target** entity id (recorded in `CrateSummary`).
    pub target_id: EntityId,
    /// The Cargo package name.
    pub package_name: String,
    /// The actual Cargo target crate name (drives normalized entity ids).
    pub crate_name: String,
    /// The target name.
    pub target_name: String,
    /// The kind of target documented.
    pub target_kind: RustdocTargetKind,
    /// The toolchain that produced the JSON (recorded in `CrateSummary`).
    pub toolchain: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(pkg: &str, name: &str, manifest: &str, root: &str) -> RustdocTarget {
        RustdocTarget {
            package_id: EntityId::package(pkg),
            target_id: EntityId::target(pkg, "lib", name),
            package_name: pkg.to_string(),
            target_name: name.to_string(),
            crate_name: name.replace('-', "_"),
            target_kind: RustdocTargetKind::Library,
            manifest_path: PathBuf::from(manifest),
            package_root: PathBuf::from(root),
        }
    }

    #[test]
    fn valid_plan_passes() {
        let plan = RustdocPlan {
            workspace_root: PathBuf::from("/w"),
            targets: vec![target("a", "a", "/w/a/Cargo.toml", "/w/a")],
        };
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn outside_workspace_is_invalid() {
        let plan = RustdocPlan {
            workspace_root: PathBuf::from("/w"),
            targets: vec![target("a", "a", "/elsewhere/Cargo.toml", "/elsewhere")],
        };
        assert_eq!(plan.validate().unwrap_err().code(), "invalid_plan");
    }

    #[test]
    fn duplicate_target_id_is_invalid() {
        // Same semantic target_id (same package/kind/name) → invalid, even though
        // the crate names below are identical.
        let plan = RustdocPlan {
            workspace_root: PathBuf::from("/w"),
            targets: vec![
                target("a", "a", "/w/a/Cargo.toml", "/w/a"),
                target("a", "a", "/w/a/Cargo.toml", "/w/a"),
            ],
        };
        assert_eq!(plan.validate().unwrap_err().code(), "invalid_plan");
    }

    #[test]
    fn lib_and_bin_same_crate_name_stay_distinct() {
        // A package `tool` with a lib `tool` and a bin `tool` share crate_name
        // `tool`, but their target_ids differ, so the plan is valid.
        let lib = RustdocTarget {
            package_id: EntityId::package("tool"),
            target_id: EntityId::target("tool", "lib", "tool"),
            package_name: "tool".into(),
            target_name: "tool".into(),
            crate_name: "tool".into(),
            target_kind: RustdocTargetKind::Library,
            manifest_path: PathBuf::from("/w/tool/Cargo.toml"),
            package_root: PathBuf::from("/w/tool"),
        };
        let bin = RustdocTarget {
            target_id: EntityId::target("tool", "bin", "tool"),
            target_kind: RustdocTargetKind::Binary,
            ..lib.clone()
        };
        let plan = RustdocPlan {
            workspace_root: PathBuf::from("/w"),
            targets: vec![lib, bin],
        };
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn path_bearing_id_is_invalid() {
        let mut bad = target("a", "a", "/w/a/Cargo.toml", "/w/a");
        bad.target_id = EntityId::from_raw("target:a:lib:/w/a/src");
        let plan = RustdocPlan {
            workspace_root: PathBuf::from("/w"),
            targets: vec![bad],
        };
        assert_eq!(plan.validate().unwrap_err().code(), "invalid_plan");
    }

    #[test]
    fn empty_crate_name_is_invalid() {
        let mut bad = target("a", "a", "/w/a/Cargo.toml", "/w/a");
        bad.crate_name = String::new();
        let plan = RustdocPlan {
            workspace_root: PathBuf::from("/w"),
            targets: vec![bad],
        };
        assert_eq!(plan.validate().unwrap_err().code(), "invalid_plan");
    }

    #[test]
    fn all_features_conflicts_with_explicit() {
        let options = RustdocOptions {
            features: FeatureSelection {
                features: vec!["x".into()],
                all_features: true,
                no_default_features: false,
            },
            ..Default::default()
        };
        assert!(options.validate().is_err());
    }
}
