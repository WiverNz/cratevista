//! Mapping Cargo `Utf8Path`s to validated repository-relative source locations.
//!
//! A [`cratevista_schema::SourceLocation`] is only produced for paths safely
//! inside the selected workspace root; nothing weakens the schema's
//! `RepoRelativePath` validation, and no absolute path ever escapes into an
//! entity.

use cargo_metadata::camino::Utf8Path;
use cratevista_schema::{RepoRelativePath, SourceLocation};

/// Why a path did not become a `SourceLocation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMapError {
    /// The path is outside the selected workspace root.
    OutsideWorkspace,
    /// The path is inside the root but failed `RepoRelativePath` validation.
    Invalid(String),
}

/// Maps `path` to a repository-relative [`SourceLocation`] relative to `root`.
///
/// Returns `OutsideWorkspace` when `path` is not under `root`, or `Invalid` when
/// the repo-relative form fails schema path validation.
pub fn map_source(root: &Utf8Path, path: &Utf8Path) -> Result<SourceLocation, SourceMapError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| SourceMapError::OutsideWorkspace)?;
    let repo_relative = RepoRelativePath::new(relative.as_str())
        .map_err(|error| SourceMapError::Invalid(error.to_string()))?;
    Ok(SourceLocation::new(repo_relative, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_root_becomes_repo_relative() {
        let root = Utf8Path::new("/w");
        let path = Utf8Path::new("/w/crates/foo/src/lib.rs");
        let location = map_source(root, path).unwrap();
        assert_eq!(location.path.as_str(), "crates/foo/src/lib.rs");
        assert!(location.span.is_none());
    }

    #[test]
    fn outside_root_is_reported() {
        let root = Utf8Path::new("/w");
        let path = Utf8Path::new("/elsewhere/lib.rs");
        assert_eq!(
            map_source(root, path),
            Err(SourceMapError::OutsideWorkspace)
        );
    }
}
