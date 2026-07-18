//! The normalized ingestion results and their path-free summaries.
//!
//! Nothing here contains an `ExplorerDocument`, a `cratevista_core::Diagnostic`,
//! a `cratevista_metadata::MetadataIngest`, serialized artifact JSON, UI layout
//! values, or any absolute machine path. Raw argv lives only in `tracing` and
//! [`crate::RustdocError`] messages.

use cratevista_schema::{DocumentDiagnostic, Entity, EntityId, EntityKind, Relation};

use crate::compat::{ADAPTER_VERSION, EXPECTED_FORMAT_VERSION, RUSTDOC_TYPES_RELEASE};
use crate::options::{NetworkMode, RustdocTargetKind};

/// The verified compatibility tuple used for a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityTuple {
    /// The nightly toolchain that produced the JSON.
    pub nightly: String,
    /// The rustdoc JSON format version.
    pub format_version: u32,
    /// The `rustdoc-types` release line.
    pub rustdoc_types: String,
    /// The CrateVista adapter version.
    pub adapter: u32,
}

impl CompatibilityTuple {
    /// The tuple for a run that used `nightly`, with the compiled-in versions.
    pub fn current(nightly: impl Into<String>) -> Self {
        CompatibilityTuple {
            nightly: nightly.into(),
            format_version: EXPECTED_FORMAT_VERSION,
            rustdoc_types: RUSTDOC_TYPES_RELEASE.to_string(),
            adapter: ADAPTER_VERSION,
        }
    }
}

/// The **reliable** role of a preserved cross-crate type reference. These are the
/// only roles the graph builder (issue 05) resolves; approximate `references_type`
/// mentions remain deferred (PRD 04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeReferenceRole {
    /// A struct/union field type.
    Field,
    /// A function/method parameter type.
    Parameter,
    /// A function/method return type.
    Return,
    /// The error type of a `Result` return.
    Error,
    /// An associated type.
    AssociatedType,
    /// The self type of an `impl` block.
    ImplFor,
    /// The trait of a trait `impl`.
    ImplTrait,
}

impl TypeReferenceRole {
    /// The stable relation-id role string used when this reference resolves to a
    /// relation (kept identical to the intra-crate relation roles).
    pub fn relation_role(self) -> &'static str {
        match self {
            TypeReferenceRole::Field => "field",
            TypeReferenceRole::Parameter => "param",
            TypeReferenceRole::Return => "return",
            TypeReferenceRole::Error => "error",
            TypeReferenceRole::AssociatedType => "assoc",
            TypeReferenceRole::ImplFor => "impl_for",
            TypeReferenceRole::ImplTrait => "impl_trait",
        }
    }
}

/// An unresolved (cross-crate) type reference preserved for issue-05 resolution.
///
/// Never an invented edge: the target could not be resolved within this crate's
/// rustdoc index. The **structured** fields (`crate_name`/`canonical_path`/
/// `item_kind`) are the primary resolver key — extracted from rustdoc's
/// `ItemSummary`/path map, never by reparsing `display`. They are absent only
/// when rustdoc provides no structured evidence; `display` is retained for
/// presentation/debugging. No rustdoc numeric id and no absolute path ever
/// appear here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnresolvedTypeRef {
    /// The entity that referenced the type.
    pub from: EntityId,
    /// The reliable reference role.
    pub role: TypeReferenceRole,
    /// The referenced Rust crate name, when rustdoc records it.
    pub crate_name: Option<String>,
    /// The referenced item's canonical path components (as rustdoc's
    /// `ItemSummary.path`, crate segment first), when available. Normalized,
    /// deterministic strings.
    pub canonical_path: Option<Vec<String>>,
    /// The referenced item's kind, when rustdoc can determine it.
    pub item_kind: Option<EntityKind>,
    /// The normalized display text of the referenced type (presentation/debug
    /// only — not the primary resolver key).
    pub display: String,
}

/// The thin per-crate normalized companion for cross-crate resolution.
///
/// Carries the stable identities (`package_id`, `target_id`, `root_module_id`)
/// the graph builder needs to link this normalized crate to **exactly one** Cargo
/// target — without reconstructing ownership from `crate_name` alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateSummary {
    /// The metadata **package** entity id this crate was documented for.
    pub package_id: EntityId,
    /// The metadata **target** entity id this crate was documented for.
    pub target_id: EntityId,
    /// The actual normalized **root module** entity id of this crate.
    pub root_module_id: EntityId,
    /// The crate name.
    pub crate_name: String,
    /// The rustdoc JSON format version this crate was parsed at.
    pub format_version: u32,
    /// The toolchain that produced the JSON.
    pub toolchain: String,
    /// Number of entities emitted for this crate.
    pub entity_count: usize,
    /// Number of relations emitted for this crate.
    pub relation_count: usize,
    /// Preserved unresolved type references, sorted.
    pub unresolved_refs: Vec<UnresolvedTypeRef>,
}

/// The result of normalizing exactly one crate's rustdoc JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct CrateIngest {
    /// Schema entities, sorted by id.
    pub entities: Vec<Entity>,
    /// Schema relations, sorted by id.
    pub relations: Vec<Relation>,
    /// Recoverable diagnostics, sorted.
    pub diagnostics: Vec<DocumentDiagnostic>,
    /// The thin normalized companion.
    pub summary: CrateSummary,
}

/// The outcome of documenting one plan target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetOutcome {
    /// The metadata **target** entity id (coherent with `CrateSummary.target_id`
    /// for successful targets; lets issue 05 reference failed targets too).
    pub target_id: EntityId,
    /// The package name.
    pub package_name: String,
    /// The target name.
    pub target_name: String,
    /// The kind of target documented.
    pub target_kind: RustdocTargetKind,
    /// Whether the target was documented successfully.
    pub succeeded: bool,
}

/// The deterministic, normalized output of a whole plan ingestion.
#[derive(Debug, Clone, PartialEq)]
pub struct RustdocIngest {
    /// Per-crate normalized companions (unresolved refs, format, toolchain).
    pub crates: Vec<CrateSummary>,
    /// Schema entities across all crates, sorted by id.
    pub entities: Vec<Entity>,
    /// Schema relations across all crates, sorted by id.
    pub relations: Vec<Relation>,
    /// Recoverable diagnostics, sorted.
    pub diagnostics: Vec<DocumentDiagnostic>,
    /// A path-free summary.
    pub summary: RustdocSummary,
}

/// A **safe**, structured summary — no absolute machine paths.
#[derive(Debug, Clone, PartialEq)]
pub struct RustdocSummary {
    /// Number of crates documented.
    pub documented_crate_count: usize,
    /// Total entity count.
    pub entity_count: usize,
    /// Total relation count.
    pub relation_count: usize,
    /// Number of targets that succeeded.
    pub succeeded_target_count: usize,
    /// Number of targets that failed (only non-zero under keep-going).
    pub failed_target_count: usize,
    /// True when keep-going skipped at least one target.
    pub partial: bool,
    /// Whether private items were documented.
    pub include_private: bool,
    /// Normalized (sorted) feature names.
    pub features: Vec<String>,
    /// The network mode used.
    pub network: NetworkMode,
    /// The compatibility tuple used.
    pub compat: CompatibilityTuple,
    /// Per-target outcomes.
    pub targets: Vec<TargetOutcome>,
}
