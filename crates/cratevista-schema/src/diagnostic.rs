//! The generated-document diagnostic contract written to `diagnostics.json`.
//!
//! [`DocumentDiagnostic`] is a distinct type from the runtime CLI diagnostic
//! (`cratevista_core::Diagnostic`); the conversion from this type into the
//! runtime diagnostic is owned by `cratevista-core` in a later issue. Diagnostics
//! are **not** embedded in [`crate::document::ExplorerDocument`]; they are their
//! own top-level artifact.

use std::num::NonZeroU64;

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
    /// How many underlying occurrences this record represents.
    ///
    /// An ordinary diagnostic is `1`; an **aggregated** summary (e.g. one
    /// `external_crate_reference` per external crate) carries the exact represented
    /// count so a reader never mistakes N records for N underlying issues. Always
    /// positive: a `NonZeroU64` makes zero unrepresentable and rejects `0` at the
    /// deserialization boundary. Additive and backward-compatible: it is omitted
    /// when `1`, and a missing field deserializes to `1`.
    #[serde(
        default = "one_occurrence",
        skip_serializing_if = "is_single_occurrence"
    )]
    pub occurrence_count: NonZeroU64,
    /// Referenced entities, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntityId>,
    /// Referenced relations, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationId>,
}

/// The default (and minimum) occurrence count: one.
fn one_occurrence() -> NonZeroU64 {
    NonZeroU64::MIN
}

fn is_single_occurrence(count: &NonZeroU64) -> bool {
    *count == NonZeroU64::MIN
}

impl DocumentDiagnostic {
    /// Builds a diagnostic with no entity/relation references, representing one
    /// occurrence.
    pub fn new(severity: Severity, code: impl Into<String>, message: impl Into<String>) -> Self {
        DocumentDiagnostic {
            severity,
            code: code.into(),
            message: message.into(),
            occurrence_count: one_occurrence(),
            entities: Vec::new(),
            relations: Vec::new(),
        }
    }

    /// Sets how many underlying occurrences this (aggregated) diagnostic represents.
    /// `count` is clamped to at least one, so the invariant "never zero" holds even
    /// for a caller that computed an empty aggregate.
    #[must_use]
    pub fn representing(mut self, count: u64) -> Self {
        self.occurrence_count = NonZeroU64::new(count).unwrap_or(NonZeroU64::MIN);
        self
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_diagnostic_represents_one_occurrence() {
        let d = DocumentDiagnostic::new(Severity::Warning, "some_code", "a message");
        assert_eq!(d.occurrence_count.get(), 1);
    }

    #[test]
    fn an_aggregated_diagnostic_carries_the_exact_count() {
        let d = DocumentDiagnostic::new(Severity::Info, "external_crate_reference", "…")
            .representing(1924);
        assert_eq!(d.occurrence_count.get(), 1924);
    }

    #[test]
    fn representing_zero_is_normalized_to_one() {
        // An empty aggregate can never mean "zero occurrences".
        let d = DocumentDiagnostic::new(Severity::Info, "c", "m").representing(0);
        assert_eq!(d.occurrence_count.get(), 1);
    }

    #[test]
    fn a_count_of_one_is_omitted_from_serialization() {
        let d = DocumentDiagnostic::new(Severity::Warning, "c", "m");
        let json = serde_json::to_string(&d).unwrap();
        assert!(
            !json.contains("occurrence_count"),
            "an ordinary diagnostic omits the field: {json}"
        );
    }

    #[test]
    fn an_aggregated_count_is_serialized_and_round_trips() {
        let d = DocumentDiagnostic::new(Severity::Info, "c", "m").representing(42);
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"occurrence_count\":42"), "{json}");
        let back: DocumentDiagnostic = serde_json::from_str(&json).unwrap();
        assert_eq!(back.occurrence_count.get(), 42);
    }

    #[test]
    fn a_missing_occurrence_count_defaults_to_one() {
        // Backward compatibility: a pre-field diagnostic still deserializes.
        let json = r#"{"severity":"info","code":"c","message":"m"}"#;
        let d: DocumentDiagnostic = serde_json::from_str(json).unwrap();
        assert_eq!(d.occurrence_count.get(), 1);
    }

    #[test]
    fn an_explicit_zero_occurrence_count_is_rejected_at_the_boundary() {
        // `NonZeroU64` refuses `0` on deserialization — zero is unrepresentable.
        let json = r#"{"severity":"info","code":"c","message":"m","occurrence_count":0}"#;
        assert!(serde_json::from_str::<DocumentDiagnostic>(json).is_err());
    }
}
