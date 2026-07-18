//! Resolved output identity and the stable `output_key` (PRD 10, Decision 2).
//!
//! The key must be **stable when filesystem existence changes**: creating the
//! intermediate parent directories of an output must not change its key. So the
//! nearest-existing-ancestor / missing-remainder split is resolved into **one**
//! absolute component sequence *before* hashing, and the split never appears in
//! the hashed bytes.
//!
//! The key is **local bookkeeping only** — a per-output collision guard — and is
//! never exposed in the published site.

use std::path::{Component, Path, PathBuf};

use super::error::BuildError;

/// The domain-separation tag; bumping it changes every key.
const DOMAIN: &[u8] = b"cratevista-output-key-v1";

/// The resolved absolute output identity: the canonical existing ancestor with the
/// (missing) remainder appended, as one path with no boundary between the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOutput {
    resolved: PathBuf,
}

impl ResolvedOutput {
    /// The resolved absolute path. Internal to static-build; never surfaced to a
    /// user-facing diagnostic.
    pub(crate) fn path(&self) -> &Path {
        &self.resolved
    }

    /// The parent of the resolved output — where the lock and keyed siblings live.
    pub(crate) fn parent(&self) -> Option<&Path> {
        self.resolved.parent()
    }

    /// Constructs a resolved identity from an absolute path. **Tests only**, to
    /// inject a deliberately different re-resolution and prove the identity guard.
    #[cfg(test)]
    pub(crate) fn from_path_for_test(path: PathBuf) -> ResolvedOutput {
        ResolvedOutput { resolved: path }
    }
}

/// Resolves `output` to a stable identity, rejecting symlinked components.
///
/// `output` must be **absolute**. Steps (locked algorithm):
/// 1. lexically normalize;
/// 2. find the nearest existing ancestor;
/// 3. reject a symlinked output or ancestor component (`build_output_symlink`);
/// 4. canonicalize that ancestor and append the normalized missing remainder,
///    producing one resolved absolute path.
pub fn resolve_output(output: &Path) -> Result<ResolvedOutput, BuildError> {
    if !output.is_absolute() {
        return Err(BuildError::Filesystem {
            context: "output-not-absolute",
        });
    }
    let normalized = lexically_normalize(output);

    // Nearest existing ancestor (may be `normalized` itself).
    let nearest_existing = normalized
        .ancestors()
        .find(|candidate| std::fs::symlink_metadata(candidate).is_ok())
        .ok_or(BuildError::Filesystem {
            context: "no-existing-ancestor",
        })?
        .to_path_buf();

    // Reject a symlink at the output or any existing ancestor component.
    for ancestor in nearest_existing.ancestors() {
        if let Ok(metadata) = std::fs::symlink_metadata(ancestor)
            && metadata.file_type().is_symlink()
        {
            return Err(BuildError::OutputSymlink);
        }
    }

    // Canonicalize the existing part (no symlinks remain to resolve), then append
    // the missing remainder. `full` is identical whether few or many of the
    // remainder components already exist — that is the stability property.
    let canonical_base = nearest_existing
        .canonicalize()
        .map_err(|_| BuildError::Filesystem {
            context: "canonicalize",
        })?;
    let remainder =
        normalized
            .strip_prefix(&nearest_existing)
            .map_err(|_| BuildError::Filesystem {
                context: "strip-prefix",
            })?;
    let mut resolved = canonical_base;
    for component in remainder.components() {
        if let Component::Normal(name) = component {
            resolved.push(name);
        }
    }
    Ok(ResolvedOutput { resolved })
}

/// Derives the `output_key` for an already-resolved output.
///
/// 16 lowercase-hex characters of `BLAKE3(domain || framed-components)`. Filename-
/// safe by construction.
pub fn output_key(resolved: &ResolvedOutput) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN);
    for component in resolved.resolved.components() {
        let bytes = component_bytes(component.as_os_str());
        // `u32` length prefix frames each component so boundaries are unambiguous
        // without a separator byte.
        hasher.update(&(bytes.len() as u32).to_le_bytes());
        hasher.update(&bytes);
    }
    let hash = hasher.finalize();
    hash.to_hex()[..16].to_string()
}

