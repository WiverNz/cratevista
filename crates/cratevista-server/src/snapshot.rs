//! Marker- and hash-verified loading of a consistent three-file artifact
//! snapshot.
//!
//! The three artifacts are committed by per-file rename with `generation.json`
//! written **last**, and are **not** one crash-atomic transaction. Comparing the
//! `generation.json` bytes before and after reading the other two files (marker
//! A == B) only detects a commit that landed *mid-read* — it does **not** prove
//! the `document.json` / `diagnostics.json` belong to that generation (a torn
//! commit can leave an old `generation.json` observable both before and after
//! the newer siblings are renamed). Integrity is therefore proven by the
//! **BLAKE3 `artifact_hashes`** embedded in `generation.json`: the loader hashes
//! the exact bytes it read and requires them to match.

use std::sync::Arc;

use cratevista_schema::{DiagnosticsReport, ExplorerDocument, GenerationReport};

use crate::error::SnapshotError;
use crate::options::{ArtifactPaths, SnapshotLoadOptions};

/// A cheap discriminator for a committed generation: the exact `generation.json`
/// bytes, plus a header-safe [`token`](SnapshotMarker::token) derived from them.
///
/// Used only to detect that a commit landed **during** a read (so the loader
/// retries) and to let PRD 09 notice a *new* generation. It does **not** prove
/// the other two artifacts belong to this generation — the embedded
/// `artifact_hashes` do that.
#[derive(Debug, Clone)]
pub struct SnapshotMarker {
    bytes: Arc<[u8]>,
    /// Precomputed at construction — see [`SnapshotMarker::token`].
    token: Arc<str>,
}

impl SnapshotMarker {
    /// Builds a marker from `generation.json` bytes, hashing the token **once**.
    fn new(bytes: Arc<[u8]>) -> Self {
        let token = blake3::hash(&bytes).to_hex().to_string().into();
        SnapshotMarker { bytes, token }
    }

    /// The marker bytes (the `generation.json` content).
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether this marker equals the given `generation.json` bytes.
    pub fn matches(&self, generation_bytes: &[u8]) -> bool {
        self.bytes.as_ref() == generation_bytes
    }

    /// A header-safe, opaque name for this generation: lowercase-hex BLAKE3 of
    /// the exact `generation.json` bytes — always **64 characters**, ASCII-only,
    /// and deterministic (equal bytes always yield an equal token).
    ///
    /// This exists because [`as_bytes`](SnapshotMarker::as_bytes) is the raw
    /// artifact content: arbitrary bytes cannot go in an HTTP header. The hash is
    /// computed **once, when the marker is constructed**, and returned by
    /// reference — `/api/document`, `/api/generation` and `/api/diagnostics` all
    /// serve it on **every** request, so re-hashing per request would rehash the
    /// whole report on every artifact fetch.
    ///
    /// It leaks nothing: it is a hash of bytes the server already serves in full
    /// at `/api/generation`.
    ///
    /// Equal tokens mean the same generation, which is what lets a client fetching
    /// the three artifacts separately (three requests, one live swap between them
    /// under PRD-09 watch mode) detect that it assembled a mixed set.
    pub fn token(&self) -> &str {
        &self.token
    }
}

/// A fully validated, integrity-verified snapshot of the three artifacts, held
/// and swapped as **one unit** so a request never mixes generations.
#[derive(Debug, Clone)]
pub struct ArtifactSnapshot {
    /// The parsed, validated document.
    pub document: Arc<ExplorerDocument>,
    /// The parsed generation report.
    pub generation: Arc<GenerationReport>,
    /// The parsed diagnostics report.
    pub diagnostics: Arc<DiagnosticsReport>,
    /// The exact `document.json` bytes (reused verbatim as the API response body).
    pub document_bytes: Arc<[u8]>,
    /// The exact `generation.json` bytes.
    pub generation_bytes: Arc<[u8]>,
    /// The exact `diagnostics.json` bytes.
    pub diagnostics_bytes: Arc<[u8]>,
    /// The completion marker (the `generation.json` bytes).
    pub marker: SnapshotMarker,
    /// `generation.partial` (surfaced by `/api/health`).
    pub partial: bool,
}

