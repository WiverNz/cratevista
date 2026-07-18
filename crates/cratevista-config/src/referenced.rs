//! The paths a configuration explicitly references — flow docs, flow examples
//! and override docs.
//!
//! # Why this exists separately from `docs`
//!
//! [`crate::docs::embed_files`] resolves these same paths and **reads** them, but
//! it keeps only the resulting text: the path itself is discarded. Watch mode
//! (issue 09) needs the paths — "manual documentation included by configuration"
//! is a watched input — and re-deriving them outside this crate would mean a
//! second TOML parser and a second answer to "which files are inputs".
//!
//! # What this module does not do
//!
//! **No filesystem access.** Every check here is pure string validation through
//! [`RepoRelativePath`]. That is the whole point: a *declared* reference is an
//! input regardless of whether the file is currently there.
//!
//! # The inclusion rule
//!
//! **A file's content being unusable does not disqualify the path; the path being
//! illegal does.**
//!
//! - **Included** even when the file is missing, oversized, non-UTF-8, a
//!   directory or otherwise unreadable. These are exactly the files a user is
//!   about to *fix* — a `config_missing_file` path is arguably the most important
//!   one to watch, because the next thing that happens to it is someone creating
//!   it, and that must be noticed.
//! - **Excluded** when the spelling is absolute, drive-lettered, UNC, empty or
//!   contains `..`. Those are not inputs; they are already reported as
//!   `config_invalid_file_path`, and handing one to a filesystem watcher would
//!   register a watch **outside the workspace** — turning a rejected path into a
//!   real observation of the user's disk. This exclusion is a security boundary,
//!   not tidiness.
//!
//! Symlink escapes are deliberately **not** filtered here: detecting one requires
//! resolving the path on disk, which this module must not do. `docs::embed_files`
//! still refuses to *read* through an escaping symlink
//! (`config_path_escapes_workspace`); the worst case here is that a link inside
//! the workspace is watched, which is exactly what watching the workspace means.

use cratevista_schema::RepoRelativePath;

use crate::model::RawConfig;

/// Which kind of declaration referenced a file.
///
/// Kept as a typed enum rather than a string so a consumer can tell a flow's
/// prose from an embedded example without re-reading the config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReferencedFileKind {
    /// `[[flow]].docs`
    FlowDoc,
    /// `[[flow.example]].path`
    FlowExample,
    /// `[[override]].docs`
    OverrideDoc,
}

impl ReferencedFileKind {
    /// A stable lowercase name, for diagnostics and tests.
    pub fn as_str(self) -> &'static str {
        match self {
            ReferencedFileKind::FlowDoc => "flow_doc",
            ReferencedFileKind::FlowExample => "flow_example",
            ReferencedFileKind::OverrideDoc => "override_doc",
        }
    }
}

/// One file a configuration explicitly references.
///
/// `path` is **always workspace-relative and validated** — it is a
/// [`RepoRelativePath`], so it cannot be absolute or traversing by construction,
/// and no canonical or absolute filesystem path is ever exposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencedConfigFile {
    /// The declared, normalized, workspace-relative path.
    pub path: RepoRelativePath,
    /// The declaration that referenced it.
    pub kind: ReferencedFileKind,
}

/// Collects every valid path declared by successfully parsed configuration.
///
/// Sorted by `(path, kind)` and deduplicated on the same pair — so a file
/// referenced twice as a flow doc appears once, while a file referenced as both a
/// flow doc *and* an override doc keeps **both** typed entries (they are
/// different declarations that happen to name one file).
///
/// A file that failed to parse contributes nothing, because it never became a
/// `RawFlowFile`/`RawOverrideFile` — its problem is already a diagnostic.
pub fn collect(raw: &RawConfig) -> Vec<ReferencedConfigFile> {
    let mut found = Vec::new();

    // Flows: docs + example paths. Every parsed flow counts, including one whose
    // id turns out to be a duplicate: it is still a declared reference, and
    // watching a file that turns out not to matter costs one extra regeneration,
    // whereas missing one is a stale document.
    for file in &raw.flow_files {
        for flow in &file.value.flows {
            for doc in &flow.docs {
                push(&mut found, doc.get_ref(), ReferencedFileKind::FlowDoc);
            }
            for example in &flow.examples {
                push(
                    &mut found,
                    example.path.get_ref(),
                    ReferencedFileKind::FlowExample,
                );
            }
        }
    }

    // Overrides: docs. Collected regardless of whether the target entity exists —
    // a missing target is PRD 05's `overlay_target_missing`, not a reason to stop
    // watching the prose the author wrote.
    for file in &raw.override_files {
        for entry in &file.value.overrides {
            for doc in &entry.docs {
                push(&mut found, doc.get_ref(), ReferencedFileKind::OverrideDoc);
            }
        }
    }

    // Sort by the normalized path text, then kind: `RepoRelativePath` has no
    // `Ord`, and sorting on `as_str` is the same total order without requiring
    // one. Deterministic across runs because the input order already is
    // (discovery sorts by file name).
    found.sort_by(|a, b| {
        a.path
            .as_str()
            .cmp(b.path.as_str())
            .then(a.kind.cmp(&b.kind))
    });
    found.dedup_by(|a, b| a.path == b.path && a.kind == b.kind);
    found
}

