//! Fatal rustdoc-adapter errors.
//!
//! Each variant maps to a stable code (see [`RustdocError::code`]). Recoverable
//! problems are `cratevista_schema::DocumentDiagnostic`s in the result instead.
//! Raw argv (which may contain absolute manifest/target/workspace paths) lives
//! **only** in these error messages and in local `tracing`, never in
//! [`crate::RustdocSummary`] or any public artifact.

/// A fatal error that prevents producing a trustworthy [`crate::RustdocIngest`].
#[derive(Debug)]
pub enum RustdocError {
    /// The pinned/selected nightly toolchain is not installed.
    NightlyMissing,
    /// An explicitly requested toolchain was not found.
    ToolchainNotFound(String),
    /// `cargo rustdoc` ran but failed (non-zero exit).
    RustdocInvocationFailed {
        /// The effective argv.
        argv: Vec<String>,
        /// The captured stderr tail.
        stderr: String,
    },
    /// The rustdoc JSON format version is not the one this adapter supports.
    UnsupportedFormatVersion {
        /// The version found in the JSON.
        found: u32,
        /// The version this adapter supports.
        supported: u32,
    },
    /// The rustdoc JSON could not be parsed.
    MalformedRustdocJson(String),
    /// The expected rustdoc JSON output file was not produced.
    OutputFileMissing(String),
    /// `cargo rustdoc` reported that the requested target does not exist.
    TargetNotFound(String),
    /// A target kind that cannot be documented was requested in fail-fast mode.
    UnsupportedTargetKind(String),
    /// No target in the plan succeeded (also fatal under keep-going).
    NoTargetSucceeded,
    /// The provided plan/options are invalid (paths, duplicates, contradictions).
    InvalidPlan(String),
    /// An internal invariant was violated (prevents deterministic output).
    InternalInvariant(String),
}

impl RustdocError {
    /// The stable diagnostic code for this error.
    pub fn code(&self) -> &'static str {
        match self {
            RustdocError::NightlyMissing => "nightly_missing",
            RustdocError::ToolchainNotFound(_) => "toolchain_not_found",
            RustdocError::RustdocInvocationFailed { .. } => "rustdoc_invocation_failed",
            RustdocError::UnsupportedFormatVersion { .. } => "unsupported_format_version",
            RustdocError::MalformedRustdocJson(_) => "malformed_rustdoc_json",
            RustdocError::OutputFileMissing(_) => "output_file_missing",
            RustdocError::TargetNotFound(_) => "target_not_found",
            RustdocError::UnsupportedTargetKind(_) => "unsupported_target_kind",
            RustdocError::NoTargetSucceeded => "no_target_succeeded",
            RustdocError::InvalidPlan(_) => "invalid_plan",
            RustdocError::InternalInvariant(_) => "internal_invariant",
        }
    }

    /// A short, actionable remediation hint (used by the CLI/doctor wiring).
    pub fn remediation(&self) -> Option<String> {
        match self {
            RustdocError::NightlyMissing => Some(format!(
                "install the supported nightly: rustup toolchain install {}",
                crate::compat::PINNED_NIGHTLY
            )),
            RustdocError::UnsupportedFormatVersion { .. } => Some(format!(
                "install the supported nightly: rustup toolchain install {}",
                crate::compat::PINNED_NIGHTLY
            )),
            RustdocError::ToolchainNotFound(name) => Some(format!(
                "install the toolchain: rustup toolchain install {name}"
            )),
            RustdocError::RustdocInvocationFailed { .. } => Some(
                "run the printed `cargo rustdoc` command manually to see the error.".to_string(),
            ),
            _ => None,
        }
    }
}

impl std::fmt::Display for RustdocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RustdocError::NightlyMissing => write!(
                f,
                "the pinned nightly toolchain `{}` is not installed",
                crate::compat::PINNED_NIGHTLY
            ),
            RustdocError::ToolchainNotFound(name) => {
                write!(f, "toolchain `{name}` was not found")
            }
            RustdocError::RustdocInvocationFailed { argv, stderr } => write!(
                f,
                "`{}` failed: {}",
                argv.join(" "),
                stderr.lines().last().unwrap_or("")
            ),
            RustdocError::UnsupportedFormatVersion { found, supported } => write!(
                f,
                "rustdoc JSON format version {found} is not supported (adapter expects {supported}); \
                 install the supported nightly: rustup toolchain install {}",
                crate::compat::PINNED_NIGHTLY
            ),
            RustdocError::MalformedRustdocJson(msg) => {
                write!(f, "malformed rustdoc JSON: {msg}")
            }
            RustdocError::OutputFileMissing(path) => {
                write!(f, "rustdoc succeeded but produced no JSON output at {path}")
            }
            RustdocError::TargetNotFound(name) => {
                write!(f, "target `{name}` does not exist")
            }
            RustdocError::UnsupportedTargetKind(kind) => {
                write!(f, "target kind `{kind}` cannot be documented")
            }
            RustdocError::NoTargetSucceeded => {
                write!(f, "no target could be documented")
            }
            RustdocError::InvalidPlan(msg) => write!(f, "invalid rustdoc plan: {msg}"),
            RustdocError::InternalInvariant(msg) => {
                write!(f, "internal invariant violated: {msg}")
            }
        }
    }
}

impl std::error::Error for RustdocError {}
