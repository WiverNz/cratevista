//! Canonical serialization and the prepare-then-commit write of the three
//! artifacts.
//!
//! **Prepare:** serialize `document.json` and `diagnostics.json`; compute the
//! BLAKE3 digests of those exact canonical bytes and embed them in
//! [`GenerationReport::artifact_hashes`]; serialize `generation.json`; write all
//! three to completed temporary sibling files. Any serialization or write error
//! aborts before the commit phase, replacing nothing. **Commit:** replace
//! `document.json` then `diagnostics.json` by same-directory rename, and
//! `generation.json` **last** as the completion marker. Each rename is atomic
//! where the OS supports it; the three-file set is **not** one crash-atomic
//! transaction across all operating systems, so the embedded `artifact_hashes`
//! (not marker/byte equality alone) are the reader's integrity mechanism
//! (see `cratevista-server`). No `last-failure.json` is written.

use std::path::{Path, PathBuf};

use cratevista_schema::canonical::to_canonical_string;
use cratevista_schema::{ArtifactHashes, DiagnosticsReport, ExplorerDocument, GenerationReport};

/// The three artifact file names.
pub const DOCUMENT_FILE: &str = "document.json";
pub const DIAGNOSTICS_FILE: &str = "diagnostics.json";
pub const GENERATION_FILE: &str = "generation.json";

/// A failure preparing or committing artifacts.
#[derive(Debug)]
pub enum ArtifactError {
    /// A value could not be canonically serialized (commits nothing).
    Serialize(String),
    /// A filesystem error preparing temp files (commits nothing).
    Prepare(String),
    /// A rename failed during commit (best-effort cleanup performed).
    Commit(String),
}

impl std::fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactError::Serialize(m) => write!(f, "could not serialize an artifact: {m}"),
            ArtifactError::Prepare(m) => write!(f, "could not prepare artifacts: {m}"),
            ArtifactError::Commit(m) => write!(f, "could not commit artifacts: {m}"),
        }
    }
}

/// Computes the lowercase-hex BLAKE3 digest (64 ASCII chars) of `bytes`.
///
/// This is the exact encoding embedded in [`ArtifactHashes`] and verified by the
/// server: lowercase hexadecimal, exactly 64 characters, no prefix, no
/// whitespace.
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Serializes and commits the three artifacts into `output_dir`.
///
/// The `generation` report is taken by value because this function **populates
/// its [`GenerationReport::artifact_hashes`]** with the BLAKE3 digests of the
/// exact canonical `document.json` / `diagnostics.json` bytes it is about to
/// write (any prior value is overwritten). The bytes hashed are the same bytes
/// committed to disk. Returns the committed paths in commit order
/// (`generation.json` last).
pub fn commit_artifacts(
    output_dir: &Path,
    document: &ExplorerDocument,
    diagnostics: &DiagnosticsReport,
    mut generation: GenerationReport,
) -> Result<Vec<PathBuf>, ArtifactError> {
    // --- prepare: serialize document + diagnostics first (a serialize error
    // commits nothing), then hash those exact bytes and embed the digests in the
    // generation report before serializing it last.
    let document =
        to_canonical_string(document).map_err(|e| ArtifactError::Serialize(e.to_string()))?;
    let diagnostics =
        to_canonical_string(diagnostics).map_err(|e| ArtifactError::Serialize(e.to_string()))?;
    generation.artifact_hashes = Some(ArtifactHashes {
        document_blake3: blake3_hex(document.as_bytes()),
        diagnostics_blake3: blake3_hex(diagnostics.as_bytes()),
    });
    let generation =
        to_canonical_string(&generation).map_err(|e| ArtifactError::Serialize(e.to_string()))?;

    std::fs::create_dir_all(output_dir)
        .map_err(|e| ArtifactError::Prepare(format!("{}: {e}", output_dir.display())))?;

    let doc_tmp = output_dir.join(format!("{DOCUMENT_FILE}.tmp"));
    let diag_tmp = output_dir.join(format!("{DIAGNOSTICS_FILE}.tmp"));
    let gen_tmp = output_dir.join(format!("{GENERATION_FILE}.tmp"));
    let temps = [doc_tmp.clone(), diag_tmp.clone(), gen_tmp.clone()];

    // Write all three temp files; any failure cleans up and replaces nothing.
    for (path, contents) in [
        (&doc_tmp, &document),
        (&diag_tmp, &diagnostics),
        (&gen_tmp, &generation),
    ] {
        if let Err(error) = std::fs::write(path, contents) {
            cleanup(&temps);
            return Err(ArtifactError::Prepare(format!(
                "{}: {error}",
                path.display()
            )));
        }
    }

    // --- commit: document, diagnostics, then generation (last = completion marker).
    let doc = output_dir.join(DOCUMENT_FILE);
    let diag = output_dir.join(DIAGNOSTICS_FILE);
    let generation_path = output_dir.join(GENERATION_FILE);

    if let Err(error) = std::fs::rename(&doc_tmp, &doc) {
        cleanup(&[diag_tmp, gen_tmp]);
        return Err(ArtifactError::Commit(format!("{}: {error}", doc.display())));
    }
    if let Err(error) = std::fs::rename(&diag_tmp, &diag) {
        cleanup(&[gen_tmp]);
        return Err(ArtifactError::Commit(format!(
            "{}: {error}",
            diag.display()
        )));
    }
    if let Err(error) = std::fs::rename(&gen_tmp, &generation_path) {
        return Err(ArtifactError::Commit(format!(
            "{}: {error}",
            generation_path.display()
        )));
    }

    Ok(vec![doc, diag, generation_path])
}

