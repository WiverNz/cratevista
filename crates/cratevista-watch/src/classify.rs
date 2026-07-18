//! Deciding whether a changed path is an input worth regenerating for.
//!
//! # Purely lexical, on purpose
//!
//! Nothing here touches the filesystem: no `exists`, no `metadata`, no
//! `canonicalize`. Two reasons.
//!
//! 1. **A missing file must stay classifiable.** A config that references
//!    `.cratevista/docs/checkout.md` before it exists still declares it an input —
//!    the next thing that happens to that path is someone creating it, and that
//!    must be noticed. Asking the disk would answer "not there" and drop it.
//! 2. **Classification runs on every event**, at event rate. A syscall per event
//!    per rule turns a `cargo fmt` sweep into a stat storm.
//!
//! # What this is NOT
//!
//! **Lexical containment is not symlink containment.** [`WatchSet::classify`]
//! rejects a path whose *text* escapes the workspace root, which is exactly the
//! guard that `..` and absolute spellings need. It cannot see through a symlink:
//! `<root>/link/x.rs` is lexically inside the root no matter where `link` points.
//!
//! That gap is deliberate and is closed elsewhere. **The WatchSet builder in
//! `cratevista-core` must canonicalize every registration target that exists and
//! refuse any whose resolved path falls outside the canonical workspace root**,
//! before those targets ever reach this crate. This module then re-checks each
//! event lexically, so the two layers answer different questions: core answers
//! "may I watch this directory at all?", and this answers "does this event name
//! something I was told to care about?".

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// How an input is matched against event paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputKind {
    /// One exact file: `Cargo.toml`, `Cargo.lock`, a workspace-member manifest,
    /// `cratevista.toml`, or a doc/example a configuration explicitly
    /// references. Matches that path and nothing else — in particular, a
    /// *sibling* of a referenced doc is not an input.
    ExactFile,
    /// A Rust source root. Matches `*.rs` at **any depth** below it, because a
    /// new module can appear in a new subdirectory.
    RustSourceRoot,
    /// `.cratevista/flows`. Matches `*.toml` **directly inside** it — discovery
    /// is non-recursive, so a file in a subdirectory is not loaded and must not
    /// regenerate.
    FlowsDir,
    /// `.cratevista/overrides`. Non-recursive `*.toml`, as `FlowsDir`.
    OverridesDir,
    /// A workspace-member manifest **pattern**, e.g. `crates/*`.
    ///
    /// Matches only a `Cargo.toml` whose **member directory** matches the
    /// declared pattern. This is what lets a member that does not exist yet be
    /// watched: `members = ["crates/*"]` covers `crates/new/Cargo.toml` the moment
    /// it is created, without the root manifest changing again.
    ///
    /// It is deliberately not "any `Cargo.toml` under a directory": a vendored or
    /// nested manifest is not a workspace member, and treating it as one would
    /// regenerate on files that have nothing to do with the project.
    WorkspaceMemberManifestPattern,
}

/// One thing core told the watcher to care about.
///
/// Ordered and hashable so a caller can canonicalize a list of inputs. Core does
/// exactly that: recovery coverage is built from the previously active inputs plus
/// new ones, so without a total order the retained list would grow a duplicate on
/// every regeneration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WatchInput {
    /// Absolute path — a file for [`InputKind::ExactFile`], a directory for the
    /// root kinds, or an absolute **pattern** for
    /// [`InputKind::WorkspaceMemberManifestPattern`].
    pub path: PathBuf,
    /// How to match it.
    pub kind: InputKind,
    /// Workspace-relative exclusions, applied **before** relevance is returned.
    ///
    /// Only meaningful for [`InputKind::WorkspaceMemberManifestPattern`], which is
    /// the only kind whose match set is open-ended. Empty for every other kind.
    pub excludes: Vec<String>,
}

impl WatchInput {
    /// An exact file input.
    pub fn file(path: impl Into<PathBuf>) -> Self {
        WatchInput {
            path: path.into(),
            kind: InputKind::ExactFile,
            excludes: Vec::new(),
        }
    }

    /// A workspace-member manifest pattern, with workspace-relative `excludes`.
    ///
    /// `pattern` is the **absolute** member pattern (root-joined), e.g.
    /// `<root>/crates/*`; `excludes` are workspace-relative member paths or
    /// patterns, e.g. `crates/skipped`.
    pub fn workspace_member_pattern(
        pattern: impl Into<PathBuf>,
        excludes: impl IntoIterator<Item = String>,
    ) -> Self {
        let mut excludes: Vec<String> = excludes.into_iter().collect();
        excludes.sort();
        excludes.dedup();
        WatchInput {
            path: pattern.into(),
            kind: InputKind::WorkspaceMemberManifestPattern,
            excludes,
        }
    }

    /// A recursive Rust source root.
    pub fn rust_root(path: impl Into<PathBuf>) -> Self {
        WatchInput {
            path: path.into(),
            kind: InputKind::RustSourceRoot,
            excludes: Vec::new(),
        }
    }

    /// The non-recursive `.cratevista/flows` directory.
    pub fn flows_dir(path: impl Into<PathBuf>) -> Self {
        WatchInput {
            path: path.into(),
            kind: InputKind::FlowsDir,
            excludes: Vec::new(),
        }
    }

    /// The non-recursive `.cratevista/overrides` directory.
    pub fn overrides_dir(path: impl Into<PathBuf>) -> Self {
        WatchInput {
            path: path.into(),
            kind: InputKind::OverridesDir,
            excludes: Vec::new(),
        }
    }
}

