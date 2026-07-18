//! Stable, path-free error types for the server.
//!
//! Every public error carries a stable machine-readable `code()` and never
//! embeds an absolute filesystem path (a leaked path would expose the user's
//! home directory / project location). `cratevista-core` maps these codes to
//! process exit codes and renders them as diagnostics.

/// A failure loading a consistent, integrity-verified artifact snapshot.
///
/// Messages are deliberately generic and **never** contain a filesystem path.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// One or more of the three artifact files is missing.
    #[error("one or more artifact files are missing")]
    ArtifactsMissing,
    /// An artifact file could not be read (permission / I/O error).
    #[error("could not read an artifact file: {0}")]
    ArtifactReadFailed(String),
    /// The completion marker changed on every read attempt (a generation kept
    /// landing mid-read); the retry budget was exhausted.
    #[error("the artifact set kept changing while being read")]
    ArtifactChangedDuringRead,
    /// `generation.json` has no `artifact_hashes` — a pre-amendment artifact set
    /// that cannot be integrity-verified.
    #[error("this artifact set predates integrity hashing and cannot be verified")]
    SnapshotIntegrityUnavailable,
    /// An `artifact_hashes` digest is not 64 lowercase-hex ASCII characters.
    #[error("an artifact hash digest is malformed ({0})")]
    InvalidArtifactHash(String),
    /// The marker was stable but the embedded hashes never matched the loaded
    /// bytes (a torn or corrupt artifact set); the retry budget was exhausted.
    #[error("the artifact content hashes do not match the generation report")]
    SnapshotHashMismatch,
    /// `document.json` is not valid JSON for an `ExplorerDocument`.
    #[error("document.json is malformed: {0}")]
    MalformedDocument(String),
    /// `generation.json` is not valid JSON for a `GenerationReport`.
    #[error("generation.json is malformed: {0}")]
    MalformedGeneration(String),
    /// `diagnostics.json` is not valid JSON for a `DiagnosticsReport`.
    #[error("diagnostics.json is malformed: {0}")]
    MalformedDiagnostics(String),
    /// The document failed referential-integrity validation.
    #[error("document.json failed schema validation ({0} problem(s))")]
    InvalidDocument(usize),
    /// An artifact declares an unsupported schema major version.
    #[error("unsupported schema version: {0}")]
    SchemaVersionUnsupported(String),
    /// `document.json` and `diagnostics.json` disagree on `schema_version`.
    #[error("document and diagnostics disagree on schema_version ({document} vs {diagnostics})")]
    SchemaVersionMismatch {
        /// The document's declared version.
        document: String,
        /// The diagnostics' declared version.
        diagnostics: String,
    },
    /// An internal invariant was violated.
    #[error("internal invariant violated: {0}")]
    InternalInvariant(String),
}

impl SnapshotError {
    /// The stable machine-readable code for this error.
    pub fn code(&self) -> &'static str {
        match self {
            SnapshotError::ArtifactsMissing => "artifacts_missing",
            SnapshotError::ArtifactReadFailed(_) => "artifact_read_failed",
            SnapshotError::ArtifactChangedDuringRead => "artifact_changed_during_read",
            SnapshotError::SnapshotIntegrityUnavailable => "snapshot_integrity_unavailable",
            SnapshotError::InvalidArtifactHash(_) => "invalid_artifact_hash",
            SnapshotError::SnapshotHashMismatch => "snapshot_hash_mismatch",
            SnapshotError::MalformedDocument(_) => "malformed_document",
            SnapshotError::MalformedGeneration(_) => "malformed_generation",
            SnapshotError::MalformedDiagnostics(_) => "malformed_diagnostics",
            SnapshotError::InvalidDocument(_) => "invalid_document",
            SnapshotError::SchemaVersionUnsupported(_) => "schema_version_unsupported",
            SnapshotError::SchemaVersionMismatch { .. } => "schema_version_mismatch",
            SnapshotError::InternalInvariant(_) => "internal_invariant",
        }
    }

    /// An actionable remediation hint, when one applies.
    pub fn remediation(&self) -> Option<&'static str> {
        match self {
            SnapshotError::ArtifactsMissing => {
                Some("Run `cargo cratevista generate` to produce the artifacts.")
            }
            SnapshotError::SnapshotIntegrityUnavailable => {
                Some("Regenerate the artifacts with `cargo cratevista generate`.")
            }
            SnapshotError::ArtifactChangedDuringRead => {
                Some("A generation may be in progress; retry once it finishes.")
            }
            SnapshotError::SnapshotHashMismatch => {
                Some("The artifact set looks torn or corrupt; run `cargo cratevista generate`.")
            }
            _ => None,
        }
    }

    /// Whether this failure means the environment/prerequisites are unsatisfied
    /// (missing or unverifiable artifacts) rather than a runtime error. Core maps
    /// `true` to exit code 3 and `false` to exit code 1.
    pub fn is_environment(&self) -> bool {
        matches!(
            self,
            SnapshotError::ArtifactsMissing | SnapshotError::SnapshotIntegrityUnavailable
        )
    }
}

/// A failure binding the listener or running the server. Never embeds a
/// filesystem path (a bind address is loopback `host:port`, not sensitive).
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// The listener could not bind the requested address.
    #[error("could not bind {0}")]
    BindFailed(String),
    /// The default port and its increment range were all occupied.
    #[error("no free port in {start}..={end}")]
    PortRangeExhausted {
        /// First port tried.
        start: u16,
        /// Last port tried.
        end: u16,
    },
    /// The serve loop or graceful shutdown failed.
    #[error("server runtime error: {0}")]
    ShutdownFailed(String),
    /// An internal invariant was violated.
    #[error("internal invariant violated: {0}")]
    InternalInvariant(String),
}

impl ServerError {
    /// The stable machine-readable code for this error.
    pub fn code(&self) -> &'static str {
        match self {
            ServerError::BindFailed(_) => "bind_failed",
            ServerError::PortRangeExhausted { .. } => "port_range_exhausted",
            ServerError::ShutdownFailed(_) => "shutdown_failed",
            ServerError::InternalInvariant(_) => "internal_invariant",
        }
    }
}
