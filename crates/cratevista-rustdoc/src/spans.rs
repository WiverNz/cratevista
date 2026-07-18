//! Mapping rustdoc `Span`s to validated repository-relative `SourceLocation`s.
//!
//! rustdoc's `filename` may be **absolute** or **relative** depending on the
//! invocation, so [`crate::NormalizeContext`] carries both `workspace_root` and
//! `package_root`. Absolute paths are never exposed or hashed: anything outside
//! the workspace root, or a generated/synthetic pseudo-path, is omitted with a
//! recoverable diagnostic, and `RepoRelativePath` validation is never weakened.

use cratevista_schema::{RepoRelativePath, SourceLocation, Span as SchemaSpan};
use rustdoc_types::Span as RustdocSpan;

use crate::options::NormalizeContext;

/// Why a span did not become a [`SourceLocation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanOmission {
    /// The file is outside the workspace root (e.g. a dependency source).
    OutsideWorkspace,
    /// The file is generated/macro-expanded/synthetic (no real path).
    Generated,
    /// The file resolved inside the root but failed path validation.
    Invalid(String),
}

/// Maps a rustdoc span to a repo-relative [`SourceLocation`] under the workspace.
pub fn map_span(
    context: &NormalizeContext,
    span: &RustdocSpan,
) -> Result<SourceLocation, SpanOmission> {
    let filename = span.filename.to_string_lossy();
    let normalized = normalize_sep(&filename);
    if normalized.is_empty() || is_generated(&normalized) {
        return Err(SpanOmission::Generated);
    }

    let workspace = normalize_sep(&context.workspace_root.to_string_lossy());
    let full = if is_absolute_str(&normalized) {
        normalized
    } else {
        let package = normalize_sep(&context.package_root.to_string_lossy());
        format!("{}/{}", package.trim_end_matches('/'), normalized)
    };

    let relative =
        strip_prefix_components(&workspace, &full).ok_or(SpanOmission::OutsideWorkspace)?;
    let path =
        RepoRelativePath::new(&relative).map_err(|e| SpanOmission::Invalid(e.to_string()))?;
    Ok(SourceLocation::new(path, Some(schema_span(span))))
}

fn schema_span(span: &RustdocSpan) -> SchemaSpan {
    SchemaSpan {
        start_line: span.begin.0 as u32,
        start_col: span.begin.1 as u32,
        end_line: span.end.0 as u32,
        end_col: span.end.1 as u32,
    }
}

/// Whether a normalized filename denotes a generated/synthetic pseudo-location.
fn is_generated(path: &str) -> bool {
    path.contains('<') || path.contains('>')
}

fn normalize_sep(path: &str) -> String {
    path.replace('\\', "/")
}

/// Whether a normalized path string is absolute (POSIX root, Windows drive, or UNC).
fn is_absolute_str(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("//")
        || (bytes.len() >= 2 && bytes[1] == b':' && (bytes[0] as char).is_ascii_alphabetic())
}

/// Component-wise lexical strip of `base` from `full`, ignoring `.`/empty parts
/// and comparing case-insensitively (Windows paths differ only in drive-letter
/// casing in practice). Returns `None` when `full` is not under `base`.
fn strip_prefix_components(base: &str, full: &str) -> Option<String> {
    let base_components: Vec<&str> = split_components(base);
    let full_components: Vec<&str> = split_components(full);
    if full_components.len() < base_components.len() {
        return None;
    }
    for (b, f) in base_components.iter().zip(full_components.iter()) {
        if !b.eq_ignore_ascii_case(f) {
            return None;
        }
    }
    Some(full_components[base_components.len()..].join("/"))
}

fn split_components(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn context() -> NormalizeContext {
        NormalizeContext {
            workspace_root: PathBuf::from("/w"),
            package_root: PathBuf::from("/w/crates/foo"),
            package_id: cratevista_schema::EntityId::package("foo"),
            target_id: cratevista_schema::EntityId::target("foo", "lib", "foo"),
            package_name: "foo".into(),
            crate_name: "foo".into(),
            target_name: "foo".into(),
            target_kind: crate::options::RustdocTargetKind::Library,
            toolchain: "nightly-test".into(),
        }
    }

    fn span(filename: &str) -> RustdocSpan {
        RustdocSpan {
            filename: PathBuf::from(filename),
            begin: (3, 5),
            end: (3, 9),
        }
    }

    #[test]
    fn relative_filename_resolves_from_package_root() {
        let location = map_span(&context(), &span("src/lib.rs")).unwrap();
        assert_eq!(location.path.as_str(), "crates/foo/src/lib.rs");
        let s = location.span.unwrap();
        assert_eq!(
            (s.start_line, s.start_col, s.end_line, s.end_col),
            (3, 5, 3, 9)
        );
    }

    #[test]
    fn absolute_inside_workspace_is_stripped() {
        let location = map_span(&context(), &span("/w/crates/foo/src/a.rs")).unwrap();
        assert_eq!(location.path.as_str(), "crates/foo/src/a.rs");
    }

    #[test]
    fn absolute_outside_workspace_is_omitted() {
        assert_eq!(
            map_span(&context(), &span("/elsewhere/dep/src/lib.rs")),
            Err(SpanOmission::OutsideWorkspace)
        );
    }

    #[test]
    fn generated_is_omitted() {
        assert_eq!(
            map_span(&context(), &span("<anon>")),
            Err(SpanOmission::Generated)
        );
    }

    #[test]
    fn windows_backslashes_normalize() {
        let location = map_span(&context(), &span("src\\nested\\b.rs")).unwrap();
        assert_eq!(location.path.as_str(), "crates/foo/src/nested/b.rs");
    }
}