/// Why a path inside the workspace was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreReason {
    /// Under `target/` — including `target/cratevista/`, our own output. This is
    /// the loop guard.
    GeneratedOutput,
    /// Under `.git/`.
    VersionControl,
    /// Under `web/node_modules/` or `web/dist/`.
    FrontendArtifacts,
    /// Under a hidden directory that is not `.cratevista`.
    HiddenDirectory,
    /// An editor backup, swap or probe file.
    EditorTemporary,
}

/// The verdict for one event path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    /// A declared input changed; a regeneration should be considered.
    Relevant(InputKind),
    /// The path's text falls outside the workspace root.
    Outside,
    /// An ignore rule matched.
    Ignored(IgnoreReason),
    /// Inside the workspace and not ignored, but nothing declared it an input —
    /// the common case for a directory watch, which reports every file in the
    /// directory whether we asked for it or not.
    NotAnInput,
}

impl Classification {
    /// Whether this path should start or extend a burst.
    pub fn is_relevant(self) -> bool {
        matches!(self, Classification::Relevant(_))
    }
}

/// The generated-output directory. Everything under it is ignored, which is what
/// stops our own `target/cratevista/*.json` writes from retriggering generation.
const GENERATED_DIR: &str = "target";
/// The one hidden directory that is a real input.
const CONFIG_DIR: &str = ".cratevista";

/// The declared inputs plus the workspace they live in.
///
/// Construction normalizes every input path once, so per-event classification is
/// string comparison rather than repeated path arithmetic.
#[derive(Debug, Clone)]
pub struct WatchSet {
    /// Normalized absolute root, `/`-separated.
    root: String,
    /// Normalized, root-relative inputs — path, kind, and (for a member pattern)
    /// its exclusions — sorted and deduplicated.
    inputs: Vec<(String, InputKind, Vec<String>)>,
}

impl WatchSet {
    /// Builds a set from an **absolute** workspace root and the inputs core
    /// resolved.
    ///
    /// An input that is not lexically inside `root` is dropped here rather than
    /// rejected loudly: core is what proves containment (including through
    /// symlinks), and a watcher that refuses to start because one stale path
    /// slipped through would be worse than one that watches the rest.
    pub fn new(root: &Path, inputs: impl IntoIterator<Item = WatchInput>) -> Self {
        let root = normalize(root);
        let mut normalized: Vec<(String, InputKind, Vec<String>)> = inputs
            .into_iter()
            .filter_map(|input| {
                relative_to(&root, &normalize(&input.path))
                    .map(|rel| (rel, input.kind, input.excludes))
            })
            .collect();
        normalized.sort();
        normalized.dedup();
        WatchSet {
            root,
            inputs: normalized,
        }
    }

    /// The workspace root, normalized.
    pub fn root(&self) -> &str {
        &self.root
    }

    /// How many inputs were kept.
    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    /// Whether no input was kept.
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    /// Classifies one event path.
    ///
    /// Runs on **every** event, not only at registration: a directory watch
    /// delivers events for files nobody asked for — an editor's `.swp` beside a
    /// watched `.rs`, a `4913` probe, a `document.json.tmp` — so registration-time
    /// filtering alone would let them through.
    pub fn classify(&self, path: &Path) -> Classification {
        let normalized = normalize(path);
        let Some(relative) = relative_to(&self.root, &normalized) else {
            return Classification::Outside;
        };

        if let Some(reason) = ignore_reason(&relative) {
            return Classification::Ignored(reason);
        }

        for (input, kind, excludes) in &self.inputs {
            if matches(input, *kind, excludes, &relative) {
                return Classification::Relevant(*kind);
            }
        }
        Classification::NotAnInput
    }

    /// Whether a path should start or extend a burst.
    pub fn is_relevant(&self, path: &Path) -> bool {
        self.classify(path).is_relevant()
    }

    /// Whether a directory that just appeared at `path` needs its subtree
    /// reconciled.
    ///
    /// True only for a directory lying inside a [`InputKind::RustSourceRoot`],
    /// which is the one input kind that matches **at any depth** and therefore the
    /// one whose coverage a new subdirectory can silently fall outside of. An
    /// `ExactFile` has no subtree, and `FlowsDir`/`OverridesDir` are non-recursive
    /// by design — a TOML in a subdirectory of `.cratevista/flows` is never loaded,
    /// so discovering one would be wrong rather than helpful.
    ///
    /// This decides only whether to *look*. Everything found is still classified by
    /// [`WatchSet::classify`] like any other path: the Rust root is not a relevance
    /// rule, and treating it as one would make every file in a new directory
    /// interesting.
    pub(crate) fn needs_subtree_reconciliation(&self, path: &Path) -> bool {
        let normalized = normalize(path);
        let Some(relative) = relative_to(&self.root, &normalized) else {
            return false;
        };
        if is_ignored_directory(&relative) {
            return false;
        }
        self.inputs.iter().any(|(input, kind, _)| {
            *kind == InputKind::RustSourceRoot && is_under(input, &relative)
        })
    }

    /// Whether a **directory** found while walking must not be descended into.
    ///
    /// Takes a real filesystem path and normalizes it here, because that is the
    /// only place normalization belongs: the root this compares against is a
    /// normalized *lexical* string, not a path the OS would recognize. On Windows a
    /// canonical path is verbatim (`\\?\C:\...`) while the stored root reads
    /// `//?/C:/...`, so `Path::strip_prefix` between them silently never matches.
    ///
    /// A path outside the workspace answers `true`: whatever it is, it is not ours
    /// to walk.
    pub(crate) fn is_ignored_directory(&self, path: &Path) -> bool {
        match relative_to(&self.root, &normalize(path)) {
            Some(relative) => is_ignored_directory(&relative),
            None => true,
        }
    }

