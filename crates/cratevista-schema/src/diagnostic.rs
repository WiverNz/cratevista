//! The generated-document diagnostic contract written to `diagnostics.json`.
//!
//! [`DocumentDiagnostic`] is a distinct type from the runtime CLI diagnostic
//! (`cratevista_core::Diagnostic`); the conversion from this type into the
//! runtime diagnostic is owned by `cratevista-core` in a later issue. Diagnostics
//! are **not** embedded in [`crate::document::ExplorerDocument`]; they are their
//! own top-level artifact.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{EntityId, RelationId};
use crate::version::SchemaVersion;

/// Severity of a [`DocumentDiagnostic`]. Ordered `Error < Warning < Info`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A fatal problem.
    Error,
    /// A non-fatal problem worth surfacing.
    Warning,
    /// Informational context.
    Info,
}

/// A diagnostic about the analyzed document/graph (unresolved types, dropped
/// spans, excluded externals, …).
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct DocumentDiagnostic {
    /// Severity.
    pub severity: Severity,
    /// A short, stable machine-readable code.
    pub code: String,
    /// The human-readable message.
    pub message: String,
    /// Referenced entities, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntityId>,
    /// Referenced relations, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationId>,
}

impl DocumentDiagnostic {
    /// Builds a diagnostic with no entity/relation references.
    pub fn new(severity: Severity, code: impl Into<String>, message: impl Into<String>) -> Self {
        DocumentDiagnostic {
            severity,
            code: code.into(),
            message: message.into(),
            entities: Vec::new(),
            relations: Vec::new(),
        }
    }
}

/// The `diagnostics.json` artifact: a versioned, deterministically-ordered list
/// of [`DocumentDiagnostic`]s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiagnosticsReport {
    /// Schema version of this artifact.
    pub schema_version: SchemaVersion,
    /// The diagnostics, sorted deterministically.
    pub diagnostics: Vec<DocumentDiagnostic>,
}

impl DiagnosticsReport {
    /// Builds a report, sorting the diagnostics into a deterministic order.
    pub fn new(mut diagnostics: Vec<DocumentDiagnostic>) -> Self {
        diagnostics.sort();
        DiagnosticsReport {
            schema_version: SchemaVersion::current(),
            diagnostics,
        }
    }
}