/// Loads a consistent, integrity-verified snapshot with bounded retry.
///
/// Marker or hash mismatches are retried within `options`; a persistent marker
/// change returns [`SnapshotError::ArtifactChangedDuringRead`] and a persistent
/// hash mismatch returns [`SnapshotError::SnapshotHashMismatch`]. Missing files,
/// malformed JSON, a missing/invalid `artifact_hashes`, and schema-version
/// problems are **not** transient and fail immediately. No candidate snapshot is
/// ever published on a failed check.
pub fn load_snapshot(
    paths: &ArtifactPaths,
    options: &SnapshotLoadOptions,
) -> Result<ArtifactSnapshot, SnapshotError> {
    load_from(&FsReader { paths }, options)
}

/// The four raw byte reads that make up one load attempt.
struct RawRead {
    marker_a: Vec<u8>,
    document: Vec<u8>,
    diagnostics: Vec<u8>,
    marker_b: Vec<u8>,
}

/// Reads one attempt's worth of raw bytes. Separated from verification so the
/// retry/verification logic is testable without real files.
trait RawReader {
    fn read(&self) -> Result<RawRead, SnapshotError>;
}

/// Reads the four byte sequences from disk in order: `generation.json` (marker
/// A), `document.json`, `diagnostics.json`, `generation.json` (marker B).
struct FsReader<'a> {
    paths: &'a ArtifactPaths,
}

impl RawReader for FsReader<'_> {
    fn read(&self) -> Result<RawRead, SnapshotError> {
        let marker_a = read_file(&self.paths.generation)?;
        let document = read_file(&self.paths.document)?;
        let diagnostics = read_file(&self.paths.diagnostics)?;
        let marker_b = read_file(&self.paths.generation)?;
        Ok(RawRead {
            marker_a,
            document,
            diagnostics,
            marker_b,
        })
    }
}

/// Reads a file, mapping a missing file to [`SnapshotError::ArtifactsMissing`]
/// and any other error to [`SnapshotError::ArtifactReadFailed`]. Never includes
/// the path in the error.
fn read_file(path: &std::path::Path) -> Result<Vec<u8>, SnapshotError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(SnapshotError::ArtifactsMissing)
        }
        Err(error) => Err(SnapshotError::ArtifactReadFailed(error.kind().to_string())),
    }
}

/// The outcome of verifying one attempt.
enum Attempt {
    /// A fully verified snapshot.
    Loaded(Box<ArtifactSnapshot>),
    /// A transient inconsistency worth retrying (carries the error to surface if
    /// retries are exhausted).
    Retry(SnapshotError),
}

fn load_from<R: RawReader>(
    reader: &R,
    options: &SnapshotLoadOptions,
) -> Result<ArtifactSnapshot, SnapshotError> {
    let mut last_transient: Option<SnapshotError> = None;
    for attempt in 0..=options.max_retries {
        if attempt > 0 && !options.retry_delay.is_zero() {
            std::thread::sleep(options.retry_delay);
        }
        let raw = reader.read()?;
        match verify(raw, options)? {
            Attempt::Loaded(snapshot) => return Ok(*snapshot),
            Attempt::Retry(reason) => last_transient = Some(reason),
        }
    }
    Err(last_transient.unwrap_or_else(|| {
        SnapshotError::InternalInvariant("retry loop yielded no outcome".into())
    }))
}

