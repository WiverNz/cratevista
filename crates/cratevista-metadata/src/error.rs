//! Fatal ingestion errors.

/// A fatal error that prevents producing a trustworthy [`crate::MetadataIngest`].
///
/// Each variant maps to a stable code (see [`MetadataError::code`]). Recoverable
/// problems are `cratevista_schema::DocumentDiagnostic`s in the result instead.
#[derive(Debug)]
pub enum MetadataError {
    /// The `cargo` executable could not be found or executed.
    CargoNotFound(String),
    /// `cargo metadata` ran but failed (non-zero exit).
    CargoMetadataFailed {
        /// The effective argv.
        argv: Vec<String>,
        /// The captured stderr tail.
        stderr: String,
    },
    /// The metadata output could not be parsed.
    MalformedMetadata(String),
    /// An explicitly selected package name is absent from the workspace.
    PackageNotFound(String),
    /// The provided options are internally contradictory.
    InvalidOptions(String),
    /// An internal invariant was violated (prevents deterministic output).
    InternalInvariant(String),
}

impl MetadataError {
    /// The stable diagnostic code for this error.
    pub fn code(&self) -> &'static str {
        match self {
            MetadataError::CargoNotFound(_) => "cargo_not_found",
            MetadataError::CargoMetadataFailed { .. } => "cargo_metadata_failed",
            MetadataError::MalformedMetadata(_) => "malformed_metadata",
            MetadataError::PackageNotFound(_) => "package_not_found",
            MetadataError::InvalidOptions(_) => "invalid_options",
            MetadataError::InternalInvariant(_) => "internal_invariant",
        }
    }

    /// A short, actionable remediation hint.
    pub fn remediation(&self) -> Option<&'static str> {
        match self {
            MetadataError::CargoNotFound(_) => {
                Some("Install Rust and Cargo from https://rustup.rs/.")
            }
            MetadataError::PackageNotFound(_) => Some(
                "Check the package name, or run without --package to include the whole workspace.",
            ),
            MetadataError::CargoMetadataFailed { .. } => {
                Some("Run `cargo metadata` manually to see the underlying Cargo error.")
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetadataError::CargoNotFound(msg) => write!(f, "cargo executable not available: {msg}"),
            MetadataError::CargoMetadataFailed { argv, stderr } => write!(
                f,
                "`{}` failed: {}",
                argv.join(" "),
                stderr.lines().last().unwrap_or("")
            ),
            MetadataError::MalformedMetadata(msg) => write!(f, "malformed cargo metadata: {msg}"),
            MetadataError::PackageNotFound(name) => {
                write!(f, "selected package `{name}` not found in the workspace")
            }
            MetadataError::InvalidOptions(msg) => write!(f, "invalid metadata options: {msg}"),
            MetadataError::InternalInvariant(msg) => {
                write!(f, "internal invariant violated: {msg}")
            }
        }
    }
}

impl std::error::Error for MetadataError {}
