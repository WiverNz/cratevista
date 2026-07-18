//! Process / OS path resolution.
//!
//! This module handles *application-level* path concerns: resolving the working
//! directory, locating a Cargo manifest, and enforcing the UTF-8 path policy.
//!
//! The *domain* validated repository-relative source-path type (traversal-safe
//! `SourceLocation`) is a schema concern and is defined in `cratevista-schema`
//! (issue 02), not here.

use std::path::{Path, PathBuf};

use crate::error::CoreError;

/// Returns the path as `&str`, or an error if it is not valid UTF-8.
pub fn checked_utf8(path: &Path) -> Result<&str, CoreError> {
    path.to_str()
        .ok_or_else(|| CoreError::NonUtf8Path(path.to_path_buf()))
}

/// Resolves the project root directory from an optional `--manifest-path`.
///
/// When `manifest_path` is `Some`, its parent directory is used; otherwise the
/// current working directory is used.
pub fn resolve_project_root(manifest_path: Option<&Path>) -> Result<PathBuf, CoreError> {
    match manifest_path {
        Some(path) => {
            // Enforce the UTF-8 policy early with a clear diagnostic.
            checked_utf8(path)?;
            let root = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            Ok(root)
        }
        None => std::env::current_dir()
            .map_err(|source| CoreError::io("cannot determine current directory", source)),
    }
}

/// Walks up from `start` looking for a `Cargo.toml`, returning the first found.
///
/// Read-only: never creates or modifies anything.
pub fn find_cargo_manifest(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join("Cargo.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_utf8_accepts_normal_path() {
        let path = Path::new("some/normal/path.rs");
        assert_eq!(checked_utf8(path).unwrap(), "some/normal/path.rs");
    }

    #[test]
    fn resolve_project_root_uses_manifest_parent() {
        let root = resolve_project_root(Some(Path::new("sub/dir/Cargo.toml"))).unwrap();
        assert_eq!(root, PathBuf::from("sub/dir"));
    }

    #[test]
    fn resolve_project_root_handles_bare_manifest() {
        let root = resolve_project_root(Some(Path::new("Cargo.toml"))).unwrap();
        assert_eq!(root, PathBuf::from("."));
    }

    #[test]
    fn find_cargo_manifest_walks_up() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();

        let found = find_cargo_manifest(&nested).expect("should find manifest up the tree");
        assert_eq!(found, dir.path().join("Cargo.toml"));
    }

    #[test]
    fn find_cargo_manifest_returns_none_without_manifest() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_cargo_manifest(dir.path()).is_none());
    }
}
