//! Resolving and embedding file-backed documentation and examples.
//!
//! This is the **only** module in the crate that touches the filesystem, and it
//! is deliberately narrow:
//!
//! - it reads **only files an author named explicitly** — there is no globbing,
//!   no directory walking, and no implicit discovery of content;
//! - every path goes through [`RepoRelativePath`], which rejects absolute paths,
//!   drive letters, UNC paths and any `..`;
//! - the **resolved** file must still be inside the canonical workspace root, so
//!   a symlink cannot smuggle content out of the project;
//! - contents are embedded verbatim after UTF-8 decoding, so the explorer can
//!   render them without `/api/source` and a static export stays self-contained.
//!
//! Every failure is **local**: a bad doc or example is dropped with a located
//! diagnostic and everything else still embeds.
//!
//! Because embedded content ships in `document.json` to every client regardless
//! of `--source`, examples are capped (see [`MAX_EXAMPLE_BYTES`]) and **never
//! truncated** — a partial example would be a silent lie about what the file
//! contains.

use std::path::{Path, PathBuf};

use cratevista_graph::GraphOverlay;
use cratevista_schema::{DocBlock, EntityId, LocalizedText, RepoRelativePath, ViewExample, ViewId};
use serde_spanned::Spanned;

use crate::error::{ConfigDiagnostic, code};
use crate::model::{RawConfig, RawLocalized};
use crate::overlay::accepted_flows;
use crate::validate::Validation;

/// The per-example size cap: **64 KiB**.
///
/// Far below `/api/source`'s 1 MiB, because an example is embedded in the
/// document and therefore shipped on **every** `/api/document` fetch, whereas
/// `/api/source` serves on demand. An oversize example is dropped whole.
pub const MAX_EXAMPLE_BYTES: usize = 64 * 1024;

/// Why a referenced file could not be embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    /// The path is absolute, escapes via `..`, or is otherwise not repo-relative.
    InvalidPath(String),
    /// Nothing exists at that path.
    Missing,
    /// It exists but is a directory or other non-file.
    NotAFile,
    /// It resolves (via a symlink) outside the workspace root.
    Escapes,
    /// It could not be read.
    Unreadable(std::io::ErrorKind),
    /// It is not valid UTF-8 text.
    NotUtf8,
    /// It exceeds the cap. Carries the real size so the message is actionable.
    TooLarge { bytes: u64, limit: usize },
}

impl FileError {
    /// The stable diagnostic code for this failure.
    fn code(&self, is_example: bool) -> &'static str {
        match self {
            FileError::InvalidPath(_) => code::INVALID_FILE_PATH,
            FileError::Missing => code::MISSING_FILE,
            FileError::NotAFile => code::NOT_A_FILE,
            FileError::Escapes => code::PATH_ESCAPES_WORKSPACE,
            FileError::Unreadable(_) => code::READ_FAILED,
            FileError::NotUtf8 => {
                if is_example {
                    code::EXAMPLE_NOT_UTF8
                } else {
                    code::NOT_UTF8
                }
            }
            FileError::TooLarge { .. } => code::EXAMPLE_TOO_LARGE,
        }
    }

    /// A message that never contains an absolute path — only the author's own
    /// repo-relative spelling, which the caller prepends.
    fn describe(&self) -> String {
        match self {
            FileError::InvalidPath(reason) => {
                format!("is not a valid repo-relative path: {reason}")
            }
            FileError::Missing => "does not exist".into(),
            FileError::NotAFile => "is not a regular file".into(),
            FileError::Escapes => {
                "resolves outside the workspace (a symlink may point out of the project)".into()
            }
            FileError::Unreadable(kind) => format!("could not be read: {kind}"),
            FileError::NotUtf8 => "is not valid UTF-8 text".into(),
            FileError::TooLarge { bytes, limit } => format!(
                "is {bytes} bytes, over the {limit}-byte limit for embedded examples; \
                 it is dropped rather than truncated"
            ),
        }
    }
}

/// Reads explicitly named files from inside one workspace.
///
/// Holds the canonicalized root so the containment check is done against a
/// symlink-resolved path rather than a textual prefix.
#[derive(Debug, Clone)]
pub struct WorkspaceFiles {
    root: PathBuf,
    /// The canonical root. `None` when the root itself cannot be canonicalized,
    /// in which case containment cannot be proven and every read is refused —
    /// failing closed rather than open.
    canonical_root: Option<PathBuf>,
}

