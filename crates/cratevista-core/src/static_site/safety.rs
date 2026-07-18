//! Output path-safety: reject only outputs whose **replacement would delete a
//! protected path** (PRD 10, Decision 2).
//!
//! The rule is **directional**: `<output>` is dangerous exactly when publishing it
//! (which removes whatever is at `<output>`) would remove a protected path — i.e.
//! `output == p` or `output` is an **ancestor** of `p`. A **descendant** of a
//! protected root is safe and is *not* rejected.

use std::path::{Path, PathBuf};

use super::error::BuildError;
use super::output_identity::{ResolvedOutput, output_key, resolve_output};

/// The explicit, core-owned safety context for one build.
///
/// `protected` are the real generation inputs `run_build` already holds (root and
/// member manifests, `Cargo.lock`, source roots, `cratevista.toml`, flow/override
/// files, referenced docs, the three artifacts and their root, and the workspace
/// root). `materialize_static_site` checks against this list, so its tests supply an
/// explicit set and never run cargo.
///
/// The `output_key` is **private and derived from the output itself** via
/// [`OutputSafety::for_output`]: no caller can inject an arbitrary key that does not
/// correspond to `output`, so recovery/cleanup can never be pointed at another
/// output's siblings.
#[derive(Debug, Clone)]
pub struct OutputSafety {
    /// Absolute protected paths.
    protected: Vec<PathBuf>,
    /// The current output's key (see [`super::output_identity`]).
    output_key: String,
}

impl OutputSafety {
    /// Builds the safety context for `output`, deriving the `output_key` from the
    /// output's own resolved identity. Rejects a symlinked output/ancestor
    /// (`build_output_symlink`).
    ///
    /// This is the **only** production constructor, so a key that does not match
    /// `output` cannot enter the system.
    pub fn for_output(output: &Path, protected: Vec<PathBuf>) -> Result<OutputSafety, BuildError> {
        let resolved = resolve_output(output)?;
        Ok(OutputSafety {
            protected,
            output_key: output_key(&resolved),
        })
    }

    /// Builds a safety context from explicit parts. **Crate-internal / tests only**
    /// — it does not verify that `output_key` matches any output, so it is used to
    /// prove that a forged/mismatched key is rejected downstream, never as a
    /// production path.
    #[cfg(test)]
    pub(crate) fn from_parts(protected: Vec<PathBuf>, output_key: String) -> OutputSafety {
        OutputSafety {
            protected,
            output_key,
        }
    }

    /// The derived output key (16 lowercase-hex).
    pub fn output_key(&self) -> &str {
        &self.output_key
    }

    /// The protected paths.
    pub fn protected(&self) -> &[PathBuf] {
        &self.protected
    }

    /// Checks `output` against the protected set. Rejects a symlinked output/
    /// ancestor (`build_output_symlink`) and a replacement-dangerous overlap
    /// (`build_output_forbidden`); a descendant that contains no input is allowed.
    pub fn check(&self, output: &Path) -> Result<(), BuildError> {
        let resolved = resolve_output(output)?; // also rejects symlinks
        self.check_resolved(&resolved)
    }

    /// [`OutputSafety::check`] on an already-resolved output.
    pub fn check_resolved(&self, resolved: &ResolvedOutput) -> Result<(), BuildError> {
        let out = resolved.path();
        for protected in &self.protected {
            let p = canonical_or_lexical(protected);
            // output == protected  →  replacing output deletes the protected path.
            if out == p.as_path() {
                return Err(BuildError::OutputForbidden {
                    reason: context_label_of(&p, out),
                });
            }
            // output is an ANCESTOR of protected  →  the protected path lives under
            // output, so replacing output removes it.
            if p.starts_with(out) {
                return Err(BuildError::OutputForbidden {
                    reason: context_label_of(&p, out),
                });
            }
            // output is a DESCENDANT of protected (out.starts_with(p)) → allowed:
            // replacing output removes only output and its own contents.
        }
        Ok(())
    }
}

/// A short **safe** reason. Never embeds an absolute path.
fn context_label_of(output: &Path, protected: &Path) -> &'static str {
    if output == protected {
        "reason: it is one of the generation inputs"
    } else {
        // output is an ancestor of protected
        "reason: it contains a generation input, so replacing it would delete it"
    }
}