    /// Whether a path lies inside this set's workspace root, lexically.
    ///
    /// Used on an already-**canonicalized** path, which is what turns this into a
    /// real containment check rather than a textual one.
    pub(crate) fn contains_path(&self, path: &Path) -> bool {
        relative_to(&self.root, &normalize(path)).is_some()
    }

    /// Filters event paths to the relevant ones, **sorted and deduplicated**.
    ///
    /// Deterministic regardless of the order the OS reported the events in — the
    /// same edit must always describe itself the same way.
    pub fn relevant<'a>(&self, paths: impl IntoIterator<Item = &'a Path>) -> Vec<PathBuf> {
        let unique: BTreeSet<String> = paths
            .into_iter()
            .filter(|path| self.is_relevant(path))
            .map(normalize)
            .collect();
        unique.into_iter().map(PathBuf::from).collect()
    }
}

/// Whether a normalized, root-relative event path matches one input.
fn matches(input: &str, kind: InputKind, excludes: &[String], relative: &str) -> bool {
    match kind {
        // Exact, so a sibling of a referenced doc is not an input. This is what
        // makes an *unreferenced* `.cratevista/docs/*.md` ignored while its
        // referenced neighbour is watched.
        InputKind::ExactFile => relative == input,
        // Any depth: a new module may appear in a new subdirectory.
        InputKind::RustSourceRoot => is_under(input, relative) && has_extension(relative, "rs"),
        // Directly inside only: discovery is non-recursive, so a TOML in a
        // subdirectory is never loaded and must not regenerate.
        InputKind::FlowsDir | InputKind::OverridesDir => {
            is_directly_inside(input, relative) && has_extension(relative, "toml")
        }
        InputKind::WorkspaceMemberManifestPattern => {
            matches_member_pattern(input, excludes, relative)
        }
    }
}

/// Whether an event path is a **candidate workspace-member manifest** for a
/// declared pattern.
///
/// The rule, in order:
///
/// 1. it must be a `Cargo.toml` — nothing else is a member manifest;
/// 2. a pattern that already names `Cargo.toml` matches the full path;
///    otherwise the pattern is matched against the manifest's **parent**, so
///    `crates/*` covers `crates/new/Cargo.toml`;
/// 3. exclusions are applied **last** and win.
///
/// This is why `crates/*` does not match `crates/a/nested/Cargo.toml`: the parent
/// is `crates/a/nested`, which is two components, and `*` spans one.
fn matches_member_pattern(pattern: &str, excludes: &[String], relative: &str) -> bool {
    if !is_manifest(relative) {
        return false;
    }

    let matched = if is_manifest_pattern(pattern) {
        crate::pattern::matches(pattern, relative)
    } else {
        match parent_of(relative) {
            Some(member) => crate::pattern::matches(pattern, member),
            // A bare `Cargo.toml` at the root has no member directory.
            None => false,
        }
    };
    if !matched {
        return false;
    }

    // Excludes are checked against the member directory and the manifest itself,
    // so `exclude = ["crates/skipped"]` drops both spellings.
    let member = parent_of(relative).unwrap_or("");
    !excludes.iter().any(|exclude| {
        exclude == member
            || exclude == relative
            || member.starts_with(&format!("{exclude}/"))
            || crate::pattern::matches(exclude, member)
            || crate::pattern::matches(exclude, relative)
    })
}

/// Whether a path names a `Cargo.toml`.
fn is_manifest(path: &str) -> bool {
    path == "Cargo.toml" || path.ends_with("/Cargo.toml")
}

/// Whether a pattern already spells out the manifest.
fn is_manifest_pattern(pattern: &str) -> bool {
    pattern == "Cargo.toml" || pattern.ends_with("/Cargo.toml")
}

/// The directory part of a path, or `None` when there is none.
fn parent_of(path: &str) -> Option<&str> {
    path.rfind('/').map(|index| &path[..index])
}