impl WorkspaceFiles {
    /// Prepares a reader rooted at `workspace_root`.
    pub fn new(workspace_root: &Path) -> Self {
        WorkspaceFiles {
            root: workspace_root.to_path_buf(),
            canonical_root: workspace_root.canonicalize().ok(),
        }
    }

    /// True when `candidate` (already canonical) lies inside the canonical root.
    ///
    /// Compares resolved paths, not the author's text: `..` is already rejected
    /// by [`RepoRelativePath`], so the case this defends against is a **symlink**
    /// inside the workspace pointing out of it.
    fn contains(&self, candidate: &Path) -> bool {
        match &self.canonical_root {
            Some(root) => candidate.starts_with(root),
            // Without a canonical root there is nothing to prove containment
            // against, so refuse rather than assume.
            None => false,
        }
    }

    /// Reads a repo-relative text file, enforcing validation, containment and an
    /// optional byte cap.
    ///
    /// The cap is checked against the file's real size **before** reading, so an
    /// enormous file is never pulled into memory just to be rejected.
    pub fn read_text(&self, raw_path: &str, max_bytes: Option<usize>) -> Result<String, FileError> {
        // 1. The author's path must be repo-relative: no absolute, no drive
        //    letter, no UNC, no `..`.
        let repo_relative = RepoRelativePath::new(raw_path)
            .map_err(|error| FileError::InvalidPath(error.to_string()))?;

        // 2. Resolve it, following any symlinks.
        let joined = self.root.join(repo_relative.as_str());
        let canonical = joined.canonicalize().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => FileError::Missing,
            kind => FileError::Unreadable(kind),
        })?;

        // 3. The RESOLVED file must still be inside the workspace. This is the
        //    symlink guard: step 1 cannot see through a link.
        if !self.contains(&canonical) {
            return Err(FileError::Escapes);
        }

        // 4. A directory is not content.
        let metadata =
            std::fs::metadata(&canonical).map_err(|error| FileError::Unreadable(error.kind()))?;
        if !metadata.is_file() {
            return Err(FileError::NotAFile);
        }

        // 5. Size first: never read a huge file just to reject it, and never
        //    truncate.
        if let Some(limit) = max_bytes
            && metadata.len() > limit as u64
        {
            return Err(FileError::TooLarge {
                bytes: metadata.len(),
                limit,
            });
        }

        // 6. Bytes, then a strict UTF-8 decode — no lossy replacement, which
        //    would silently corrupt the embedded content.
        let bytes =
            std::fs::read(&canonical).map_err(|error| FileError::Unreadable(error.kind()))?;
        String::from_utf8(bytes).map_err(|_| FileError::NotUtf8)
    }
}

/// Joins Markdown blocks with exactly one blank line between them.
///
/// Mirrors `cratevista_graph::overlay`'s discovered/manual join (PRD-08
/// Amendment B): only newlines *adjoining the junction* are normalized, so
/// indentation, trailing spaces and interior blank lines survive byte-for-byte.
/// Duplicated rather than shared because that function is a private detail of
/// the graph's override merge; the rule is pinned by a test here.
fn join_markdown(blocks: &[String]) -> String {
    const NEWLINES: [char; 2] = ['\n', '\r'];
    let mut joined = String::new();
    for block in blocks {
        let trimmed = block.trim_matches(NEWLINES);
        if trimmed.is_empty() {
            continue;
        }
        if !joined.is_empty() {
            joined.push_str("\n\n");
        }
        joined.push_str(trimmed);
    }
    joined
}

fn localized(raw: &RawLocalized) -> LocalizedText {
    match raw {
        RawLocalized::Plain(text) => LocalizedText::new(text.clone()),
        RawLocalized::Translations(map) => {
            let mut text = LocalizedText::new(map.get("default").cloned().unwrap_or_default());
            for (language, value) in map {
                if language != "default" {
                    text.translations.insert(language.clone(), value.clone());
                }
            }
            text
        }
    }
}

/// Builds a located diagnostic for a failed file reference.
fn file_diagnostic<T>(
    file_path: &str,
    source: &str,
    spanned: &Spanned<T>,
    raw_path: &str,
    error: &FileError,
    is_example: bool,
    context: &str,
) -> ConfigDiagnostic {
    // `raw_path` is the author's own repo-relative spelling, so the message
    // stays free of absolute paths.
    ConfigDiagnostic::new(
        error.code(is_example),
        format!("{context}: `{raw_path}` {}", error.describe()),
        file_path,
    )
    .at_position(crate::error::position_of(source, spanned.span().start))
}

