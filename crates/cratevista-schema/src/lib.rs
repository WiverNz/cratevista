//! CrateVista explorer schema — the canonical, versioned, frontend-independent
//! domain model.
//!
//! This crate defines the three serialized artifacts and their types:
//!
//! - [`ExplorerDocument`] → `document.json` (deterministic; no timestamps, no
//!   embedded diagnostics);
//! - [`GenerationReport`] → `generation.json` (runtime metadata);
//! - [`DiagnosticsReport`] of [`DocumentDiagnostic`] → `diagnostics.json`.
//!
//! Entity/relation [`kind`]s are open string-backed values (unknown kinds
//! round-trip losslessly and get a generic frontend fallback). Stable
//! [`ids`] derive from names/canonical paths with BLAKE3 semantic
//! discriminators. Every artifact and the JSON Schema serialize through the one
//! [`canonical`] serializer. The [`DocumentDiagnostic`] here is distinct from the
//! runtime `cratevista_core::Diagnostic`; this crate does **not** depend on
//! `cratevista-core`.
//!
//! See `PRD/issue_02_explorer_schema.md` and `docs/adr/0003-schema-versioning.md`.
#![forbid(unsafe_code)]

pub mod canonical;
pub mod diagnostic;
pub mod docs;
pub mod document;
pub mod entity;
pub mod generation;
pub mod ids;
pub mod jsonschema;
pub mod kind;
pub mod relation;
pub mod source;
pub mod validate;
pub mod version;
pub mod view;

pub use diagnostic::{DiagnosticsReport, DocumentDiagnostic, Severity};
pub use docs::DocBlock;
pub use document::{ExplorerDocument, Project};
pub use entity::{AttrValue, Entity, LocalizedText, Provenance};
pub use generation::{ArtifactHashes, Counts, GenerationReport, Generator, Timestamp};
pub use ids::{EntityId, RelationId, StageId, ViewId, discriminator};
pub use kind::{EntityKind, RelationKind};
pub use relation::Relation;
pub use source::{RepoRelativePath, SourceLocation, SourcePathError, Span};
pub use validate::SchemaError;
pub use version::SchemaVersion;
pub use view::{Stage, View, ViewExample};
