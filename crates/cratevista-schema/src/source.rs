//! Domain source locations: validated repository-relative paths and spans.
//!
//! A [`SourceLocation`] is a *domain* location, not a process path. Its path is
//! validated at construction to be repository-relative and normalized, so no
//! absolute machine path or current-working-directory semantics can leak into
//! `document.json`. The server reuses this type as its traversal guard.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};

/// Error returned when constructing a [`RepoRelativePath`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourcePathError {
    /// The path was empty (or normalized to nothing).
    #[error("source path is empty")]
    Empty,
    /// The path was absolute (leading `/`, a drive letter, or a UNC prefix).
    #[error("source path must be repository-relative, not absolute: {0}")]
    Absolute(String),
    /// The path contained a `..` component (traversal escape).
    #[error("source path must not contain a `..` component: {0}")]
    Traversal(String),
}

/// A validated, normalized, repository-relative path (forward-slash separators).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepoRelativePath(String);

impl RepoRelativePath {
    /// Validates and normalizes a repository-relative path.
    ///
    /// Rejects absolute paths (leading `/`, `X:` drive letters, `\\`/`//` UNC
    /// prefixes) and any `..` component. Backslashes are normalized to forward
    /// slashes; `.` components and redundant separators are removed.
    pub fn new(input: &str) -> Result<Self, SourcePathError> {
        if input.is_empty() {
            return Err(SourcePathError::Empty);
        }
        let normalized = input.replace('\\', "/");
        if normalized.starts_with('/') {
            return Err(SourcePathError::Absolute(input.to_string()));
        }
        let bytes = normalized.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' && (bytes[0] as char).is_ascii_alphabetic() {
            return Err(SourcePathError::Absolute(input.to_string()));
        }

        let mut parts: Vec<&str> = Vec::new();
        for component in normalized.split('/') {
            match component {
                "" | "." => continue,
                ".." => return Err(SourcePathError::Traversal(input.to_string())),
                other => parts.push(other),
            }
        }
        if parts.is_empty() {
            return Err(SourcePathError::Empty);
        }
        Ok(RepoRelativePath(parts.join("/")))
    }

    /// The normalized path as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RepoRelativePath {
    type Error = SourcePathError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        RepoRelativePath::new(&value)
    }
}

impl From<RepoRelativePath> for String {
    fn from(value: RepoRelativePath) -> Self {
        value.0
    }
}

impl JsonSchema for RepoRelativePath {
    fn schema_name() -> Cow<'static, str> {
        "RepoRelativePath".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}

/// A 1-based line/column span within a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Span {
    /// 1-based start line.
    pub start_line: u32,
    /// 1-based start column.
    pub start_col: u32,
    /// 1-based end line.
    pub end_line: u32,
    /// 1-based end column.
    pub end_col: u32,
}

/// A repository-relative source location with an optional span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceLocation {
    /// The validated repository-relative path.
    pub path: RepoRelativePath,
    /// Optional 1-based line/column span.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl SourceLocation {
    /// Builds a source location from a repository-relative path and optional span.
    pub fn new(path: RepoRelativePath, span: Option<Span>) -> Self {
        SourceLocation { path, span }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_normalizes_relative_paths() {
        assert_eq!(
            RepoRelativePath::new("src/lib.rs").unwrap().as_str(),
            "src/lib.rs"
        );
        assert_eq!(
            RepoRelativePath::new("src\\a\\b.rs").unwrap().as_str(),
            "src/a/b.rs"
        );
        assert_eq!(
            RepoRelativePath::new("./src/./lib.rs").unwrap().as_str(),
            "src/lib.rs"
        );
    }

    #[test]
    fn rejects_absolute_and_traversal() {
        assert_eq!(
            RepoRelativePath::new("/etc/passwd"),
            Err(SourcePathError::Absolute("/etc/passwd".into()))
        );
        assert_eq!(
            RepoRelativePath::new("C:\\Windows"),
            Err(SourcePathError::Absolute("C:\\Windows".into()))
        );
        assert_eq!(
            RepoRelativePath::new("\\\\server\\share"),
            Err(SourcePathError::Absolute("\\\\server\\share".into()))
        );
        assert!(matches!(
            RepoRelativePath::new("../secret"),
            Err(SourcePathError::Traversal(_))
        ));
        assert_eq!(RepoRelativePath::new(""), Err(SourcePathError::Empty));
    }

    #[test]
    fn deserialize_validates() {
        let ok: Result<SourceLocation, _> = serde_json::from_str(r#"{"path":"src/lib.rs"}"#);
        assert!(ok.is_ok());
        let bad: Result<SourceLocation, _> = serde_json::from_str(r#"{"path":"../x"}"#);
        assert!(bad.is_err());
    }
}