/// Reads every doc path of one flow/override, in declaration order.
fn read_docs(
    files: &WorkspaceFiles,
    file_path: &str,
    source: &str,
    paths: &[Spanned<String>],
    context: &str,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> Vec<String> {
    let mut blocks = Vec::new();
    for path in paths {
        let raw_path = path.get_ref().trim();
        // Docs are uncapped: unlike examples, they are prose the author wrote
        // for this document. See the PRD's step-4 ledger.
        match files.read_text(raw_path, None) {
            Ok(text) => blocks.push(text),
            Err(error) => diagnostics.push(file_diagnostic(
                file_path, source, path, raw_path, &error, false, context,
            )),
        }
    }
    blocks
}

/// Resolves and embeds every file-backed doc and example into `overlay`.
///
/// Call **after** [`crate::overlay::build_overlay`]: this fills in what step 3
/// deliberately left empty. Returns the diagnostics for anything that could not
/// be embedded; the overlay keeps everything that could.
pub fn embed_files(
    workspace_root: &Path,
    config: &RawConfig,
    validation: &Validation,
    overlay: &mut GraphOverlay,
) -> Vec<ConfigDiagnostic> {
    let files = WorkspaceFiles::new(workspace_root);
    let mut diagnostics = Vec::new();

    // --- flows: docs + examples -------------------------------------------
    for (file, flow) in accepted_flows(config, validation) {
        let flow_id = flow.id.get_ref().trim();
        let view_id = ViewId::view(flow_id);
        // Correlate by id rather than by index: `build_overlay` skips duplicate
        // flows, so positions would not line up.
        let Some(view) = overlay.views.iter_mut().find(|view| view.id == view_id) else {
            continue;
        };

        let blocks = read_docs(
            &files,
            &file.path,
            &file.source,
            &flow.docs,
            &format!("flow `{flow_id}` docs"),
            &mut diagnostics,
        );
        let markdown = join_markdown(&blocks);
        if !markdown.is_empty() {
            view.docs = Some(DocBlock {
                markdown,
                // No natural summary exists for authored flow prose, and
                // inventing one (e.g. the first line) would put words in the
                // author's mouth.
                summary: None,
                // Inert for views — coverage only reads `Entity::docs` — but the
                // author did document this flow.
                documented: true,
            });
        }

        // Narrative order: an example sequence is a story (request, then
        // response), so declaration order is preserved.
        for raw in &flow.examples {
            let raw_path = raw.path.get_ref().trim();
            match files.read_text(raw_path, Some(MAX_EXAMPLE_BYTES)) {
                Ok(content) => view.examples.push(ViewExample {
                    id: raw.id.get_ref().trim().to_string(),
                    title: localized(&raw.title),
                    language: raw.language.clone(),
                    content,
                    description: raw.description.as_ref().map(localized),
                }),
                // Only this example is dropped; the rest of the flow survives.
                Err(error) => diagnostics.push(file_diagnostic(
                    &file.path,
                    &file.source,
                    &raw.path,
                    raw_path,
                    &error,
                    true,
                    &format!("flow `{flow_id}` example `{}`", raw.id.get_ref().trim()),
                )),
            }
        }
    }

    // --- overrides: docs ---------------------------------------------------
    for file in &config.override_files {
        for raw in &file.value.overrides {
            let target = raw.target.get_ref().trim();
            if raw.docs.is_empty() || target.is_empty() {
                continue;
            }
            let Some(entry) = overlay.overrides.get_mut(&EntityId::from_raw(target)) else {
                continue;
            };
            let blocks = read_docs(
                &files,
                &file.path,
                &file.source,
                &raw.docs,
                &format!("override `{target}` docs"),
                &mut diagnostics,
            );
            let markdown = join_markdown(&blocks);
            if markdown.is_empty() {
                continue;
            }
            // A duplicate override may already have contributed docs; append so
            // last-loaded-wins stays additive for prose, as Amendment B requires.
            let combined = match entry.docs.take() {
                Some(existing) => join_markdown(&[existing.markdown, markdown]),
                None => markdown,
            };
            entry.docs = Some(DocBlock {
                markdown: combined,
                // Required by the PRD: a manual block never becomes the summary,
                // and configuration must never claim an item is documented —
                // `cratevista_graph::overlay::append_docs` preserves the
                // discovered `documented` and ignores this field entirely.
                summary: None,
                documented: false,
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::load_from;
    use crate::overlay::build_overlay;
    use crate::validate::validate;

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn write_bytes(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// Loads → validates → builds → embeds, returning the overlay + all
    /// step-4 diagnostics.
    fn embed(dir: &Path) -> (GraphOverlay, Vec<ConfigDiagnostic>) {
        let config = load_from(dir);
        assert!(
            config.diagnostics.is_empty(),
            "load: {:?}",
            config.diagnostics
        );
        let validation = validate(&config);
        let mut outcome = build_overlay(&config, &validation);
        let diagnostics = embed_files(dir, &config, &validation, &mut outcome.overlay);
        (outcome.overlay, diagnostics)
    }

    fn codes(diagnostics: &[ConfigDiagnostic]) -> Vec<&str> {
        diagnostics.iter().map(|d| d.code).collect()
    }

    #[test]
    fn a_flows_docs_and_examples_are_embedded_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "docs/flow.md",
            "# Checkout\n\nClients → Gateway.\n",
        );
        write(
            dir.path(),
            "examples/req.http",
            "POST /checkout HTTP/1.1\n\n{\"cart\": 42}",
        );
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            r#"
[[flow]]
id = "checkout"
title = "Checkout"
docs = ["docs/flow.md"]

  [[flow.example]]
  id = "req"
  title = "Request"
  path = "examples/req.http"
  language = "http"
  description = "What the client sends."
"#,
        );

        let (overlay, diagnostics) = embed(dir.path());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        let view = &overlay.views[0];
        let docs = view.docs.as_ref().expect("docs embedded");
        assert_eq!(docs.markdown, "# Checkout\n\nClients → Gateway.");
        assert_eq!(docs.summary, None);

        assert_eq!(view.examples.len(), 1);
        let example = &view.examples[0];
        assert_eq!(example.id, "req");
        assert_eq!(example.title.default, "Request");
        assert_eq!(example.language.as_deref(), Some("http"));
        assert_eq!(
            example.description.as_ref().unwrap().default,
            "What the client sends."
        );
        // Byte-for-byte after UTF-8 decoding: nothing is reformatted.
        assert_eq!(example.content, "POST /checkout HTTP/1.1\n\n{\"cart\": 42}");
    }

    #[test]
    fn multiple_docs_join_in_declaration_order_with_one_blank_line() {
        let dir = tempfile::tempdir().unwrap();
        // Trailing/leading newlines at the junction differ deliberately.
        write(dir.path(), "docs/one.md", "# One\n\nFirst.\n\n\n");
        write(dir.path(), "docs/two.md", "\n\n## Two\n\n    indented\n");
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]\nid = \"f\"\ntitle = \"F\"\ndocs = [\"docs/one.md\", \"docs/two.md\"]\n",
        );

        let (overlay, diagnostics) = embed(dir.path());
        assert!(diagnostics.is_empty());
        // Declaration order, exactly one blank line at the junction, and the
        // indentation inside the second block survives.
        assert_eq!(
            overlay.views[0].docs.as_ref().unwrap().markdown,
            "# One\n\nFirst.\n\n## Two\n\n    indented"
        );
    }

    #[test]
    fn a_missing_doc_is_diagnosed_and_the_rest_still_embeds() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "docs/real.md", "Real content.");
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]\nid = \"f\"\ntitle = \"F\"\ndocs = [\"docs/gone.md\", \"docs/real.md\"]\n",
        );

        let (overlay, diagnostics) = embed(dir.path());
        assert_eq!(codes(&diagnostics), [code::MISSING_FILE]);
        assert!(diagnostics[0].message.contains("docs/gone.md"));
        assert!(diagnostics[0].position.is_some());
        // Partial success: only the bad reference is dropped.
        assert_eq!(
            overlay.views[0].docs.as_ref().unwrap().markdown,
            "Real content."
        );
    }

    #[test]
    fn a_missing_example_drops_only_itself() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "examples/ok.json", "{}");
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            r#"
[[flow]]
id = "f"
title = "F"

  [[flow.example]]
  id = "gone"
  title = "Gone"
  path = "examples/gone.json"

  [[flow.example]]
  id = "ok"
  title = "Ok"
  path = "examples/ok.json"
