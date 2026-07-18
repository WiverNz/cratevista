//! Fatal graph-builder errors.
//!
//! Everything recoverable is a `cratevista_schema::DocumentDiagnostic`; a
//! `GraphError` is returned only when no trustworthy document can be produced.

use cratevista_schema::SchemaError;

/// A fatal error that prevents producing a trustworthy `ExplorerDocument`.
#[derive(Debug)]
pub enum GraphError {
    /// There were no metadata entities at all (nothing to build from).
    EmptyInput(String),
    /// The assembled document failed schema validation — a builder bug; the
    /// invalid document is never returned or written.
    DocumentValidationFailed(Vec<SchemaError>),
    /// `build_rustdoc_plan` could not build a usable plan.
    Plan(String),
    /// An internal invariant was violated (a structural conflict with no
    /// trustworthy resolution).
    InternalInvariant(String),
}

impl GraphError {
    /// The stable diagnostic code for this error.
    pub fn code(&self) -> &'static str {
        match self {
            GraphError::EmptyInput(_) => "empty_input",
            GraphError::DocumentValidationFailed(_) => "document_validation_failed",
            GraphError::Plan(_) => "plan_failed",
            GraphError::InternalInvariant(_) => "internal_invariant",
        }
    }
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::EmptyInput(msg) => write!(f, "no graph input: {msg}"),
            GraphError::DocumentValidationFailed(errors) => {
                write!(f, "assembled document failed schema validation:")?;
                for error in errors {
                    write!(f, "\n  - {error}")?;
                }
                Ok(())
            }
            GraphError::Plan(msg) => write!(f, "could not build rustdoc plan: {msg}"),
            GraphError::InternalInvariant(msg) => write!(f, "internal invariant violated: {msg}"),
        }
    }
}

impl std::error::Error for GraphError {}
