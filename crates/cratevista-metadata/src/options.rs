//! Library-level options controlling Cargo metadata ingestion.

use std::path::PathBuf;

use crate::error::MetadataError;

/// Which packages to include as the "focus" of ingestion. Applied in-process
/// after Cargo metadata is received (`cargo metadata` has no `--package` flag).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PackageSelection {
    /// All workspace members (external deps per [`ExternalDepsMode`]).
    #[default]
    Default,
    /// All workspace members explicitly.
    Workspace,
    /// A specific set of member packages by name.
    Packages(Vec<String>),
}

/// Feature selection forwarded to `cargo metadata`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeatureSelection {
    /// Named features to enable (`--features a,b`).
    pub features: Vec<String>,
    /// Enable all features (`--all-features`).
    pub all_features: bool,
    /// Disable default features (`--no-default-features`).
    pub no_default_features: bool,
}

/// How much of the external dependency graph to include.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExternalDepsMode {
    /// Members + their targets + intra-workspace deps only (default).
    #[default]
    Exclude,
    /// Also include direct external dependencies of workspace members.
    DirectOnly,
    /// Include the entire resolved package graph.
    FullGraph,
}

/// Which non-default target kinds to include. `lib`, `bin`, and `proc-macro`
/// are always included; these toggle the opt-in kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TargetKinds {
    /// Include `example` targets.
    pub example: bool,
    /// Include integration `test` targets.
    pub test: bool,
    /// Include `bench` targets.
    pub bench: bool,
    /// Include `custom-build` (build script) targets. Never executed.
    pub build_script: bool,
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

/// Options controlling one metadata ingestion run.
#[derive(Debug, Clone, Default)]
pub struct MetadataOptions {
    /// Path to the `Cargo.toml` to analyze.
    pub manifest_path: Option<PathBuf>,
    /// Working directory to run Cargo in (else the process cwd).
    pub cwd: Option<PathBuf>,
    /// Package selection.
    pub selection: PackageSelection,
    /// Feature selection.
    pub features: FeatureSelection,
    /// External dependency mode.
    pub external_deps: ExternalDepsMode,
    /// Opt-in target kinds.
    pub target_kinds: TargetKinds,
    /// Cargo network mode.
    pub network: NetworkMode,
}

impl MetadataOptions {
    /// Validates internally contradictory option combinations before invoking
    /// Cargo. `NetworkMode` is an enum (exclusive by construction); the only
    /// conflicting feature combination is `all_features` with explicit
    /// `features`.
    pub fn validate(&self) -> Result<(), MetadataError> {
        if self.features.all_features && !self.features.features.is_empty() {
            return Err(MetadataError::InvalidOptions(
                "`all_features` cannot be combined with explicit `features`".to_string(),
            ));
        }
        Ok(())
    }
}