"#,
        );

        let (overlay, diagnostics) = embed(dir.path());
        assert_eq!(codes(&diagnostics), [code::MISSING_FILE]);
        assert_eq!(overlay.views[0].examples.len(), 1);
        assert_eq!(overlay.views[0].examples[0].id, "ok");
    }

    #[test]
    fn examples_preserve_narrative_order() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a", "b", "c"] {
            write(dir.path(), &format!("examples/{name}.txt"), name);
        }
        // Declared c, a, b — the author's sequence, not alphabetical.
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            r#"
[[flow]]
id = "f"
title = "F"

  [[flow.example]]
  id = "c"
  title = "C"
  path = "examples/c.txt"

  [[flow.example]]
  id = "a"
  title = "A"
  path = "examples/a.txt"

  [[flow.example]]
  id = "b"
  title = "B"
  path = "examples/b.txt"
"#,
        );
        let (overlay, _) = embed(dir.path());
        let ids: Vec<&str> = overlay.views[0]
            .examples
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(ids, ["c", "a", "b"], "narrative order, not sorted");
    }

    #[test]
    fn a_traversing_path_is_refused_without_touching_the_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        // A real file just outside the workspace.
        let outside = dir
            .path()
            .parent()
            .unwrap()
            .join("cratevista-escape-probe.md");
        std::fs::write(&outside, "secret").unwrap();

        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]\nid = \"f\"\ntitle = \"F\"\ndocs = [\"../cratevista-escape-probe.md\"]\n",
        );
        let (overlay, diagnostics) = embed(dir.path());

        assert_eq!(codes(&diagnostics), [code::INVALID_FILE_PATH]);
        assert_eq!(overlay.views[0].docs, None);
        let _ = std::fs::remove_file(outside);
    }

    #[test]
    fn an_absolute_path_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "docs/real.md", "content");
        let absolute = dir.path().join("docs").join("real.md");
        let absolute = absolute.to_string_lossy().replace('\\', "/");
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            &format!("[[flow]]\nid = \"f\"\ntitle = \"F\"\ndocs = [\"{absolute}\"]\n"),
        );

        let (overlay, diagnostics) = embed(dir.path());
        // Even though the file exists and is inside the workspace, an absolute
        // spelling is not a repo-relative path.
        assert_eq!(codes(&diagnostics), [code::INVALID_FILE_PATH]);
        assert_eq!(overlay.views[0].docs, None);
    }

    #[test]
    fn a_directory_is_not_content() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("docs/sub")).unwrap();
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]\nid = \"f\"\ntitle = \"F\"\ndocs = [\"docs/sub\"]\n",
        );
        let (_, diagnostics) = embed(dir.path());
        assert_eq!(codes(&diagnostics), [code::NOT_A_FILE]);
    }

    #[test]
    fn non_utf8_content_is_refused_rather_than_lossily_decoded() {
        let dir = tempfile::tempdir().unwrap();
        // Invalid UTF-8: a lone continuation byte.
        write_bytes(dir.path(), "examples/bin.dat", &[0x68, 0x69, 0xff, 0xfe]);
        write_bytes(dir.path(), "docs/bin.md", &[0xff, 0xfe]);
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            r#"
[[flow]]
id = "f"
title = "F"
docs = ["docs/bin.md"]

  [[flow.example]]
  id = "e"
  title = "E"
  path = "examples/bin.dat"
"#,
        );
        let (overlay, diagnostics) = embed(dir.path());
        // Distinct codes: an example and a doc fail differently for a reader.
        assert_eq!(
            codes(&diagnostics),
            [code::NOT_UTF8, code::EXAMPLE_NOT_UTF8]
        );
        assert_eq!(overlay.views[0].docs, None);
        assert!(overlay.views[0].examples.is_empty());
    }

    fn flow_with_example(path: &str) -> String {
        format!(
            "[[flow]]\nid = \"f\"\ntitle = \"F\"\n\n  [[flow.example]]\n  id = \"e\"\n  title = \"E\"\n  path = \"{path}\"\n"
        )
    }

    #[test]
    fn an_example_exactly_at_the_limit_is_embedded() {
        let dir = tempfile::tempdir().unwrap();
        let content = "x".repeat(MAX_EXAMPLE_BYTES);
        write(dir.path(), "examples/big.txt", &content);
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            &flow_with_example("examples/big.txt"),
        );

        let (overlay, diagnostics) = embed(dir.path());
        assert!(diagnostics.is_empty(), "exactly at the limit is allowed");
        assert_eq!(
            overlay.views[0].examples[0].content.len(),
            MAX_EXAMPLE_BYTES
        );
    }

    #[test]
    fn an_example_one_byte_over_the_limit_is_dropped_whole_not_truncated() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "examples/big.txt",
            &"x".repeat(MAX_EXAMPLE_BYTES + 1),
        );
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            &flow_with_example("examples/big.txt"),
        );

        let (overlay, diagnostics) = embed(dir.path());
        assert_eq!(codes(&diagnostics), [code::EXAMPLE_TOO_LARGE]);
        // The real size is in the message so the author can act on it.
        assert!(
            diagnostics[0]
                .message
                .contains(&(MAX_EXAMPLE_BYTES + 1).to_string())
        );
        assert!(diagnostics[0].message.contains("rather than truncated"));
        // Dropped whole: no partial example survives.
        assert!(overlay.views[0].examples.is_empty());
    }

    #[test]
    fn docs_are_not_subject_to_the_example_limit() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "docs/big.md",
            &"y".repeat(MAX_EXAMPLE_BYTES + 10),
        );
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]\nid = \"f\"\ntitle = \"F\"\ndocs = [\"docs/big.md\"]\n",
        );
        let (overlay, diagnostics) = embed(dir.path());
        assert!(
            diagnostics.is_empty(),
            "the cap is per-example, not per-doc"
        );
        assert_eq!(
            overlay.views[0].docs.as_ref().unwrap().markdown.len(),
            MAX_EXAMPLE_BYTES + 10
        );
    }

    #[test]
    fn override_docs_map_with_no_summary_and_documented_false() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "docs/widget.md", "Extra prose about Widget.");
        write(
            dir.path(),
            ".cratevista/overrides/o.toml",
            "[[override]]\ntarget = \"item:struct:app::Widget\"\ndocs = [\"docs/widget.md\"]\n",
        );
        let (overlay, diagnostics) = embed(dir.path());
        assert!(diagnostics.is_empty());

        let entry = &overlay.overrides[&EntityId::from_raw("item:struct:app::Widget")];
        let docs = entry.docs.as_ref().expect("override docs embedded");
        assert_eq!(docs.markdown, "Extra prose about Widget.");
        assert_eq!(
            docs.summary, None,
            "a manual block never becomes the summary"
        );
        assert!(
            !docs.documented,
            "configuration must never claim an item is documented"
        );
    }

    #[test]
    fn an_override_doc_failure_leaves_the_override_intact() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".cratevista/overrides/o.toml",
            "[[override]]\ntarget = \"item:struct:app::Widget\"\nlabel = \"W\"\ndocs = [\"docs/gone.md\"]\n",
        );
        let (overlay, diagnostics) = embed(dir.path());
        assert_eq!(codes(&diagnostics), [code::MISSING_FILE]);

        // The override still applies its other fields.
        let entry = &overlay.overrides[&EntityId::from_raw("item:struct:app::Widget")];
        assert_eq!(entry.label.as_ref().unwrap().default, "W");
        assert_eq!(entry.docs, None);
    }

    #[test]
    fn no_diagnostic_contains_an_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]\nid = \"f\"\ntitle = \"F\"\ndocs = [\"docs/gone.md\"]\n",
        );
        let (_, diagnostics) = embed(dir.path());
        let root = dir.path().to_string_lossy().to_string();
        assert!(!diagnostics.is_empty());
        for diagnostic in &diagnostics {
            assert!(
                !diagnostic.message.contains(&root) && !diagnostic.file.contains(&root),
                "leaked an absolute path: {diagnostic}"
            );
        }
    }

    #[test]
    fn embedding_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "docs/one.md", "One.");
        write(dir.path(), "docs/two.md", "Two.");
        write(dir.path(), "examples/a.txt", "A");
        write(dir.path(), "examples/b.txt", "B");
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            r#"
[[flow]]
id = "f"
title = "F"
docs = ["docs/one.md", "docs/two.md"]

  [[flow.example]]
  id = "b"
  title = "B"
  path = "examples/b.txt"

  [[flow.example]]
  id = "a"
  title = "A"
  path = "examples/a.txt"