/// Verifies one raw read. `Err` is a fatal (non-retryable) failure; `Ok(Retry)`
/// is a transient inconsistency.
fn verify(raw: RawRead, options: &SnapshotLoadOptions) -> Result<Attempt, SnapshotError> {
    // 5. A == B: detects a commit landing mid-read (retry), nothing more.
    if raw.marker_a != raw.marker_b {
        return Ok(Attempt::Retry(SnapshotError::ArtifactChangedDuringRead));
    }

    // 6. Parse the generation report from the marker bytes.
    let generation: GenerationReport = serde_json::from_slice(&raw.marker_a)
        .map_err(|error| SnapshotError::MalformedGeneration(error.to_string()))?;

    // 7. Require artifact_hashes (absent → a pre-amendment set).
    let (declared_document, declared_diagnostics) = match &generation.artifact_hashes {
        Some(hashes) => (
            hashes.document_blake3.clone(),
            hashes.diagnostics_blake3.clone(),
        ),
        None => return Err(SnapshotError::SnapshotIntegrityUnavailable),
    };

    // 8. Validate the digest encoding BEFORE comparing (not a retry condition).
    if !is_valid_digest(&declared_document) {
        return Err(SnapshotError::InvalidArtifactHash("document_blake3".into()));
    }
    if !is_valid_digest(&declared_diagnostics) {
        return Err(SnapshotError::InvalidArtifactHash(
            "diagnostics_blake3".into(),
        ));
    }

    // 9 + 10. Hash the loaded bytes and compare; a mismatch is transient.
    let computed_document = blake3::hash(&raw.document).to_hex().to_string();
    let computed_diagnostics = blake3::hash(&raw.diagnostics).to_hex().to_string();
    if computed_document != declared_document || computed_diagnostics != declared_diagnostics {
        return Ok(Attempt::Retry(SnapshotError::SnapshotHashMismatch));
    }

    // 11. Parse the document and diagnostics.
    let document: ExplorerDocument = serde_json::from_slice(&raw.document)
        .map_err(|error| SnapshotError::MalformedDocument(error.to_string()))?;
    let diagnostics: DiagnosticsReport = serde_json::from_slice(&raw.diagnostics)
        .map_err(|error| SnapshotError::MalformedDiagnostics(error.to_string()))?;

    // 12. Validate both versioned artifacts (generation.json has no schema_version).
    let document_version = document.schema_version.as_str().to_string();
    let diagnostics_version = diagnostics.schema_version.as_str().to_string();
    require_supported_major(&document_version, options.supported_major)?;
    require_supported_major(&diagnostics_version, options.supported_major)?;
    if document_version != diagnostics_version {
        return Err(SnapshotError::SchemaVersionMismatch {
            document: document_version,
            diagnostics: diagnostics_version,
        });
    }
    if let Err(errors) = document.validate() {
        return Err(SnapshotError::InvalidDocument(errors.len()));
    }

    // 13. Publish the candidate.
    let partial = generation.partial;
    let marker = SnapshotMarker::new(raw.marker_a.clone().into());
    Ok(Attempt::Loaded(Box::new(ArtifactSnapshot {
        document: Arc::new(document),
        generation: Arc::new(generation),
        diagnostics: Arc::new(diagnostics),
        document_bytes: raw.document.into(),
        generation_bytes: raw.marker_a.into(),
        diagnostics_bytes: raw.diagnostics.into(),
        marker,
        partial,
    })))
}

/// A digest is valid iff it is exactly 64 lowercase-hex ASCII characters (no
/// `0x` prefix, no uppercase, no whitespace).
fn is_valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The `MAJOR` component of a `MAJOR.MINOR` version string, if parseable.
fn major_of(version: &str) -> Option<u32> {
    version.split('.').next()?.parse().ok()
}