/// Best-effort removal of temp files, ignoring errors.
fn cleanup(paths: &[PathBuf]) {
    for path in paths {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cratevista_schema::{
        Counts, DiagnosticsReport, ExplorerDocument, Generator, Project, Timestamp,
    };

    fn sample() -> (ExplorerDocument, GenerationReport, DiagnosticsReport) {
        let project = Project {
            id: "workspace".into(),
            name: "ws".into(),
            description: String::new(),
            root: None,
            repository_url: None,
            default_branch: None,
        };
        let document = ExplorerDocument::new(project, vec![], vec![], vec![]);
        let generation = GenerationReport {
            generator: Generator {
                name: "cargo-cratevista".into(),
                version: "0.1.0".into(),
            },
            generated_at: Timestamp::new("2026-07-14T00:00:00Z"),
            toolchain: None,
            rustdoc_format_version: None,
            input_hashes: Default::default(),
            counts: Counts {
                entities: 0,
                relations: 0,
                views: 0,
                diagnostics: 0,
            },
            durations_ms: Default::default(),
            artifact_hashes: None,
            partial: false,
        };
        let diagnostics = DiagnosticsReport::new(vec![]);
        (document, generation, diagnostics)
    }

    #[test]
    fn commit_writes_all_three_and_leaves_no_temps() {
        let dir = tempfile::tempdir().unwrap();
        let (document, generation, diagnostics) = sample();
        let order = commit_artifacts(dir.path(), &document, &diagnostics, generation).unwrap();

        assert!(dir.path().join(DOCUMENT_FILE).is_file());
        assert!(dir.path().join(DIAGNOSTICS_FILE).is_file());
        assert!(dir.path().join(GENERATION_FILE).is_file());
        // generation.json is committed last.
        assert!(order.last().unwrap().ends_with(GENERATION_FILE));
        // No temp files remain.
        for name in [DOCUMENT_FILE, DIAGNOSTICS_FILE, GENERATION_FILE] {
            assert!(!dir.path().join(format!("{name}.tmp")).exists());
        }
    }

    #[test]
    fn commit_replaces_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(DOCUMENT_FILE), "OLD").unwrap();
        std::fs::write(dir.path().join(GENERATION_FILE), "OLD").unwrap();
        let (document, generation, diagnostics) = sample();
        commit_artifacts(dir.path(), &document, &diagnostics, generation).unwrap();
        let doc = std::fs::read_to_string(dir.path().join(DOCUMENT_FILE)).unwrap();
        assert_ne!(doc, "OLD");
        assert!(doc.contains("schema_version"));
    }

    #[test]
    fn prepare_failure_preserves_existing_files() {
        // `output_dir` is a *file*, not a directory → create_dir_all fails during
        // prepare, so nothing is committed and the existing sibling is untouched.
        let dir = tempfile::tempdir().unwrap();
        let not_a_dir = dir.path().join("output");
        std::fs::write(&not_a_dir, "existing").unwrap();
        let (document, generation, diagnostics) = sample();
        let error = commit_artifacts(&not_a_dir, &document, &diagnostics, generation).unwrap_err();
        assert!(matches!(error, ArtifactError::Prepare(_)));
        // The pre-existing file is untouched.
        assert_eq!(std::fs::read_to_string(&not_a_dir).unwrap(), "existing");
    }

    #[test]
    fn embeds_blake3_of_exact_committed_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let (document, generation, diagnostics) = sample();
        commit_artifacts(dir.path(), &document, &diagnostics, generation).unwrap();

        let doc_bytes = std::fs::read(dir.path().join(DOCUMENT_FILE)).unwrap();
        let diag_bytes = std::fs::read(dir.path().join(DIAGNOSTICS_FILE)).unwrap();
        let gen_text = std::fs::read_to_string(dir.path().join(GENERATION_FILE)).unwrap();
        let report: GenerationReport = serde_json::from_str(&gen_text).unwrap();
        let hashes = report.artifact_hashes.expect("hashes are always populated");

        assert_eq!(hashes.document_blake3, blake3_hex(&doc_bytes));
        assert_eq!(hashes.diagnostics_blake3, blake3_hex(&diag_bytes));
        // 64 lowercase-hex chars, no self-hash of generation.json.
        for digest in [&hashes.document_blake3, &hashes.diagnostics_blake3] {
            assert_eq!(digest.len(), 64);
            assert!(
                digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            );
        }
        assert!(!gen_text.contains(&blake3_hex(gen_text.as_bytes())));
    }

    #[test]
    fn one_byte_change_changes_the_digest() {
        let a = blake3_hex(b"document-a");
        let b = blake3_hex(b"document-b");
        assert_ne!(a, b);
        assert_eq!(a, blake3_hex(b"document-a"));
    }
}