"#,
        );

        let (first, _) = embed(dir.path());
        let baseline = format!("{:?}", first.views);
        for _ in 0..5 {
            let (again, _) = embed(dir.path());
            assert_eq!(format!("{:?}", again.views), baseline);
        }
    }

    #[test]
    fn a_flow_with_no_docs_or_examples_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]\nid = \"f\"\ntitle = \"F\"\n",
        );
        let (overlay, diagnostics) = embed(dir.path());
        assert!(diagnostics.is_empty());
        assert_eq!(overlay.views[0].docs, None);
        assert!(overlay.views[0].examples.is_empty());
    }

    // --- containment ------------------------------------------------------

    /// The symlink guard's core predicate, tested directly so it has coverage on
    /// every platform — including Windows, where creating a symlink needs a
    /// privilege CI and dev machines often lack.
    #[test]
    fn containment_compares_canonical_paths() {
        let dir = tempfile::tempdir().unwrap();
        let inside = dir.path().join("inside.txt");
        std::fs::write(&inside, "x").unwrap();
        let files = WorkspaceFiles::new(dir.path());

        assert!(files.contains(&inside.canonicalize().unwrap()));
        // The parent is emphatically not inside the workspace.
        assert!(!files.contains(&dir.path().parent().unwrap().canonicalize().unwrap()));
    }

    #[test]
    fn a_workspace_root_that_cannot_be_canonicalized_fails_closed() {
        let files = WorkspaceFiles::new(Path::new("definitely/not/a/real/root"));
        // No canonical root ⇒ containment is unprovable ⇒ refuse, rather than
        // assume the file is safe.
        assert!(!files.contains(Path::new("anything")));
    }

    /// A symlink inside the workspace pointing out of it must not smuggle
    /// content in. `RepoRelativePath` cannot see this: the path text is clean.
    ///
    /// `#[cfg(unix)]` with a hard `expect`, rather than "attempt and skip":
    /// on the platform CI runs, symlink creation MUST succeed, so this can never
    /// quietly become a no-op. Windows lacks the privilege by default, and
    /// `containment_compares_canonical_paths` covers the predicate there.
    #[cfg(unix)]
    #[test]
    fn a_symlink_escaping_the_workspace_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir
            .path()
            .parent()
            .unwrap()
            .join("cratevista-symlink-probe.md");
        std::fs::write(&outside, "secret from outside the workspace").unwrap();

        let link = dir.path().join("docs").join("link.md");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&outside, &link).expect("unix must support symlinks");

        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]
