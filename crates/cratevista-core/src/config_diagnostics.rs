//! Converting `cratevista_config::ConfigDiagnostic` into the schema's
//! `DocumentDiagnostic`, so configuration problems reach `diagnostics.json`
//! alongside metadata, rustdoc and graph problems.
//!
//! # Where the location goes, and why
//!
//! `DocumentDiagnostic` carries `{severity, code, message, entities, relations}`
//! — there is **no location field**. Configuration diagnostics do have a file
//! and often a line/column, and issue 08 requires reporting them, so the
//! location is prefixed onto the **message** in the conventional
//! `file:line:column: message` form that editors and humans already parse:
//!
//! ```text
//! .cratevista/flows/a.toml:12:3: duplicate entity id `redis`; already declared at …
//! ```
//!
//! The alternative — a structured `location` field on `DocumentDiagnostic` —
//! would be an additive **PRD-02 schema amendment** (`SchemaVersion` 1.1 → 1.2)
//! plus a PRD-07 renderer, and neither is authorized by PRD-08 step 6. The
//! **`code` stays a first-class field**, so machine consumers match on it
//! exactly as before; only the human-readable part gained a prefix.
//!
//! Paths are already workspace-relative and `/`-normalized when they leave
//! `cratevista-config`, so nothing here can introduce an absolute path.
//!
//! # Severity
//!
//! Every configuration problem is a **warning**, never an error: a broken
//! configuration must not fail generation. The valid parts still produce an
//! overlay, the discovered document is still committed, and the exit code stays
//! `0`. `Severity::Error` is reserved for problems that invalidate the document
//! itself.

use cratevista_config::ConfigDiagnostic;
use cratevista_schema::{DocumentDiagnostic, Severity};

/// Renders one configuration diagnostic as a document diagnostic.
pub fn to_document_diagnostic(diagnostic: &ConfigDiagnostic) -> DocumentDiagnostic {
    // `ConfigDiagnostic`'s Display is exactly `file[:line:col]: message`, which
    // is the one place that format is defined.
    DocumentDiagnostic::new(Severity::Warning, diagnostic.code, diagnostic.to_string())
}

/// Renders a whole batch, preserving input order (the caller sorts).
pub fn to_document_diagnostics(diagnostics: &[ConfigDiagnostic]) -> Vec<DocumentDiagnostic> {
    diagnostics.iter().map(to_document_diagnostic).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cratevista_config::Position;

    fn diagnostic(position: Option<Position>) -> ConfigDiagnostic {
        ConfigDiagnostic {
            code: cratevista_config::code::DUPLICATE_ENTITY_ID,
            message: "duplicate entity id `redis`".into(),
            file: ".cratevista/flows/b.toml".into(),
            position,
        }
    }

    #[test]
    fn the_code_stays_a_first_class_field() {
        let converted = to_document_diagnostic(&diagnostic(None));
        // Machine consumers match on `code`; it must not be folded into prose.
        assert_eq!(converted.code, "config_duplicate_entity_id");
        assert_eq!(converted.severity, Severity::Warning);
    }

    #[test]
    fn the_location_is_preserved_in_the_message() {
        let converted = to_document_diagnostic(&diagnostic(Some(Position {
            line: 12,
            column: 3,
        })));
        assert_eq!(
            converted.message,
            ".cratevista/flows/b.toml:12:3: duplicate entity id `redis`"
        );
    }

    #[test]
    fn a_diagnostic_without_a_position_still_names_its_file() {
        let converted = to_document_diagnostic(&diagnostic(None));
        assert_eq!(
            converted.message,
            ".cratevista/flows/b.toml: duplicate entity id `redis`"
        );
    }

    #[test]
    fn every_configuration_problem_is_a_warning_so_generation_survives() {
        // Even a parse error: a broken config costs its own contents, not the run.
        let parse_error = ConfigDiagnostic {
            code: cratevista_config::code::PARSE_ERROR,
            message: "invalid TOML".into(),
            file: ".cratevista/flows/a.toml".into(),
            position: Some(Position { line: 1, column: 1 }),
        };
        assert_eq!(
            to_document_diagnostic(&parse_error).severity,
            Severity::Warning
        );
    }

    #[test]
    fn a_batch_preserves_order_and_length() {
        let batch = vec![
            diagnostic(None),
            diagnostic(Some(Position { line: 2, column: 1 })),
        ];
        let converted = to_document_diagnostics(&batch);
        assert_eq!(converted.len(), 2);
        assert!(!converted[0].message.contains(":2:1"));
        assert!(converted[1].message.contains(":2:1"));
    }
}