/// Whether `path` is `dir` itself or below it, on component boundaries — so
/// `srcfoo/x.rs` never matches the root `src`.
fn is_under(dir: &str, path: &str) -> bool {
    if dir.is_empty() {
        return true;
    }
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Whether `path` is a direct child of `dir` (no further separator).
fn is_directly_inside(dir: &str, path: &str) -> bool {
    let Some(rest) = path.strip_prefix(dir) else {
        return false;
    };
    let Some(name) = rest.strip_prefix('/') else {
        return false;
    };
    !name.is_empty() && !name.contains('/')
}

/// Case-insensitive extension test (`.RS` is still Rust on Windows).
fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

/// The ignore rules, applied to a normalized root-relative path.
///
/// Checked **before** input matching so an ignored location can never be revived
/// by an input that happens to overlap it.
fn ignore_reason(relative: &str) -> Option<IgnoreReason> {
    let components: Vec<&str> = relative
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    if components.first() == Some(&GENERATED_DIR) {
        return Some(IgnoreReason::GeneratedOutput);
    }
    if components.first() == Some(&"web")
        && matches!(components.get(1), Some(&"node_modules") | Some(&"dist"))
    {
        return Some(IgnoreReason::FrontendArtifacts);
    }

    for (index, component) in components.iter().enumerate() {
        let is_last = index + 1 == components.len();
        if *component == ".git" {
            return Some(IgnoreReason::VersionControl);
        }
        // A hidden *directory* is ignored; `.cratevista` is the one real input,
        // so it is excepted by name rather than by a general rule — the whole
        // configuration surface lives under it.
        if !is_last && component.starts_with('.') && *component != CONFIG_DIR {
            return Some(IgnoreReason::HiddenDirectory);
        }
        if is_last && is_editor_temporary(component) {
            return Some(IgnoreReason::EditorTemporary);
        }
    }
    None
}

/// Whether a **directory** at this root-relative path must not be descended into.
///
/// Defined by asking [`ignore_reason`] about a synthetic Rust file inside the
/// directory rather than about the directory itself. That is deliberate:
/// `ignore_reason` judges a path's **last** component as a file name (a hidden
/// *file* is fine, a hidden *directory* is not), so asking it about `src/.hidden`
/// directly would answer the wrong question and return `None`.
///
/// Deriving it this way means the traversal and the classifier can never disagree:
/// a directory is skipped exactly when everything inside it would be ignored
/// anyway, and `.cratevista`'s exception is inherited rather than restated.
pub(crate) fn is_ignored_directory(relative: &str) -> bool {
    ignore_reason(&format!("{relative}/probe.rs")).is_some()
}

/// Whether a file name is an editor backup, swap or probe artifact.
///
/// These are dropped **on their own name**. The rename that follows is a separate
/// event naming the real destination, which classifies on its own merits — so
/// "save via write-temp-then-rename" still regenerates, and only the noise is
/// filtered.
fn is_editor_temporary(name: &str) -> bool {
    // Vim: `4913` probe; `.swp`/`.swx`/`~`. Emacs: `.#foo`, `#foo#`.
    // Editors/tools generally: `*.tmp`, `*.bak`, JetBrains `*___jb_tmp___`.
    if name == "4913" {
        return true;
    }
    if name.ends_with('~')
        || name.starts_with(".#")
        || (name.starts_with('#') && name.ends_with('#'))
    {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".swp")
        || lower.ends_with(".swx")
        || lower.ends_with(".swo")
        || lower.ends_with(".tmp")
        || lower.ends_with(".bak")
        || lower.contains("___jb_tmp___")
        || lower.contains("___jb_old___")
}

/// Lexically normalizes a path: `\` → `/`, `.` dropped, `..` popped, redundant
/// separators collapsed. **No filesystem access and no symlink resolution.**
///
/// `..` is resolved textually, which is what makes an escape detectable by
/// [`relative_to`]: `<root>/../secrets` normalizes to a path that is no longer
/// under the root, so it is rejected instead of watched.
fn normalize(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");

    // Preserve a leading `/` (Unix absolute) or a `//` UNC-ish prefix so the root
    // and the event path stay comparable.
    let leading = if text.starts_with("//") {
        "//"
    } else if text.starts_with('/') {
        "/"
    } else {
        ""
    };

    let mut parts: Vec<&str> = Vec::new();
    for component in text.split('/') {
        match component {
            "" | "." => continue,
            ".." => {
                // Popping past the start keeps the `..`, so the result stays
                // outside any root and `relative_to` rejects it.
                if matches!(parts.last(), Some(&"..") | None) {
                    parts.push("..");
                } else {
                    parts.pop();
                }
            }
            other => parts.push(other),
        }
    }
    format!("{leading}{}", parts.join("/"))
}

/// The part of `path` below `root`, or `None` if `path` is not lexically inside.
///
/// Comparison is case-sensitive: a case-insensitive filesystem would make
/// `SRC/lib.rs` and `src/lib.rs` the same file, but core supplies the root and
/// the OS reports events using the same spelling it was given, so normalizing
/// case here would only create false matches on case-sensitive systems.
fn relative_to(root: &str, path: &str) -> Option<String> {
    if path == root {
        return Some(String::new());
    }
    let rest = path.strip_prefix(root)?;
    let rest = rest.strip_prefix('/')?;
    if rest.is_empty() || rest.starts_with("..") {
        return None;
    }
    Some(rest.to_string())
}

/// Whether a path is absolute in the lexical sense this module uses.
///
/// Exposed so core can assert its own inputs before handing them over; this crate
/// never needs to ask, because it compares against a root it was given.
pub fn is_lexically_absolute(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text.starts_with('/') || text.starts_with('\\') {
        return true;
    }
    // `C:` / `C:/` / `C:\`
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && (bytes[0] as char).is_ascii_alphabetic() {
        return true;
    }
    path.components()
        .next()
        .is_some_and(|first| matches!(first, Component::Prefix(_) | Component::RootDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A workspace root spelled the way each platform's paths arrive.
    const ROOT: &str = "/w";

    fn set() -> WatchSet {
        WatchSet::new(
            Path::new(ROOT),
            [
                WatchInput::file("/w/Cargo.toml"),
                WatchInput::file("/w/Cargo.lock"),
                WatchInput::file("/w/crates/demo/Cargo.toml"),
                WatchInput::file("/w/cratevista.toml"),
                WatchInput::file("/w/.cratevista/docs/checkout.md"),
                WatchInput::file("/w/.cratevista/examples/req.http"),
                WatchInput::rust_root("/w/crates/demo/src"),
                WatchInput::flows_dir("/w/.cratevista/flows"),
                WatchInput::overrides_dir("/w/.cratevista/overrides"),
            ],
        )
    }

    fn classify(path: &str) -> Classification {
        set().classify(Path::new(path))
    }

    // --- exact files ------------------------------------------------------

    #[test]
    fn the_manifests_and_lockfile_are_relevant() {
        for path in [
            "/w/Cargo.toml",
            "/w/Cargo.lock",
            "/w/crates/demo/Cargo.toml",
            "/w/cratevista.toml",
        ] {
            assert_eq!(
                classify(path),
                Classification::Relevant(InputKind::ExactFile),
                "{path}"
            );
        }
    }

    #[test]
    fn an_undeclared_manifest_is_not_an_input() {
        // Only the members core actually resolved are watched.
        assert_eq!(
            classify("/w/crates/other/Cargo.toml"),
            Classification::NotAnInput
        );
    }

    // --- rust source roots (recursive, *.rs only) -------------------------

    #[test]
    fn rust_files_at_any_depth_below_a_source_root_are_relevant() {
        for path in [
            "/w/crates/demo/src/lib.rs",
            "/w/crates/demo/src/deep/nested/module.rs",
        ] {
            assert_eq!(
                classify(path),
                Classification::Relevant(InputKind::RustSourceRoot),
                "{path}"
            );
        }
    }

    #[test]
    fn a_new_rust_file_below_a_source_root_is_relevant() {
        // Nothing here consults the filesystem, so "new" is not a special case:
        // the path is classified on its text alone.
        assert_eq!(
            classify("/w/crates/demo/src/brand_new.rs"),
            Classification::Relevant(InputKind::RustSourceRoot)
        );
    }

    #[test]
    fn a_non_rust_file_below_a_source_root_is_not_an_input() {
        for path in [
            "/w/crates/demo/src/notes.md",
            "/w/crates/demo/src/data.json",
            "/w/crates/demo/src/rs",
        ] {
            assert_eq!(classify(path), Classification::NotAnInput, "{path}");
        }
    }

    #[test]
    fn a_rust_extension_is_matched_case_insensitively() {
        assert_eq!(
            classify("/w/crates/demo/src/Lib.RS"),
            Classification::Relevant(InputKind::RustSourceRoot)
        );
    }

    #[test]
    fn a_sibling_directory_sharing_the_roots_prefix_is_not_under_it() {
        // `src` must not match `srcgen` — the guard is a component boundary, not
        // a string prefix.
        assert_eq!(
            classify("/w/crates/demo/srcgen/lib.rs"),
            Classification::NotAnInput
        );
    }

    // --- config directories (non-recursive, *.toml) -----------------------

    #[test]
    fn a_new_flow_or_override_toml_is_relevant() {
        assert_eq!(
            classify("/w/.cratevista/flows/architecture.toml"),
            Classification::Relevant(InputKind::FlowsDir)
        );
        assert_eq!(
            classify("/w/.cratevista/overrides/presentation.toml"),
            Classification::Relevant(InputKind::OverridesDir)
        );
    }

    #[test]
    fn a_toml_nested_below_a_config_directory_is_not_an_input() {
        // Discovery is non-recursive, so this file is never loaded; regenerating
        // for it would be a lie about what changed.
        assert_eq!(
            classify("/w/.cratevista/flows/nested/deep.toml"),
            Classification::NotAnInput
        );
    }

    #[test]
    fn a_non_toml_inside_a_config_directory_is_not_an_input() {
        assert_eq!(
            classify("/w/.cratevista/flows/README.md"),
            Classification::NotAnInput
        );
    }

    // --- referenced vs unreferenced docs ----------------------------------

    #[test]
    fn an_explicitly_referenced_doc_or_example_is_relevant() {
        assert_eq!(
            classify("/w/.cratevista/docs/checkout.md"),
            Classification::Relevant(InputKind::ExactFile)
        );
        assert_eq!(
            classify("/w/.cratevista/examples/req.http"),
            Classification::Relevant(InputKind::ExactFile)
        );
    }

    #[test]
    fn an_unreferenced_doc_beside_a_referenced_one_is_ignored() {
        // PRD 08's opt-in rule: only what configuration names is an input.
        assert_eq!(
            classify("/w/.cratevista/docs/scratch.md"),
            Classification::NotAnInput
        );
    }

    #[test]
    fn a_referenced_file_that_does_not_exist_is_still_classifiable() {
        // The point of staying lexical: this path is watched precisely so that
        // creating it regenerates.
        let missing = WatchSet::new(
            Path::new(ROOT),
            [WatchInput::file("/w/.cratevista/docs/not-created-yet.md")],
        );
        assert_eq!(
            missing.classify(Path::new("/w/.cratevista/docs/not-created-yet.md")),
            Classification::Relevant(InputKind::ExactFile)
        );
    }

    // --- ignore rules -----------------------------------------------------

    #[test]
    fn our_own_generated_output_is_ignored() {
        // The loop guard: these are the files we write on every generation.
        for path in [
            "/w/target/cratevista/document.json",
            "/w/target/cratevista/generation.json",
            "/w/target/cratevista/diagnostics.json",
            "/w/target/debug/demo.exe",
        ] {
            assert_eq!(
                classify(path),
                Classification::Ignored(IgnoreReason::GeneratedOutput),
                "{path}"
            );
        }
    }

    #[test]
    fn version_control_and_frontend_artifacts_are_ignored() {
        assert_eq!(
            classify("/w/.git/index"),
            Classification::Ignored(IgnoreReason::VersionControl)
        );
        assert_eq!(
            classify("/w/web/node_modules/react/index.js"),
            Classification::Ignored(IgnoreReason::FrontendArtifacts)
        );
        assert_eq!(
            classify("/w/web/dist/assets/app.js"),
            Classification::Ignored(IgnoreReason::FrontendArtifacts)
        );
    }

    #[test]
    fn web_source_is_not_swept_up_by_the_frontend_ignore() {
        // Only `node_modules` and `dist` are artifacts; `web/src` is not ignored
        // (it is simply not a declared input here).
        assert_eq!(classify("/w/web/src/App.tsx"), Classification::NotAnInput);
    }

    #[test]
    fn unrelated_hidden_directories_are_ignored() {
        for path in [
            "/w/.idea/workspace.xml",
            "/w/.vscode/settings.json",
            "/w/crates/demo/.cache/blob",
        ] {
            assert_eq!(
                classify(path),
                Classification::Ignored(IgnoreReason::HiddenDirectory),
                "{path}"
            );
        }
    }

    #[test]
    fn the_cratevista_directory_survives_the_hidden_directory_rule() {
        // The exception that the whole configuration surface depends on: a blanket
        // "ignore dotted directories" would silently stop watching every flow.
        assert!(
            classify("/w/.cratevista/flows/a.toml").is_relevant(),
            "the config directory must not be swept up by the hidden rule"
        );
        assert!(classify("/w/.cratevista/docs/checkout.md").is_relevant());
    }

    #[test]
    fn a_hidden_file_is_not_treated_as_a_hidden_directory() {
        // `.rustfmt.toml` is a hidden *file*, not a directory: the rule applies to
        // non-final components only.
        assert_eq!(classify("/w/.rustfmt.toml"), Classification::NotAnInput);
    }

    // --- editor noise -----------------------------------------------------

    #[test]
    fn editor_backup_and_temp_files_are_ignored() {
        for path in [
            "/w/crates/demo/src/lib.rs~",
            "/w/crates/demo/src/.lib.rs.swp",
            "/w/crates/demo/src/.#lib.rs",
            "/w/crates/demo/src/#lib.rs#",
            "/w/crates/demo/src/4913",
            "/w/crates/demo/src/lib.rs.tmp",
            "/w/crates/demo/src/lib.rs.bak",
            "/w/crates/demo/src/lib.rs___jb_tmp___",
        ] {
            assert_eq!(
                classify(path),
                Classification::Ignored(IgnoreReason::EditorTemporary),
                "{path}"
            );
        }
    }

    #[test]
    fn a_save_via_write_temp_then_rename_still_regenerates() {
        // The realistic sequence: the noise is dropped, and the rename's
        // destination — a real `.rs` — is relevant on its own merits.
        let set = set();
        let sequence = [
            "/w/crates/demo/src/4913",        // vim probe: create+delete
            "/w/crates/demo/src/.lib.rs.swp", // swap file
            "/w/crates/demo/src/lib.rs.tmp",  // temp write
            "/w/crates/demo/src/lib.rs",      // rename destination
        ];
        let relevant = set.relevant(sequence.iter().map(Path::new));
        assert_eq!(relevant, [PathBuf::from("/w/crates/demo/src/lib.rs")]);
    }

    #[test]
    fn a_temp_file_whose_destination_is_irrelevant_stays_irrelevant() {
        // Renaming to a non-input yields nothing: the destination is what counts,
        // and it is judged by the same rules.
        let set = set();
        let relevant = set.relevant(
            [
                "/w/crates/demo/src/notes.md.tmp",
                "/w/crates/demo/src/notes.md",
            ]
            .iter()
            .map(Path::new),
        );
        assert!(relevant.is_empty());
    }

    // --- containment ------------------------------------------------------

    #[test]
    fn a_path_lexically_outside_the_workspace_is_rejected() {
        for path in ["/elsewhere/x.rs", "/w/../secrets.md", "/", "/w2/src/lib.rs"] {
            assert_eq!(classify(path), Classification::Outside, "{path}");
        }
    }

    #[test]
    fn traversal_back_inside_the_root_normalizes_and_is_accepted() {
        // `/w/crates/../crates/demo/src/lib.rs` names a real input; `..` is
        // resolved textually, so the path is recognized rather than refused.
        assert_eq!(
            classify("/w/crates/../crates/demo/src/lib.rs"),
            Classification::Relevant(InputKind::RustSourceRoot)
        );
    }

    #[test]
    fn the_root_itself_is_not_an_input() {
        assert_eq!(classify("/w"), Classification::NotAnInput);
    }

    // --- path spellings ---------------------------------------------------

    #[test]
    fn windows_style_separators_classify_the_same_as_unix_ones() {
        // The OS reports whatever separator it likes; normalization makes both
        // spellings one path, without assuming which platform we are on.
        let set = WatchSet::new(
            Path::new("C:/w"),
            [
                WatchInput::rust_root("C:/w/crates/demo/src"),
                WatchInput::flows_dir("C:/w/.cratevista/flows"),
            ],
        );
        assert_eq!(
            set.classify(Path::new(r"C:\w\crates\demo\src\lib.rs")),
            Classification::Relevant(InputKind::RustSourceRoot)
        );
        assert_eq!(
            set.classify(Path::new(r"C:\w\.cratevista\flows\a.toml")),
            Classification::Relevant(InputKind::FlowsDir)
        );
        assert_eq!(
            set.classify(Path::new(r"C:\w\target\cratevista\document.json")),
            Classification::Ignored(IgnoreReason::GeneratedOutput)
        );
        assert_eq!(
            set.classify(Path::new(r"D:\other\lib.rs")),
            Classification::Outside
        );
    }

    #[test]
    fn a_windows_root_declared_with_backslashes_matches_forward_slash_events() {
        let set = WatchSet::new(
            Path::new(r"C:\w"),
            [WatchInput::rust_root(r"C:\w\crates\demo\src")],
        );
        assert_eq!(
            set.classify(Path::new("C:/w/crates/demo/src/lib.rs")),
            Classification::Relevant(InputKind::RustSourceRoot)
        );
    }

    #[test]
    fn redundant_separators_and_dot_components_normalize_away() {
        assert_eq!(
            classify("/w//crates/./demo//src/lib.rs"),
            Classification::Relevant(InputKind::RustSourceRoot)
        );
    }

    #[test]
    fn lexical_absoluteness_covers_unix_windows_and_unc_spellings() {
        for path in ["/x", r"C:\x", "C:/x", r"\\server\share\x"] {
            assert!(is_lexically_absolute(Path::new(path)), "{path}");
        }
        for path in ["x", "src/lib.rs", "./x"] {
            assert!(!is_lexically_absolute(Path::new(path)), "{path}");
        }
    }

    // --- set behavior -----------------------------------------------------

    #[test]
    fn relevant_output_is_sorted_and_deduplicated() {
        let set = set();
        let events = [
            "/w/crates/demo/src/z.rs",
            "/w/Cargo.toml",
            "/w/crates/demo/src/a.rs",
            "/w/crates/demo/src/z.rs",            // duplicate
            "/w/crates/demo/src/./a.rs",          // same file, different spelling
            "/w/target/cratevista/document.json", // ignored
        ];
        let relevant = set.relevant(events.iter().map(Path::new));
        assert_eq!(
            relevant,
            [
                PathBuf::from("/w/Cargo.toml"),
                PathBuf::from("/w/crates/demo/src/a.rs"),
                PathBuf::from("/w/crates/demo/src/z.rs"),
            ]
        );
    }

    #[test]
    fn classification_is_deterministic_across_repeated_runs() {
        let events: Vec<&Path> = [
            "/w/crates/demo/src/lib.rs",
            "/w/target/cratevista/document.json",
            "/w/.cratevista/flows/a.toml",
            "/w/.git/index",
        ]
        .iter()
        .map(Path::new)
        .collect();

        let first = set().relevant(events.iter().copied());
        for _ in 0..5 {
            assert_eq!(set().relevant(events.iter().copied()), first);
        }
    }

    #[test]
    fn inputs_are_normalized_deduplicated_and_contained_at_construction() {
        let set = WatchSet::new(
            Path::new(ROOT),
            [
                WatchInput::file("/w/Cargo.toml"),
                WatchInput::file("/w/./Cargo.toml"), // same input, spelled twice
                WatchInput::file("/outside/x.toml"), // dropped: not inside
                WatchInput::file("/w/../escape.toml"), // dropped: escapes
            ],
        );
        assert_eq!(set.len(), 1);
        assert!(!set.is_empty());
        assert_eq!(set.root(), "/w");
        assert!(set.is_relevant(Path::new("/w/Cargo.toml")));
        assert_eq!(
            set.classify(Path::new("/outside/x.toml")),
            Classification::Outside
        );
    }

    #[test]
    fn an_empty_set_finds_nothing_relevant_but_still_ignores_output() {
        let set = WatchSet::new(Path::new(ROOT), []);
        assert!(set.is_empty());
        assert_eq!(
            set.classify(Path::new("/w/crates/demo/src/lib.rs")),
            Classification::NotAnInput
        );
        assert_eq!(
            set.classify(Path::new("/w/target/cratevista/document.json")),
            Classification::Ignored(IgnoreReason::GeneratedOutput)
        );
    }

    // --- workspace-member manifest patterns -------------------------------

    /// A set whose only input is one member pattern, so the assertions are about
    /// the pattern rule and nothing else.
    fn member_set(pattern: &str, excludes: &[&str]) -> WatchSet {
        WatchSet::new(
            Path::new(ROOT),
            [WatchInput::workspace_member_pattern(
                format!("/w/{pattern}"),
                excludes.iter().map(|entry| entry.to_string()),
            )],
        )
    }

    #[test]
    fn a_member_pattern_matches_a_manifest_that_does_not_exist_yet() {
        // The whole reason this kind exists: `crates/*` covers a member created
        // later, without the root manifest changing again.
        let set = member_set("crates/*", &[]);
        assert_eq!(
            set.classify(Path::new("/w/crates/new/Cargo.toml")),
            Classification::Relevant(InputKind::WorkspaceMemberManifestPattern)
        );
    }

    #[test]
    fn a_member_pattern_matches_only_cargo_manifests() {
        let set = member_set("crates/*", &[]);
        assert_eq!(
            set.classify(Path::new("/w/crates/new/src/lib.rs")),
            Classification::NotAnInput,
            "a member pattern says nothing about sources — the Rust root does"
        );
        assert_eq!(
            set.classify(Path::new("/w/crates/new/README.md")),
            Classification::NotAnInput
        );
        // `.bak` is caught by the editor-temporary rule first — which is correct,
        // and worth pinning: the ignore rules run before any input matching.
        assert_eq!(
            set.classify(Path::new("/w/crates/new/Cargo.toml.bak")),
            Classification::Ignored(IgnoreReason::EditorTemporary)
        );
        // A manifest-like name that is not a manifest is simply not an input.
        assert_eq!(
            set.classify(Path::new("/w/crates/new/Cargo.toml.orig")),
            Classification::NotAnInput
        );
    }

    #[test]
    fn a_single_star_member_pattern_does_not_match_a_nested_manifest() {
        // `crates/*` matches the member directory `crates/a`; `crates/a/nested` is
        // two components, so a vendored manifest under a member is not a member.
        let set = member_set("crates/*", &[]);
        assert_eq!(
            set.classify(Path::new("/w/crates/a/nested/Cargo.toml")),
            Classification::NotAnInput
        );
        assert_eq!(
            set.classify(Path::new("/w/elsewhere/other/Cargo.toml")),
            Classification::NotAnInput
        );
    }

    #[test]
    fn a_partial_star_member_pattern_is_load_bearing() {
        // The case that proves the pattern predicate does real work: a static
        // prefix alone would accept both.
        let set = member_set("crates/a*", &[]);
        assert_eq!(
            set.classify(Path::new("/w/crates/api/Cargo.toml")),
            Classification::Relevant(InputKind::WorkspaceMemberManifestPattern)
        );
        assert_eq!(
            set.classify(Path::new("/w/crates/billing/Cargo.toml")),
            Classification::NotAnInput,
            "`crates/a*` must not accept `billing`"
        );
    }

    #[test]
    fn a_pattern_that_names_the_manifest_matches_the_full_path() {
        let set = member_set("crates/*/Cargo.toml", &[]);
        assert_eq!(
            set.classify(Path::new("/w/crates/new/Cargo.toml")),
            Classification::Relevant(InputKind::WorkspaceMemberManifestPattern)
        );
        assert_eq!(
            set.classify(Path::new("/w/crates/a/nested/Cargo.toml")),
            Classification::NotAnInput
        );
    }

    #[test]
    fn a_recursive_pattern_spans_components_only_when_declared() {
        let set = member_set("crates/**", &[]);
        assert_eq!(
            set.classify(Path::new("/w/crates/a/nested/Cargo.toml")),
            Classification::Relevant(InputKind::WorkspaceMemberManifestPattern),
            "`**` was asked for, so depth is intended here"
        );
        // And a non-recursive declaration still refuses it.
        assert_eq!(
            member_set("crates/*", &[]).classify(Path::new("/w/crates/a/nested/Cargo.toml")),
            Classification::NotAnInput
        );
    }

    #[test]
    fn an_excluded_existing_or_future_member_is_never_relevant() {
        let set = member_set("crates/*", &["crates/skipped"]);
        assert_eq!(
            set.classify(Path::new("/w/crates/kept/Cargo.toml")),
            Classification::Relevant(InputKind::WorkspaceMemberManifestPattern)
        );
        assert_eq!(
            set.classify(Path::new("/w/crates/skipped/Cargo.toml")),
            Classification::NotAnInput,
            "an exclude wins over a match, for an existing member"
        );
        // The same holds for one created later — the rule is textual, not a
        // directory listing.
        assert_eq!(
            set.classify(Path::new("/w/crates/skipped/Cargo.toml")),
            Classification::NotAnInput
        );
    }

    #[test]
    fn an_exclude_may_itself_be_a_pattern() {
        let set = member_set("crates/*", &["crates/experimental-*"]);
        assert_eq!(
            set.classify(Path::new("/w/crates/api/Cargo.toml")),
            Classification::Relevant(InputKind::WorkspaceMemberManifestPattern)
        );
        assert_eq!(
            set.classify(Path::new("/w/crates/experimental-thing/Cargo.toml")),
            Classification::NotAnInput
        );
    }

    #[test]
    fn a_member_pattern_never_reaches_outside_the_workspace() {
        let set = member_set("crates/*", &[]);
        assert_eq!(
            set.classify(Path::new("/elsewhere/crates/new/Cargo.toml")),
            Classification::Outside
        );
        assert_eq!(
            set.classify(Path::new("/w/../outside/crates/new/Cargo.toml")),
            Classification::Outside
        );
    }

    #[test]
    fn a_member_pattern_still_loses_to_the_ignore_rules() {
        // Our own output and vendored trees are never members, whatever the
        // pattern says.
        let set = member_set("**", &[]);
        assert_eq!(
            set.classify(Path::new("/w/target/package/thing/Cargo.toml")),
            Classification::Ignored(IgnoreReason::GeneratedOutput)
        );
        assert_eq!(
            set.classify(Path::new("/w/web/node_modules/x/Cargo.toml")),
            Classification::Ignored(IgnoreReason::FrontendArtifacts)
        );
    }

    #[test]
    fn a_member_pattern_classifies_windows_and_unix_spellings_alike() {
        let set = WatchSet::new(
            Path::new("C:/w"),
            [WatchInput::workspace_member_pattern("C:/w/crates/*", [])],
        );
        assert_eq!(
            set.classify(Path::new(r"C:\w\crates\new\Cargo.toml")),
            Classification::Relevant(InputKind::WorkspaceMemberManifestPattern)
        );
        assert_eq!(
            set.classify(Path::new("C:/w/crates/new/Cargo.toml")),
            Classification::Relevant(InputKind::WorkspaceMemberManifestPattern)
        );
    }

    #[test]
    fn a_malformed_pattern_never_broadens_into_every_manifest() {
        // Failing closed: the alternative is that one typo makes every vendored
        // manifest in the tree a workspace member.
        let set = member_set("crates/[unterminated", &[]);
        assert_eq!(
            set.classify(Path::new("/w/crates/anything/Cargo.toml")),
            Classification::NotAnInput
        );
        assert_eq!(
            set.classify(Path::new("/w/vendor/other/Cargo.toml")),
            Classification::NotAnInput
        );
    }

    #[test]
    fn the_root_manifest_is_not_a_member_of_its_own_pattern() {
        let set = member_set("*", &[]);
        assert_eq!(
            set.classify(Path::new("/w/Cargo.toml")),
            Classification::NotAnInput,
            "a bare root Cargo.toml has no member directory; it is watched as an \
             exact file instead"
        );
    }

    #[test]
    fn pattern_excludes_are_sorted_and_deduplicated_at_construction() {
        let input = WatchInput::workspace_member_pattern(
            "/w/crates/*",
            ["b".to_string(), "a".to_string(), "b".to_string()],
        );
        assert_eq!(input.excludes, ["a", "b"]);
        assert_eq!(input.kind, InputKind::WorkspaceMemberManifestPattern);
    }
}