id = \"f\"
title = \"F\"
docs = [\"docs/link.md\"]
",
        );
        let (overlay, diagnostics) = embed(dir.path());

        assert_eq!(codes(&diagnostics), [code::PATH_ESCAPES_WORKSPACE]);
        assert_eq!(overlay.views[0].docs, None, "no smuggled content");
        let _ = std::fs::remove_file(&outside);
    }

    /// An internal symlink is legitimate and must still resolve.
    #[cfg(unix)]
    #[test]
    fn a_symlink_staying_inside_the_workspace_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "docs/real.md", "inside content");
        std::os::unix::fs::symlink(
            dir.path().join("docs").join("real.md"),
            dir.path().join("docs").join("alias.md"),
        )
        .expect("unix must support symlinks");

        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]
id = \"f\"
title = \"F\"
docs = [\"docs/alias.md\"]
",
        );
        let (overlay, diagnostics) = embed(dir.path());
        assert!(diagnostics.is_empty(), "an internal symlink is fine");
        assert_eq!(
            overlay.views[0].docs.as_ref().unwrap().markdown,
            "inside content"
        );
    }

    #[test]
    fn the_markdown_join_matches_the_graphs_rule() {
        // Same contract as `cratevista_graph::overlay::join_markdown`: only the
        // junction is normalized.
        assert_eq!(join_markdown(&["A".into(), "B".into()]), "A\n\nB");
        assert_eq!(join_markdown(&["A\n\n\n".into(), "\n\nB".into()]), "A\n\nB");
        assert_eq!(join_markdown(&["A\r\n".into(), "\r\nB".into()]), "A\n\nB");
        // Interior content is untouched.
        assert_eq!(
            join_markdown(&["A\n\n  indented  ".into(), "B".into()]),
            "A\n\n  indented  \n\nB"
        );
        // Empty blocks vanish rather than leaving stray blank lines.
        assert_eq!(join_markdown(&["".into(), "B".into()]), "B");
        assert_eq!(join_markdown(&["A".into(), "\n\n".into()]), "A");
        assert_eq!(join_markdown(&[]), "");
    }
}