/// Resolve + derive in one call.
pub fn resolve_output_key(output: &Path) -> Result<String, BuildError> {
    Ok(output_key(&resolve_output(output)?))
}

/// One component's lossless raw bytes.
///
/// Unix: `OsStr` raw bytes (non-UTF-8 preserved). Windows: UTF-16 code units
/// (`encode_wide`) serialized little-endian — lossless for Windows paths.
#[cfg(unix)]
fn component_bytes(component: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    component.as_bytes().to_vec()
}

#[cfg(windows)]
fn component_bytes(component: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    let mut bytes = Vec::new();
    for unit in component.encode_wide() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// Neither Unix nor Windows: a lossy but deterministic fallback (this crate only
/// targets the two).
#[cfg(not(any(unix, windows)))]
fn component_bytes(component: &std::ffi::OsStr) -> Vec<u8> {
    component.to_string_lossy().into_owned().into_bytes()
}

/// Lexical normalization: drop `.`, resolve `..`, keep the root/prefix.
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop a `Normal` segment; never climb past the root/prefix.
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                } else {
                    normalized.push("..");
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn canonical(dir: &TempDir) -> PathBuf {
        dir.path().canonicalize().expect("canonical tempdir")
    }

    #[test]
    fn key_is_stable_when_intermediate_parents_are_created_later() {
        let dir = TempDir::new().unwrap();
        let root = canonical(&dir);
        // Only a distant ancestor exists.
        std::fs::create_dir_all(root.join("a")).unwrap();
        let output = root.join("a/b/c/site");

        let key_before = resolve_output_key(&output).expect("resolves");

        // Create the intermediate missing parents.
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        let key_after = resolve_output_key(&output).expect("resolves");

        assert_eq!(
            key_before, key_after,
            "creating intermediate parents must not change the key"
        );
    }

    #[test]
    fn key_is_stable_when_the_output_itself_is_created() {
        let dir = TempDir::new().unwrap();
        let root = canonical(&dir);
        let output = root.join("site");
        let before = resolve_output_key(&output).unwrap();
        std::fs::create_dir_all(&output).unwrap();
        let after = resolve_output_key(&output).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn distinct_outputs_produce_distinct_keys() {
        let dir = TempDir::new().unwrap();
        let root = canonical(&dir);
        let a = resolve_output_key(&root.join("site-a")).unwrap();
        let b = resolve_output_key(&root.join("site-b")).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn key_is_16_lowercase_hex_and_filename_safe() {
        let dir = TempDir::new().unwrap();
        let key = resolve_output_key(&canonical(&dir).join("site")).unwrap();
        assert_eq!(key.len(), 16);
        assert!(
            key.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn deterministic_across_calls() {
        let dir = TempDir::new().unwrap();
        let output = canonical(&dir).join("x/y/site");
        assert_eq!(
            resolve_output_key(&output).unwrap(),
            resolve_output_key(&output).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_ancestor_component_is_rejected() {
        use std::os::unix::fs::symlink;
        let dir = TempDir::new().unwrap();
        let root = canonical(&dir);
        std::fs::create_dir_all(root.join("real")).unwrap();
        symlink(root.join("real"), root.join("link")).unwrap();
        let output = root.join("link/site");
        assert_eq!(resolve_output(&output), Err(BuildError::OutputSymlink));
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_component_is_supported() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let dir = TempDir::new().unwrap();
        let root = canonical(&dir);
        let weird = OsStr::from_bytes(b"we\xffird");
        let output = root.join(weird).join("site");
        // It resolves and hashes without panicking, and is stable.
        let key = resolve_output_key(&output).unwrap();
        assert_eq!(key.len(), 16);
        assert_eq!(key, resolve_output_key(&output).unwrap());
    }

    #[test]
    fn a_relative_output_is_rejected_as_a_programming_error() {
        assert!(matches!(
            resolve_output(Path::new("relative/site")),
            Err(BuildError::Filesystem { .. })
        ));
    }
}