/// Validates one declared spelling and keeps it if it is a legal repo-relative
/// path. Invalid spellings are dropped silently here — `docs::embed_files`
/// already reports them (`config_invalid_file_path`), and reporting twice would
/// double every diagnostic.
fn push(out: &mut Vec<ReferencedConfigFile>, raw_path: &str, kind: ReferencedFileKind) {
    if let Ok(path) = RepoRelativePath::new(raw_path.trim()) {
        out.push(ReferencedConfigFile { path, kind });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::load_from;
    use std::fs;
    use tempfile::TempDir;

    /// Builds a workspace with the given `(relative path, contents)` files and
    /// collects its referenced files.
    fn collect_from(files: &[(&str, &str)]) -> (TempDir, Vec<ReferencedConfigFile>) {
        write_workspace_and_collect(files, &[])
    }

    /// As [`collect_from`], plus raw-byte files (for genuinely non-UTF-8 input).
    fn write_workspace_and_collect(
        files: &[(&str, &str)],
        raw_files: &[(&str, &[u8])],
    ) -> (TempDir, Vec<ReferencedConfigFile>) {
        let dir = TempDir::new().expect("tempdir");
        for (path, contents) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
            fs::write(&full, contents).expect("write");
        }
        for (path, bytes) in raw_files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
            fs::write(&full, bytes).expect("write");
        }
        let raw = load_from(dir.path());
        let collected = collect(&raw);
        (dir, collected)
    }

    fn paths(files: &[ReferencedConfigFile]) -> Vec<(&str, &str)> {
        files
            .iter()
            .map(|file| (file.path.as_str(), file.kind.as_str()))
            .collect()
    }

    #[test]
    fn all_three_kinds_are_collected() {
        let (_dir, files) = collect_from(&[
            (
                ".cratevista/flows/a.toml",
                r#"
[[flow]]
id = "checkout"
title = "Checkout"
docs = [".cratevista/docs/checkout.md"]

  [[flow.example]]
  id = "req"
  title = "Request"
  path = ".cratevista/examples/req.http"
"#,
            ),
            (
                ".cratevista/overrides/p.toml",
                r#"
[[override]]
target = "package:demo"
docs = [".cratevista/docs/notes.md"]
"#,
            ),
        ]);
        assert_eq!(
            paths(&files),
            [
                (".cratevista/docs/checkout.md", "flow_doc"),
                (".cratevista/docs/notes.md", "override_doc"),
                (".cratevista/examples/req.http", "flow_example"),
            ]
        );
    }

    #[test]
    fn references_are_collected_across_files() {
        let (_dir, files) = collect_from(&[
            (
                ".cratevista/flows/a_first.toml",
                r#"
[[flow]]
id = "one"
title = "One"
docs = ["docs/one.md"]
"#,
            ),
            (
                ".cratevista/flows/z_last.toml",
                r#"
[[flow]]
id = "two"
title = "Two"
docs = ["docs/two.md"]
"#,
            ),
        ]);
        assert_eq!(
            paths(&files),
            [("docs/one.md", "flow_doc"), ("docs/two.md", "flow_doc")]
        );
    }

    #[test]
    fn a_missing_file_is_still_listed() {
        // The whole point: the next thing that happens to this path is someone
        // creating it, and that must be watchable.
        let (_dir, files) = collect_from(&[(
            ".cratevista/flows/a.toml",
            r#"
[[flow]]
id = "one"
title = "One"
docs = ["docs/does-not-exist.md"]
"#,
        )]);
        assert_eq!(paths(&files), [("docs/does-not-exist.md", "flow_doc")]);
    }

    #[test]
    fn oversized_and_non_utf8_files_are_still_listed() {
        // Both files are genuinely unreadable-as-content: one is over the example
        // cap, the other is invalid UTF-8 (a lone 0xFF continuation byte).
        // `embed_files` refuses both — and both are still declared inputs.
        let oversized = "x".repeat(crate::docs::MAX_EXAMPLE_BYTES + 1);
        let (_dir, files) = write_workspace_and_collect(
            &[
                (
                    ".cratevista/flows/a.toml",
                    r#"
[[flow]]
id = "one"
title = "One"
docs = ["docs/bad-utf8.md"]

  [[flow.example]]
  id = "big"
  title = "Big"
  path = "examples/big.txt"
"#,
                ),
                ("examples/big.txt", &oversized),
            ],
            &[("docs/bad-utf8.md", &[0x68, 0x69, 0xFF, 0xFE])],
        );
        assert_eq!(
            paths(&files),
            [
                ("docs/bad-utf8.md", "flow_doc"),
                ("examples/big.txt", "flow_example"),
            ]
        );
    }

    #[test]
    fn a_directory_reference_is_still_listed() {
        let (_dir, files) = collect_from(&[
            (
                ".cratevista/flows/a.toml",
                r#"
[[flow]]
id = "one"
title = "One"
docs = ["docs"]
"#,
            ),
            ("docs/something.md", "hi"),
        ]);
        assert_eq!(paths(&files), [("docs", "flow_doc")]);
    }

    #[test]
    fn invalid_and_traversing_spellings_are_excluded() {
        // Handing any of these to a watcher would register a watch outside the
        // workspace.
        let (_dir, files) = collect_from(&[(
            ".cratevista/flows/a.toml",
            r#"
[[flow]]
id = "one"
title = "One"
docs = ["../outside.md", "/etc/passwd", "C:\\secrets.md", "\\\\host\\share\\x.md", "", "ok.md"]
"#,
        )]);
        assert_eq!(paths(&files), [("ok.md", "flow_doc")]);
    }

    #[test]
    fn repeated_references_are_deduplicated() {
        let (_dir, files) = collect_from(&[
            (
                ".cratevista/flows/a.toml",
                r#"
[[flow]]
id = "one"
title = "One"
docs = ["docs/shared.md", "docs/shared.md"]
"#,
            ),
            (
                ".cratevista/flows/b.toml",
                r#"
[[flow]]
id = "two"
title = "Two"
docs = ["docs/shared.md"]
"#,
            ),
        ]);
        assert_eq!(paths(&files), [("docs/shared.md", "flow_doc")]);
    }

    #[test]
    fn one_path_under_two_kinds_keeps_both_entries() {
        // Same file, two different declarations: both are real references.
        let (_dir, files) = collect_from(&[
            (
                ".cratevista/flows/a.toml",
                r#"
[[flow]]
id = "one"
title = "One"
docs = ["docs/shared.md"]
"#,
            ),
            (
                ".cratevista/overrides/p.toml",
                r#"
[[override]]
target = "package:demo"
docs = ["docs/shared.md"]
"#,
            ),
        ]);
        assert_eq!(
            paths(&files),
            [
                ("docs/shared.md", "flow_doc"),
                ("docs/shared.md", "override_doc"),
            ]
        );
    }

    #[test]
    fn order_is_deterministic_across_repeated_loads() {
        let files = &[
            (
                ".cratevista/flows/a.toml",
                r#"
[[flow]]
id = "one"
title = "One"
docs = ["docs/z.md", "docs/a.md", "docs/m.md"]
"#,
            ),
            (
                ".cratevista/overrides/p.toml",
                r#"
[[override]]
target = "package:demo"
docs = ["docs/b.md"]
"#,
            ),
        ];
        let (dir, first) = collect_from(files);
        let second = collect(&load_from(dir.path()));
        assert_eq!(first, second);
        // And sorted by path, not declaration order.
        assert_eq!(
            paths(&first),
            [
                ("docs/a.md", "flow_doc"),
                ("docs/b.md", "override_doc"),
                ("docs/m.md", "flow_doc"),
                ("docs/z.md", "flow_doc"),
            ]
        );
    }

    #[test]
    fn no_absolute_path_is_ever_exposed() {
        let (dir, files) = collect_from(&[(
            ".cratevista/flows/a.toml",
            r#"
[[flow]]
id = "one"
title = "One"
docs = ["docs/one.md"]

  [[flow.example]]
  id = "e"
  title = "E"
  path = "examples/e.txt"
"#,
        )]);
        let root = dir.path().to_string_lossy().replace('\\', "/");
        for file in &files {
            let path = file.path.as_str();
            assert!(!path.contains(&*root), "leaked the workspace root: {path}");
            assert!(!path.starts_with('/'), "absolute: {path}");
            assert!(!path.contains(".."), "traversing: {path}");
            assert!(
                !(path.len() >= 2 && path.as_bytes()[1] == b':'),
                "drive letter: {path}"
            );
        }
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn a_file_that_failed_to_parse_contributes_nothing() {
        let (_dir, files) = collect_from(&[
            (".cratevista/flows/broken.toml", "this is not = = toml"),
            (
                ".cratevista/flows/good.toml",
                r#"
[[flow]]
id = "one"
title = "One"
docs = ["docs/good.md"]
"#,
            ),
        ]);
        assert_eq!(paths(&files), [("docs/good.md", "flow_doc")]);
    }

    #[test]
    fn absent_configuration_yields_nothing() {
        let (_dir, files) = collect_from(&[]);
        assert!(files.is_empty());
    }
}
