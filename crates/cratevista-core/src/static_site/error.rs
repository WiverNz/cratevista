//! The typed error for static-build safety foundations (PRD 10).
//!
//! One authoritative enum, so the PRD's **one-code / one-meaning** contract is
//! enforced in a single place. Phase 2A implements and tests the first group; the
//! remaining variants are declared here (to keep one enum) but are **not** claimed
//! implemented or tested until Phase 2B.
//!
//! # No leakage
//!
//! No variant carries an absolute path, a username or the output identity. Where
//! context is needed a short, safe **label** is used. Rendering goes through
//! [`crate::diagnostic::Diagnostic`], whose `message` is written here.

use crate::diagnostic::Diagnostic;
use crate::exit::ExitCode;
use crate::usecase::CommandFailure;

/// A static-build failure with a stable, browser-safe code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    // --- implemented and tested in Phase 2A --------------------------------
    /// `--base-path` failed validation (Decision 3).
    InvalidBasePath {
        /// A short, safe reason (no user input echoed).
        reason: &'static str,
    },
    /// The current output's advisory lock is held by another process.
    OutputBusy,
    /// A marker is **present** but malformed, an unsupported format/version, an
    /// invalid kind/`output_key` combination, or a mismatched `output_key`.
    OutputMarkerInvalid {
        /// A short, safe reason.
        reason: &'static str,
    },
    /// `<output>` (or a symlinked ancestor component) is a symlink.
    OutputSymlink,
    /// `<output>` equals, or is an ancestor of, a protected path — replacing it
    /// would delete that path.
    OutputForbidden {
        /// A short, safe reason.
        reason: &'static str,
    },

    // --- declared for one authoritative enum; NOT implemented until Phase 2B -
    /// A non-empty `<output>` has no ownership marker. *(Phase 2B.)*
    OutputNotOwned,
    /// `<output>` absent and multiple valid backups exist for this key. *(2B.)*
    RecoveryAmbiguous,
    /// Publication failed but the predecessor was restored. *(Phase 2B.)*
    PublishFailed,
    /// Rollback/finalization could not restore the predecessor. *(Phase 2B.)*
    PublishUnrecoverable,

    // --- internal defensive catch-all --------------------------------------
    /// An unexpected filesystem error that is none of the above. Deliberately not
    /// one of the PRD's `build_output_*` codes, so their one-meaning contract is
    /// preserved; carries only a safe static context label.
    Filesystem {
        /// A short, safe context label (e.g. `"lock"`, `"resolve"`).
        context: &'static str,
    },
}

impl BuildError {
    /// The stable, machine-matchable diagnostic code.
    pub fn code(&self) -> &'static str {
        match self {
            BuildError::InvalidBasePath { .. } => "build_invalid_base_path",
            BuildError::OutputBusy => "build_output_busy",
            BuildError::OutputMarkerInvalid { .. } => "build_output_marker_invalid",
            BuildError::OutputSymlink => "build_output_symlink",
            BuildError::OutputForbidden { .. } => "build_output_forbidden",
            BuildError::OutputNotOwned => "build_output_not_owned",
            BuildError::RecoveryAmbiguous => "build_recovery_ambiguous",
            BuildError::PublishFailed => "build_publish_failed",
            BuildError::PublishUnrecoverable => "build_publish_unrecoverable",
            BuildError::Filesystem { .. } => "build_filesystem_error",
        }
    }

    /// The process exit code. `--base-path` is a usage error; everything else is a
    /// runtime error.
    pub fn exit(&self) -> ExitCode {
        match self {
            BuildError::InvalidBasePath { .. } => ExitCode::USAGE_ERROR,
            _ => ExitCode::RUNTIME_ERROR,
        }
    }

    /// A short, safe, user-facing message. **Never** contains an absolute path,
    /// username or output identity.
    pub fn message(&self) -> String {
        match self {
            BuildError::InvalidBasePath { reason } => {
                format!("--base-path is not valid: {reason}")
            }
            BuildError::OutputBusy => {
                "another `cargo cratevista build` is already writing this output".to_string()
            }
            BuildError::OutputMarkerInvalid { reason } => {
                format!("the output directory's CrateVista marker is not valid: {reason}")
            }
            BuildError::OutputSymlink => {
                "the output path resolves through a symbolic link, which is refused".to_string()
            }
            BuildError::OutputForbidden { reason } => {
                format!("the output directory is not a safe place to write: {reason}")
            }
            BuildError::OutputNotOwned => {
                "the output directory is not empty and was not created by CrateVista".to_string()
            }
            BuildError::RecoveryAmbiguous => {
                "the previous build left more than one recoverable backup; \
                 resolve it by hand"
                    .to_string()
            }
            BuildError::PublishFailed => {
                "the site could not be published; the previous output was restored".to_string()
            }
            BuildError::PublishUnrecoverable => {
                "the site could not be published and the previous state could not be \
                 restored; see the preserved directories"
                    .to_string()
            }
            BuildError::Filesystem { context } => {
                format!("a filesystem operation failed ({context})")
            }
        }
    }

    /// Renders this error as a runtime [`Diagnostic`] (safe by construction).
    pub fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic::error(self.code(), self.message())
    }

    /// Maps this error to a [`CommandFailure`] with the correct exit code.
    pub fn to_command_failure(&self) -> CommandFailure {
        CommandFailure::new(self.to_diagnostic(), self.exit())
    }
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.message())
    }
}

impl std::error::Error for BuildError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_phase_2a_code_is_distinct_and_stable() {
        let codes = [
            BuildError::InvalidBasePath { reason: "x" }.code(),
            BuildError::OutputBusy.code(),
            BuildError::OutputMarkerInvalid { reason: "x" }.code(),
            BuildError::OutputSymlink.code(),
            BuildError::OutputForbidden { reason: "x" }.code(),
        ];
        let mut sorted = codes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "codes must be distinct");
        assert_eq!(
            codes,
            [
                "build_invalid_base_path",
                "build_output_busy",
                "build_output_marker_invalid",
                "build_output_symlink",
                "build_output_forbidden",
            ]
        );
    }

    #[test]
    fn base_path_is_a_usage_error_others_are_runtime() {
        assert_eq!(
            BuildError::InvalidBasePath { reason: "x" }.exit(),
            ExitCode::USAGE_ERROR
        );
        assert_eq!(BuildError::OutputBusy.exit(), ExitCode::RUNTIME_ERROR);
        assert_eq!(
            BuildError::OutputForbidden { reason: "x" }.exit(),
            ExitCode::RUNTIME_ERROR
        );
    }

    #[test]
    fn messages_never_leak_paths_usernames_or_output_identity() {
        // A representative sweep: none of the safe messages contains a path shape.
        for error in [
            BuildError::InvalidBasePath {
                reason: "contains a scheme",
            },
            BuildError::OutputBusy,
            BuildError::OutputMarkerInvalid {
                reason: "malformed JSON",
            },
            BuildError::OutputSymlink,
            BuildError::OutputForbidden {
                reason: "is the workspace root",
            },
            BuildError::Filesystem { context: "lock" },
        ] {
            let rendered = error.to_diagnostic().to_string();
            assert!(!rendered.contains("C:\\"), "{rendered}");
            assert!(!rendered.contains("/home/"), "{rendered}");
            assert!(!rendered.contains("/Users/"), "{rendered}");
        }
    }

    #[test]
    fn filesystem_catch_all_does_not_reuse_a_prd_output_code() {
        assert_eq!(
            BuildError::Filesystem { context: "x" }.code(),
            "build_filesystem_error"
        );
    }
}