/// Canonicalizes a protected path (it exists); falls back to the path itself if
/// canonicalization fails (e.g. a not-yet-created referenced doc), so a missing
/// input is still compared lexically.
fn canonical_or_lexical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn canonical(dir: &TempDir) -> PathBuf {
        dir.path().canonicalize().unwrap()
    }

    /// A workspace whose protected set mirrors production.
    fn workspace() -> (TempDir, PathBuf, OutputSafety) {
        let dir = TempDir::new().unwrap();
        let root = canonical(&dir);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target/cratevista")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(root.join("Cargo.lock"), "").unwrap();
        std::fs::write(root.join("cratevista.toml"), "").unwrap();
        for artifact in ["document.json", "generation.json", "diagnostics.json"] {
            std::fs::write(root.join("target/cratevista").join(artifact), "{}").unwrap();
        }
        let protected = vec![
            root.clone(),
            root.join("Cargo.toml"),
            root.join("Cargo.lock"),
            root.join("src"),
            root.join("cratevista.toml"),
            root.join("target/cratevista"),
            root.join("target/cratevista/document.json"),
            root.join("target/cratevista/generation.json"),
            root.join("target/cratevista/diagnostics.json"),
        ];
        let safety = OutputSafety::from_parts(protected, "deadbeefdeadbeef".to_string());
        (dir, root, safety)
    }

    #[test]
    fn workspace_dist_is_allowed() {
        let (_dir, root, safety) = workspace();
        assert_eq!(safety.check(&root.join("dist")), Ok(()));
    }

    #[test]
    fn default_target_cratevista_site_is_allowed() {
        let (_dir, root, safety) = workspace();
        assert_eq!(safety.check(&root.join("target/cratevista/site")), Ok(()));
    }

    #[test]
    fn output_equal_to_a_protected_root_is_rejected() {
        let (_dir, root, safety) = workspace();
        assert!(matches!(
            safety.check(&root),
            Err(BuildError::OutputForbidden { .. })
        ));
        assert!(matches!(
            safety.check(&root.join("target/cratevista")),
            Err(BuildError::OutputForbidden { .. })
        ));
    }

    #[test]
    fn output_equal_to_a_protected_file_is_rejected() {
        let (_dir, root, safety) = workspace();
        assert!(matches!(
            safety.check(&root.join("Cargo.toml")),
            Err(BuildError::OutputForbidden { .. })
        ));
    }

    #[test]
    fn output_that_contains_a_protected_path_is_rejected() {
        let (_dir, root, safety) = workspace();
        // `<root>/target` is an ancestor of the protected `target/cratevista` and
        // its artifacts → replacing it would delete them.
        assert!(matches!(
            safety.check(&root.join("target")),
            Err(BuildError::OutputForbidden { .. })
        ));
        // The workspace parent is an ancestor of the workspace root itself.
        assert!(matches!(
            safety.check(root.parent().unwrap()),
            Err(BuildError::OutputForbidden { .. })
        ));
    }

    #[test]
    fn an_unrelated_descendant_in_the_workspace_is_allowed() {
        let (_dir, root, safety) = workspace();
        assert_eq!(safety.check(&root.join("some/other/place")), Ok(()));
        // A descendant of a source root that is not itself an input is allowed.
        assert_eq!(safety.check(&root.join("src/generated-site")), Ok(()));
    }

    #[test]
    fn a_non_existing_output_is_checked_through_its_nearest_existing_ancestor() {
        let (_dir, root, safety) = workspace();
        // Neither exists yet; both resolve through the existing workspace root.
        assert_eq!(safety.check(&root.join("a/b/c/dist")), Ok(()));
        assert!(matches!(
            // ancestor of an input, even though `target/x` does not exist:
            // normalizes to `<root>/target` which contains `target/cratevista`.
            safety.check(&root.join("target/../target")),
            Err(BuildError::OutputForbidden { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_output_ancestor_is_rejected() {
        use std::os::unix::fs::symlink;
        let (_dir, root, safety) = workspace();
        symlink(root.join("src"), root.join("link")).unwrap();
        assert_eq!(
            safety.check(&root.join("link/site")),
            Err(BuildError::OutputSymlink)
        );
    }
}
