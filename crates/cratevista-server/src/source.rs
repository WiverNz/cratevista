//! The guarded `/api/source` endpoint.
//!
//! Disabled by default (`403`). When enabled, a single repo-relative `path`
//! query is validated by [`RepoRelativePath`] (rejecting absolute / drive / UNC
//! / `..`), then resolved by **canonicalize-and-contain** under the project
//! root so a symlink cannot escape. The file must be a regular file within a
//! size limit and valid UTF-8. No response ever includes the resolved absolute
//! path.
//!
//! Honest note: canonicalize-then-read is not a perfectly atomic TOCTOU-proof
//! sandbox; it is a strong, standard containment check for a loopback dev tool.

use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use cratevista_schema::RepoRelativePath;

use crate::options::SourceAccessPolicy;
use crate::state::AppState;

/// A per-request source-endpoint failure. Messages never contain a path.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// Source access is off (the default).
    #[error("source access is disabled")]
    Disabled,
    /// The requested path was absolute, contained `..`, or was otherwise invalid.
    #[error("invalid source path")]
    PathInvalid,
    /// The resolved path escaped the project root (e.g. via a symlink).
    #[error("source path escapes the project root")]
    OutsideRoot,
    /// The target exists but is not a regular file.
    #[error("source path is not a regular file")]
    NotFile,
    /// The file exceeds the configured size limit.
    #[error("source file exceeds the size limit")]
    TooLarge,
    /// The file is not valid UTF-8.
    #[error("source file is not valid UTF-8")]
    NotUtf8,
}

impl SourceError {
    /// The stable machine-readable code.
    pub fn code(&self) -> &'static str {
        match self {
            SourceError::Disabled => "source_disabled",
            SourceError::PathInvalid => "source_path_invalid",
            SourceError::OutsideRoot => "source_outside_root",
            SourceError::NotFile => "source_not_file",
            SourceError::TooLarge => "source_too_large",
            SourceError::NotUtf8 => "source_not_utf8",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            SourceError::Disabled => StatusCode::FORBIDDEN,
            SourceError::PathInvalid | SourceError::OutsideRoot => StatusCode::BAD_REQUEST,
            SourceError::NotFile => StatusCode::NOT_FOUND,
            SourceError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            SourceError::NotUtf8 => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        }
    }
}

impl IntoResponse for SourceError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({
            "error": { "code": self.code(), "message": self.to_string() }
        }));
        (self.status(), body).into_response()
    }
}

/// The `path` query parameter.
#[derive(Debug, Deserialize)]
pub struct SourceQuery {
    /// The requested repository-relative path.
    pub path: String,
}

/// `GET /api/source?path=…` — serve a repo-relative source file's contents.
pub async fn source(
    State(state): State<Arc<AppState>>,
    query: Result<Query<SourceQuery>, QueryRejection>,
) -> Response {
    let Ok(Query(query)) = query else {
        // A missing/invalid `path` parameter is a path-invalid request.
        return SourceError::PathInvalid.into_response();
    };
    match read_source(state.source_policy(), &query.path) {
        Ok(text) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(text))
            .expect("source response builds"),
        Err(error) => error.into_response(),
    }
}

/// Validates and reads a repo-relative source file under the policy root.
pub(crate) fn read_source(
    policy: &SourceAccessPolicy,
    requested: &str,
) -> Result<String, SourceError> {
    let (root, max_bytes) = match policy {
        SourceAccessPolicy::Disabled => return Err(SourceError::Disabled),
        SourceAccessPolicy::Enabled { root, max_bytes } => (root, *max_bytes),
    };

    // 1. Structural validation (absolute / drive / UNC / `..` are rejected here).
    let relative = RepoRelativePath::new(requested).map_err(|_| SourceError::PathInvalid)?;

    // 2. Canonicalize-and-contain: resolve symlinks and require containment.
    let canonical_root = root.canonicalize().map_err(|_| SourceError::PathInvalid)?;
    let candidate = canonical_root.join(relative.as_str());
    let canonical = candidate
        .canonicalize()
        .map_err(|_| SourceError::PathInvalid)?;
    if !canonical.starts_with(&canonical_root) {
        return Err(SourceError::OutsideRoot);
    }

    // 3. Must be a regular file within the size limit.
    let metadata = std::fs::metadata(&canonical).map_err(|_| SourceError::PathInvalid)?;
    if !metadata.is_file() {
        return Err(SourceError::NotFile);
    }
    if metadata.len() > max_bytes {
        return Err(SourceError::TooLarge);
    }

    // 4. UTF-8 only.
    let bytes = std::fs::read(&canonical).map_err(|_| SourceError::PathInvalid)?;
    String::from_utf8(bytes).map_err(|_| SourceError::NotUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled(root: &std::path::Path, max_bytes: u64) -> SourceAccessPolicy {
        SourceAccessPolicy::Enabled {
            root: root.to_path_buf(),
            max_bytes,
        }
    }

    #[test]
    fn disabled_by_default() {
        let error = read_source(&SourceAccessPolicy::Disabled, "src/lib.rs").unwrap_err();
        assert_eq!(error.code(), "source_disabled");
    }

    #[test]
    fn reads_a_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "pub fn a() {}\n").unwrap();
        let text = read_source(&enabled(dir.path(), 1024), "src/lib.rs").unwrap();
        assert_eq!(text, "pub fn a() {}\n");
    }

    #[test]
    fn rejects_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let abs = if cfg!(windows) {
            "C:\\Windows\\win.ini"
        } else {
            "/etc/passwd"
        };
        assert_eq!(
            read_source(&enabled(dir.path(), 1024), abs)
                .unwrap_err()
                .code(),
            "source_path_invalid"
        );
    }

    #[test]
    fn rejects_traversal() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read_source(&enabled(dir.path(), 1024), "../secret")
                .unwrap_err()
                .code(),
            "source_path_invalid"
        );
    }

    #[test]
    fn rejects_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        assert_eq!(
            read_source(&enabled(dir.path(), 1024), "src")
                .unwrap_err()
                .code(),
            "source_not_file"
        );
    }

    #[test]
    fn rejects_oversize() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("big.rs"), vec![b'x'; 5000]).unwrap();
        assert_eq!(
            read_source(&enabled(dir.path(), 100), "big.rs")
                .unwrap_err()
                .code(),
            "source_too_large"
        );
    }

    #[test]
    fn rejects_non_utf8() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bin.rs"), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(
            read_source(&enabled(dir.path(), 1024), "bin.rs")
                .unwrap_err()
                .code(),
            "source_not_utf8"
        );
    }

    #[test]
    fn missing_file_does_not_leak_path() {
        let dir = tempfile::tempdir().unwrap();
        let error = read_source(&enabled(dir.path(), 1024), "does/not/exist.rs").unwrap_err();
        let text = error.to_string();
        assert!(!text.contains(":\\"));
        assert!(!text.contains('/') || !text.contains(dir.path().to_string_lossy().as_ref()));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        let root = tempfile::tempdir().unwrap();
        symlink(
            outside.path().join("secret.txt"),
            root.path().join("link.rs"),
        )
        .unwrap();
        let error = read_source(&enabled(root.path(), 1024), "link.rs").unwrap_err();
        // The symlink resolves outside the root.
        assert_eq!(error.code(), "source_outside_root");
    }
}
