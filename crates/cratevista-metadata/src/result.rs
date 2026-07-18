//! The normalized ingestion result and its compact summary.

use cratevista_schema::{DocumentDiagnostic, Entity, Relation};

use crate::options::{ExternalDepsMode, PackageSelection};

/// The deterministic, normalized output of metadata ingestion.
///
/// It never contains an `ExplorerDocument`, a `cratevista_core::Diagnostic`,
/// serialized JSON, filesystem output paths, or UI coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct MetadataIngest {
    /// Entities (workspace, packages, targets), sorted by id.
    pub entities: Vec<Entity>,
    /// Relations (`contains`, `depends_on`), sorted by id.
    pub relations: Vec<Relation>,
    /// Recoverable diagnostics, sorted.
    pub diagnostics: Vec<DocumentDiagnostic>,
    /// A compact ingestion summary.
    pub summary: MetadataSummary,
}

/// A compact summary of a metadata ingestion run: counts and selection context
/// only. Per-package feature detail lives on the package entities.
#[derive(Debug, Clone, PartialEq)]
pub struct MetadataSummary {
    /// The workspace root as a repository-relative string (`"."` for the root).
    pub workspace_root_repo_relative: Option<String>,
    /// The package selection used.
    pub selection: PackageSelection,
    /// The external-dependency mode used.
    pub external_deps_mode: ExternalDepsMode,
    /// Number of workspace member packages.
    pub workspace_package_count: usize,
    /// Number of packages emitted as entities.
    pub selected_package_count: usize,
    /// Number of external packages emitted as entities.
    pub external_package_count: usize,
    /// Number of target entities emitted.
    pub target_count: usize,
    /// Number of `depends_on` relations emitted.
    pub dependency_relation_count: usize,
    /// Number of recoverable diagnostics emitted.
    pub recoverable_diagnostic_count: usize,
    /// The exact effective Cargo argv (no absolute paths leak into entities).
    pub cargo_argv: Vec<String>,
}