fn require_supported_major(version: &str, supported: u32) -> Result<(), SnapshotError> {
    match major_of(version) {
        Some(major) if major == supported => Ok(()),
        _ => Err(SnapshotError::SchemaVersionUnsupported(version.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::time::Duration;

    use cratevista_schema::canonical::to_canonical_string;
    use cratevista_schema::{
        ArtifactHashes, Counts, DiagnosticsReport, EntityId, ExplorerDocument, GenerationReport,
        Generator, Project, Provenance, Relation, RelationKind, SchemaVersion, Timestamp,
    };

    fn hex(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }

    fn options() -> SnapshotLoadOptions {
        SnapshotLoadOptions {
            max_retries: 4,
            retry_delay: Duration::ZERO,
            supported_major: 1,
        }
    }

    fn project() -> Project {
        Project {
            id: "workspace".into(),
            name: "ws".into(),
            description: String::new(),
            root: None,
            repository_url: None,
            default_branch: None,
        }
    }

    fn valid_document_bytes() -> Vec<u8> {
        let doc = ExplorerDocument::new(project(), vec![], vec![], vec![]);
        to_canonical_string(&doc).unwrap().into_bytes()
    }

    fn invalid_document_bytes() -> Vec<u8> {
        // A relation whose endpoints do not exist → validate() fails, but the
        // JSON still parses.
        let rel = Relation::new(
            RelationKind::new("contains"),
            EntityId::from_raw("missing-a"),
            EntityId::from_raw("missing-b"),
            Provenance::Discovered,
        );
        let doc = ExplorerDocument::new(project(), vec![], vec![rel], vec![]);
        to_canonical_string(&doc).unwrap().into_bytes()
    }

    fn diagnostics_bytes() -> Vec<u8> {
        to_canonical_string(&DiagnosticsReport::new(vec![]))
            .unwrap()
            .into_bytes()
    }

    /// Builds `generation.json` bytes carrying the given (possibly bogus) digests.
    fn generation_bytes(document_blake3: &str, diagnostics_blake3: &str) -> Vec<u8> {
        let report = GenerationReport {
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
            artifact_hashes: Some(ArtifactHashes {
                document_blake3: document_blake3.to_string(),
                diagnostics_blake3: diagnostics_blake3.to_string(),
            }),
            partial: false,
        };
        to_canonical_string(&report).unwrap().into_bytes()
    }

    /// `generation.json` without artifact_hashes (a pre-amendment set).
    fn generation_bytes_no_hashes() -> Vec<u8> {
        let report = GenerationReport {
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
        to_canonical_string(&report).unwrap().into_bytes()
    }

    fn consistent_raw(document: Vec<u8>, diagnostics: Vec<u8>) -> RawRead {
        let generation = generation_bytes(&hex(&document), &hex(&diagnostics));
        RawRead {
            marker_a: generation.clone(),
            document,
            diagnostics,
            marker_b: generation,
        }
    }

    /// A reader that replays scripted reads, repeating the last one when exhausted.
    struct ScriptReader {
        scripts: Vec<Result<RawRead, SnapshotError>>,
        index: Cell<usize>,
    }

    impl ScriptReader {
        fn new(scripts: Vec<Result<RawRead, SnapshotError>>) -> Self {
            ScriptReader {
                scripts,
                index: Cell::new(0),
            }
        }
    }

    // RawRead / SnapshotError are not Clone, so ScriptReader yields each script
    // once and then a fresh reconstruction of the final inconsistency via a
    // rebuild closure is unnecessary: tests that need "keep failing" push enough
    // scripts (max_retries + 1).
    impl RawReader for ScriptReader {
        fn read(&self) -> Result<RawRead, SnapshotError> {
            let i = self.index.get();
            self.index.set(i + 1);
            let slot = self
                .scripts
                .get(i)
                .unwrap_or_else(|| self.scripts.last().expect("at least one script"));
            match slot {
                Ok(raw) => Ok(RawRead {
                    marker_a: raw.marker_a.clone(),
                    document: raw.document.clone(),
                    diagnostics: raw.diagnostics.clone(),
                    marker_b: raw.marker_b.clone(),
                }),
                Err(error) => Err(clone_error(error)),
            }
        }
    }

    fn clone_error(error: &SnapshotError) -> SnapshotError {
        // Only the variants used by scripted error tests need reconstruction.
        match error {
            SnapshotError::ArtifactsMissing => SnapshotError::ArtifactsMissing,
            other => {
                SnapshotError::InternalInvariant(format!("unexpected scripted error: {other}"))
            }
        }
    }

    fn load_scripted(
        scripts: Vec<Result<RawRead, SnapshotError>>,
    ) -> Result<ArtifactSnapshot, SnapshotError> {
        load_from(&ScriptReader::new(scripts), &options())
    }

    #[test]
    fn valid_matching_snapshot_loads() {
        let snapshot = load_scripted(vec![Ok(consistent_raw(
            valid_document_bytes(),
            diagnostics_bytes(),
        ))])
        .expect("valid snapshot loads");
        assert!(!snapshot.partial);
        assert!(snapshot.marker.matches(&snapshot.generation_bytes));
    }

    #[test]
    fn stable_marker_and_matching_hashes_succeed_first_try() {
        let reader = ScriptReader::new(vec![Ok(consistent_raw(
            valid_document_bytes(),
            diagnostics_bytes(),
        ))]);
        load_from(&reader, &options()).expect("loads");
        assert_eq!(reader.index.get(), 1, "no retry needed");
    }

    #[test]
    fn marker_changes_once_then_retry_succeeds() {
        let torn = RawRead {
            marker_a: generation_bytes(&hex(b"a"), &hex(b"b")),
            document: valid_document_bytes(),
            diagnostics: diagnostics_bytes(),
            marker_b: generation_bytes(&hex(b"c"), &hex(b"d")), // A != B
        };
        let good = consistent_raw(valid_document_bytes(), diagnostics_bytes());
        load_scripted(vec![Ok(torn), Ok(good)]).expect("second attempt succeeds");
    }

    #[test]
    fn marker_changes_persistently_exhausts_retries() {
        let make = || RawRead {
            marker_a: generation_bytes(&hex(b"a"), &hex(b"b")),
            document: valid_document_bytes(),
            diagnostics: diagnostics_bytes(),
            marker_b: generation_bytes(&hex(b"c"), &hex(b"d")),
        };
        let scripts = (0..6).map(|_| Ok(make())).collect();
        let error = load_scripted(scripts).unwrap_err();
        assert_eq!(error.code(), "artifact_changed_during_read");
    }

    #[test]
    fn torn_commit_new_document_old_generation_is_rejected() {
        // marker A == B (a *stable* old generation.json) whose hashes match the
        // OLD document, but the document bytes are NEW → hash mismatch.
        let old_generation = generation_bytes(&hex(b"old-document"), &hex(&diagnostics_bytes()));
        let raw = RawRead {
            marker_a: old_generation.clone(),
            document: valid_document_bytes(), // new, does not hash to old
            diagnostics: diagnostics_bytes(),
            marker_b: old_generation,
        };
        let error = load_scripted(vec![Ok(raw)]).unwrap_err();
        assert_eq!(error.code(), "snapshot_hash_mismatch");
    }

    #[test]
    fn document_hash_mismatch_is_rejected() {
        let raw = RawRead {
            marker_a: generation_bytes(&hex(b"wrong"), &hex(&diagnostics_bytes())),
            document: valid_document_bytes(),
            diagnostics: diagnostics_bytes(),
            marker_b: generation_bytes(&hex(b"wrong"), &hex(&diagnostics_bytes())),
        };
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "snapshot_hash_mismatch"
        );
    }

    #[test]
    fn diagnostics_hash_mismatch_is_rejected() {
        let doc = valid_document_bytes();
        let raw = RawRead {
            marker_a: generation_bytes(&hex(&doc), &hex(b"wrong")),
            document: doc.clone(),
            diagnostics: diagnostics_bytes(),
            marker_b: generation_bytes(&hex(&doc), &hex(b"wrong")),
        };
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "snapshot_hash_mismatch"
        );
    }

    #[test]
    fn missing_artifact_hashes_is_integrity_unavailable() {
        let generation = generation_bytes_no_hashes();
        let raw = RawRead {
            marker_a: generation.clone(),
            document: valid_document_bytes(),
            diagnostics: diagnostics_bytes(),
            marker_b: generation,
        };
        let error = load_scripted(vec![Ok(raw)]).unwrap_err();
        assert_eq!(error.code(), "snapshot_integrity_unavailable");
        assert!(error.is_environment());
    }

    #[test]
    fn invalid_digest_length_is_rejected() {
        let doc = valid_document_bytes();
        let raw = RawRead {
            marker_a: generation_bytes("abc", &hex(&diagnostics_bytes())),
            document: doc,
            diagnostics: diagnostics_bytes(),
            marker_b: generation_bytes("abc", &hex(&diagnostics_bytes())),
        };
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "invalid_artifact_hash"
        );
    }

    #[test]
    fn uppercase_digest_is_rejected() {
        let doc = valid_document_bytes();
        let upper = hex(&doc).to_uppercase();
        let raw = RawRead {
            marker_a: generation_bytes(&upper, &hex(&diagnostics_bytes())),
            document: doc.clone(),
            diagnostics: diagnostics_bytes(),
            marker_b: generation_bytes(&upper, &hex(&diagnostics_bytes())),
        };
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "invalid_artifact_hash"
        );
    }

    #[test]
    fn zero_x_prefixed_digest_is_rejected() {
        let doc = valid_document_bytes();
        let base = hex(&doc);
        let prefixed = format!("0x{}", &base[2..]); // still 64 chars, contains 'x'
        let raw = RawRead {
            marker_a: generation_bytes(&prefixed, &hex(&diagnostics_bytes())),
            document: doc.clone(),
            diagnostics: diagnostics_bytes(),
            marker_b: generation_bytes(&prefixed, &hex(&diagnostics_bytes())),
        };
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "invalid_artifact_hash"
        );
    }

    #[test]
    fn malformed_generation_is_rejected() {
        let raw = RawRead {
            marker_a: b"{not json".to_vec(),
            document: valid_document_bytes(),
            diagnostics: diagnostics_bytes(),
            marker_b: b"{not json".to_vec(),
        };
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "malformed_generation"
        );
    }

    #[test]
    fn malformed_document_is_rejected() {
        let bad_doc = b"{not json".to_vec();
        let raw = consistent_raw(bad_doc, diagnostics_bytes());
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "malformed_document"
        );
    }

    #[test]
    fn malformed_diagnostics_is_rejected() {
        let bad_diag = b"{not json".to_vec();
        let raw = consistent_raw(valid_document_bytes(), bad_diag);
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "malformed_diagnostics"
        );
    }

    #[test]
    fn invalid_document_is_rejected() {
        let raw = consistent_raw(invalid_document_bytes(), diagnostics_bytes());
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "invalid_document"
        );
    }

    /// Rewrites the `schema_version` of a serialized artifact.
    ///
    /// Anchored on [`SchemaVersion::CURRENT`] rather than a literal: these tests
    /// previously hard-coded `"1.0"`, so the 1.0→1.1 bump turned the rewrite
    /// into a silent no-op and the assertions stopped testing anything. The
    /// `assert!` makes that failure mode loud rather than silent.
    fn with_schema_version(bytes: Vec<u8>, version: &str) -> Vec<u8> {
        let text = String::from_utf8(bytes).unwrap();
        let from = format!("\"schema_version\": \"{}\"", SchemaVersion::CURRENT);
        let to = format!("\"schema_version\": \"{version}\"");
        assert!(
            text.contains(&from),
            "fixture must carry the current schema version so this rewrite applies"
        );
        text.replace(&from, &to).into_bytes()
    }

    #[test]
    fn unsupported_document_major_is_rejected() {
        let doc = with_schema_version(valid_document_bytes(), "2.0");
        let raw = consistent_raw(doc, diagnostics_bytes());
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "schema_version_unsupported"
        );
    }

    #[test]
    fn unsupported_diagnostics_major_is_rejected() {
        let diag = with_schema_version(diagnostics_bytes(), "2.0");
        let raw = consistent_raw(valid_document_bytes(), diag);
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "schema_version_unsupported"
        );
    }

    #[test]
    fn exact_version_mismatch_is_rejected() {
        // Both major 1 (supported) but different exact versions.
        let other_minor = "1.99";
        assert_ne!(other_minor, SchemaVersion::CURRENT);
        let diag = with_schema_version(diagnostics_bytes(), other_minor);
        let raw = consistent_raw(valid_document_bytes(), diag);
        assert_eq!(
            load_scripted(vec![Ok(raw)]).unwrap_err().code(),
            "schema_version_mismatch"
        );
    }

    /// PRD-08 Amendment A: the 1.0 → 1.1 bump is additive, so a snapshot written
    /// by an older generator — document AND diagnostics both at `1.0` — must
    /// still load. Major-version gating is unchanged.
    #[test]
    fn older_matching_minor_version_snapshot_still_loads() {
        let doc = with_schema_version(valid_document_bytes(), "1.0");
        let diag = with_schema_version(diagnostics_bytes(), "1.0");
        let raw = consistent_raw(doc, diag);
        let snapshot = load_scripted(vec![Ok(raw)]).expect("a 1.0 snapshot must still load");
        assert_eq!(snapshot.document.schema_version.as_str(), "1.0");
    }

    #[test]
    fn no_error_message_contains_an_absolute_path() {
        // Craft a variety of errors and assert none leak a Windows/Unix path.
        let errors = vec![
            load_scripted(vec![Ok(consistent_raw(
                b"{bad".to_vec(),
                diagnostics_bytes(),
            ))])
            .unwrap_err(),
            load_scripted(vec![Ok({
                let generation = generation_bytes_no_hashes();
                RawRead {
                    marker_a: generation.clone(),
                    document: valid_document_bytes(),
                    diagnostics: diagnostics_bytes(),
                    marker_b: generation,
                }
            })])
            .unwrap_err(),
        ];
        for error in errors {
            let text = format!("{error} {:?}", error.remediation());
            assert!(!text.contains(":\\"), "leaked windows path: {text}");
            assert!(!text.contains("/home/"), "leaked unix path: {text}");
        }
    }

    // ---- filesystem-backed tests (real load_snapshot) ----

    fn write_snapshot_dir(dir: &std::path::Path) {
        let document = valid_document_bytes();
        let diagnostics = diagnostics_bytes();
        let generation = generation_bytes(&hex(&document), &hex(&diagnostics));
        std::fs::write(dir.join("document.json"), &document).unwrap();
        std::fs::write(dir.join("diagnostics.json"), &diagnostics).unwrap();
        std::fs::write(dir.join("generation.json"), &generation).unwrap();
    }

    #[test]
    fn fs_valid_snapshot_loads() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot_dir(dir.path());
        let paths = ArtifactPaths::in_dir(dir.path());
        load_snapshot(&paths, &options()).expect("loads from disk");
    }

    #[test]
    fn fs_missing_artifacts_reported() {
        let dir = tempfile::tempdir().unwrap();
        // Only document + diagnostics; no generation.json.
        std::fs::write(dir.path().join("document.json"), valid_document_bytes()).unwrap();
        std::fs::write(dir.path().join("diagnostics.json"), diagnostics_bytes()).unwrap();
        let paths = ArtifactPaths::in_dir(dir.path());
        assert_eq!(
            load_snapshot(&paths, &options()).unwrap_err().code(),
            "artifacts_missing"
        );
    }

    #[test]
    fn fs_stale_tmp_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot_dir(dir.path());
        // Stale temp files from an aborted write must be ignored.
        std::fs::write(dir.path().join("document.json.tmp"), b"garbage").unwrap();
        std::fs::write(dir.path().join("generation.json.tmp"), b"garbage").unwrap();
        let paths = ArtifactPaths::in_dir(dir.path());
        load_snapshot(&paths, &options()).expect("committed files load; temps ignored");
    }

    // --- PRD-06 amendment A1: SnapshotMarker::token -----------------------

    #[test]
    fn the_token_is_64_lowercase_hex_and_is_the_blake3_of_the_marker_bytes() {
        let bytes: Arc<[u8]> = Arc::from(&b"{\"generated_at\":\"2026-07-14T00:00:00Z\"}"[..]);
        let marker = SnapshotMarker::new(bytes.clone());

        assert_eq!(marker.token().len(), 64, "fixed length");
        assert!(
            marker
                .token()
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "lowercase hex only, so it is always a legal header value: {}",
            marker.token()
        );
        // Exactly the hash of the raw marker bytes — nothing else mixed in.
        assert_eq!(marker.token(), blake3::hash(&bytes).to_hex().as_str());
    }

    #[test]
    fn the_token_is_deterministic_and_distinguishes_generations() {
        let a = SnapshotMarker::new(Arc::from(&b"generation-one"[..]));
        let same = SnapshotMarker::new(Arc::from(&b"generation-one"[..]));
        let other = SnapshotMarker::new(Arc::from(&b"generation-two"[..]));

        assert_eq!(a.token(), same.token(), "equal bytes → equal token");
        assert_ne!(
            a.token(),
            other.token(),
            "different bytes → different token"
        );
    }

    #[test]
    fn the_token_is_computed_once_at_construction_not_per_call() {
        let marker = SnapshotMarker::new(Arc::from(&b"generation-one"[..]));
        // Same backing allocation every call: the token is stored, not rehashed.
        // A per-request implementation would return a fresh String each time.
        assert!(std::ptr::eq(marker.token(), marker.token()));
        // A clone shares it too, so cloning a snapshot never rehashes.
        assert!(std::ptr::eq(marker.token(), marker.clone().token()));
    }

    #[test]
    fn the_marker_still_detects_a_mid_read_commit() {
        // The token is additive: `matches` keeps working on the raw bytes.
        let marker = SnapshotMarker::new(Arc::from(&b"generation-one"[..]));
        assert!(marker.matches(b"generation-one"));
        assert!(!marker.matches(b"generation-two"));
        assert_eq!(marker.as_bytes(), b"generation-one");
    }
}
