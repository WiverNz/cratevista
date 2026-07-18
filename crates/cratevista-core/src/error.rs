//! The top-level error type for the application layer.

use std::path::PathBuf;

use crate::diagnostic::Diagnostic;

/// Errors produced by `cratevista-core` operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A path was not valid UTF-8. CrateVista requires UTF-8 paths.
    #[error("path is not valid UTF-8: {0}")]
    NonUtf8Path(PathBuf),

    /// An underlying I/O error.
    #[error("{context}: {source}")]
    Io {
        /// What was being attempted.
        context: String,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

impl CoreError {
    /// Wraps an I/O error with human context.
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        CoreError::Io {
            context: context.into(),
            source,
        }
    }

    /// Renders this error as a user-facing [`Diagnostic`].
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            CoreError::NonUtf8Path(path) => Diagnostic::error("non_utf8_path", self.to_string())
                .with_context("path", path.display().to_string())
                .with_remediation("CrateVista requires UTF-8 paths; rename or relocate the path."),
            CoreError::Io { .. } => Diagnostic::error("io_error", self.to_string()),
        }
    }
}
