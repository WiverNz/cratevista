//! Process exit-code policy — the single source of truth for CrateVista.
//!
//! | code | meaning                                   |
//! |------|-------------------------------------------|
//! | 0    | success                                   |
//! | 1    | runtime / generation error                |
//! | 2    | usage / argument error (clap default)     |
//! | 3    | prerequisite / environment error          |
//! | 4    | not implemented yet (bootstrap stubs)     |

/// A CrateVista process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitCode(u8);

impl ExitCode {
    /// Success.
    pub const SUCCESS: ExitCode = ExitCode(0);
    /// A runtime or generation error.
    pub const RUNTIME_ERROR: ExitCode = ExitCode(1);
    /// A usage or argument error. Matches clap's default exit code.
    pub const USAGE_ERROR: ExitCode = ExitCode(2);
    /// A prerequisite or environment error (e.g. missing Cargo, no project).
    pub const ENVIRONMENT_ERROR: ExitCode = ExitCode(3);
    /// A command that is not implemented yet (bootstrap stub).
    pub const NOT_IMPLEMENTED: ExitCode = ExitCode(4);

    /// Returns the numeric code suitable for [`std::process::exit`].
    pub fn code(self) -> i32 {
        i32::from(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_policy() {
        assert_eq!(ExitCode::SUCCESS.code(), 0);
        assert_eq!(ExitCode::RUNTIME_ERROR.code(), 1);
        assert_eq!(ExitCode::USAGE_ERROR.code(), 2);
        assert_eq!(ExitCode::ENVIRONMENT_ERROR.code(), 3);
        assert_eq!(ExitCode::NOT_IMPLEMENTED.code(), 4);
    }
}
