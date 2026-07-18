//! The A/B/C ownership marker and crash-safe marker I/O (PRD 10, Decision 2).
//!
//! Exactly three valid states:
//!
//! | state | `kind` | `output_key` |
//! | --- | --- | --- |
//! | **A** incomplete staging | `staging` | `Some(key)` |
//! | **B** complete, not finalized | `site` | `Some(key)` |
//! | **C** final published site | `site` | `None` (portable) |
//!
//! Any deserializable JSON is **not** valid everywhere: reading validates against
//! an expected context (published C, staging-A-for-key, complete-B-for-key), and a
//! present-but-invalid marker maps to `build_output_marker_invalid`. An **absent**
//! marker is not this error (Phase 2B maps a non-empty unmarked output to
//! `build_output_not_owned`).
//!
//! Writes are crash-safe: serialize → write a uniquely named temp sibling → flush →
//! **rename over** the authoritative file. The authoritative marker is never
//! truncated or rewritten in place.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::BuildError;
use super::nonce::generate_nonce;
use crate::clock::Clock;

/// The authoritative marker file name inside a site / staging / backup directory.
pub const MARKER_FILENAME: &str = ".cratevista-static-site.json";

/// The stable `format` tag.
const FORMAT: &str = "cratevista-static-site";
/// The supported schema version.
const VERSION: u32 = 1;

/// The marker `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkerKind {
    /// An incomplete staging directory.
    Staging,
    /// A complete (unfinalized or finalized) site.
    Site,
}

/// A parsed marker. Construct the three valid states with [`Marker::staging`],
/// [`Marker::complete`], [`Marker::published`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    format: String,
    version: u32,
    kind: MarkerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output_key: Option<String>,
    generated_at: String,
}

/// The role a marker is expected to fill at a location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerRole<'a> {
    /// A final published site (state C): `kind: site`, no `output_key`.
    Published,
    /// An incomplete staging (state A) for this key.
    Staging(&'a str),
    /// A complete-but-unfinalized site (state B) for this key.
    Complete(&'a str),
}

impl Marker {
    /// State **A** — incomplete staging for `output_key`.
    pub fn staging(output_key: &str, clock: &dyn Clock) -> Marker {
        Marker::staging_at(output_key, &clock.now_rfc3339())
    }

    /// State **A** with an explicit `generated_at` (one build stamps every marker
    /// with the same time).
    pub fn staging_at(output_key: &str, generated_at: &str) -> Marker {
        Marker {
            format: FORMAT.to_string(),
            version: VERSION,
            kind: MarkerKind::Staging,
            output_key: Some(output_key.to_string()),
            generated_at: generated_at.to_string(),
        }
    }

    /// State **B** — complete, not yet finalized, for `output_key`.
    pub fn complete(output_key: &str, clock: &dyn Clock) -> Marker {
        Marker::complete_at(output_key, &clock.now_rfc3339())
    }

    /// State **B** with an explicit `generated_at`.
    pub fn complete_at(output_key: &str, generated_at: &str) -> Marker {
        Marker {
            format: FORMAT.to_string(),
            version: VERSION,
            kind: MarkerKind::Site,
            output_key: Some(output_key.to_string()),
            generated_at: generated_at.to_string(),
        }
    }

    /// State **C** — final published site (portable; no `output_key`).
    pub fn published(clock: &dyn Clock) -> Marker {
        Marker::published_at(&clock.now_rfc3339())
    }

    /// State **C** with an explicit `generated_at`.
    pub fn published_at(generated_at: &str) -> Marker {
        Marker {
            format: FORMAT.to_string(),
            version: VERSION,
            kind: MarkerKind::Site,
            output_key: None,
            generated_at: generated_at.to_string(),
        }
    }

    /// The `output_key`, if this marker carries one (states A and B).
    pub fn output_key(&self) -> Option<&str> {
        self.output_key.as_deref()
    }

    /// The `kind`.
    pub fn kind(&self) -> MarkerKind {
        self.kind
    }

    /// Serializes to the canonical marker bytes.
    fn to_bytes(&self) -> Vec<u8> {
        // A plain struct of strings/enums cannot fail to serialize.
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Parses marker bytes, distinguishing malformed JSON, unsupported
    /// format/version, and an invalid `kind`/`output_key` combination. Every
    /// failure maps to `build_output_marker_invalid`.
    pub fn parse(bytes: &[u8]) -> Result<Marker, BuildError> {
        let marker: Marker =
            serde_json::from_slice(bytes).map_err(|_| invalid("it is not valid JSON"))?;
        if marker.format != FORMAT {
            return Err(invalid("its format tag is not recognized"));
        }
        if marker.version != VERSION {
            return Err(invalid("its version is not supported"));
        }
        // A staging marker MUST carry a key; a `staging` kind with no key is an
        // invalid combination.
        if marker.kind == MarkerKind::Staging && marker.output_key.is_none() {
            return Err(invalid("a staging marker is missing its output key"));
        }
        Ok(marker)
    }

    /// Validates that this marker fills `role` (already parsed & structurally
    /// valid). Distinguishes wrong-kind, wrong key-presence, and mismatched key —
    /// all `build_output_marker_invalid`.
    pub fn validate_as(&self, role: MarkerRole<'_>) -> Result<(), BuildError> {
        match role {
            MarkerRole::Published => {
                if self.kind != MarkerKind::Site {
                    return Err(invalid("a published site must be marked as a site"));
                }
                if self.output_key.is_some() {
                    return Err(invalid(
                        "a published site marker must not carry an output key",
                    ));
                }
                Ok(())
            }
            MarkerRole::Staging(expected) => {
                if self.kind != MarkerKind::Staging {
                    return Err(invalid("an incomplete staging must be marked as staging"));
                }
                self.expect_key(expected)
            }
            MarkerRole::Complete(expected) => {
                if self.kind != MarkerKind::Site {
                    return Err(invalid("a complete candidate must be marked as a site"));
                }
                if self.output_key.is_none() {
                    return Err(invalid("a complete candidate must carry its output key"));
                }
                self.expect_key(expected)
            }
        }
    }

    /// Reads and validates the authoritative marker in `dir` as `role`.
    ///
    /// Returns `Ok(Some(marker))` when present and valid, `Ok(None)` when **absent**
    /// (not an error here), and `Err(build_output_marker_invalid)` when present but
    /// invalid.
    pub fn read_as(dir: &Path, role: MarkerRole<'_>) -> Result<Option<Marker>, BuildError> {
        let path = dir.join(MARKER_FILENAME);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => {
                return Err(BuildError::Filesystem {
                    context: "marker-read",
                });
            }
        };
        let marker = Marker::parse(&bytes)?;
        marker.validate_as(role)?;
        Ok(Some(marker))
    }

    fn expect_key(&self, expected: &str) -> Result<(), BuildError> {
        match self.output_key.as_deref() {
            Some(key) if key == expected => Ok(()),
            Some(_) => Err(invalid("its output key belongs to a different output")),
            None => Err(invalid("it is missing its output key")),
        }
    }
}

fn invalid(reason: &'static str) -> BuildError {
    BuildError::OutputMarkerInvalid { reason }
}

// ---------------------------------------------------------------------------
// Crash-safe marker I/O
// ---------------------------------------------------------------------------

/// A minimal, injectable filesystem seam for marker writes, so tests can fail each
/// step (temp creation, write, flush, rename) without a real disk fault.
pub trait MarkerFs {
    /// Creates (or truncates) the **temp** file at `path` for writing.
    fn create(&self, path: &Path) -> std::io::Result<Box<dyn MarkerFile>>;
    /// Renames `from` over `to` (the authoritative marker).
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()>;
    /// Best-effort removal of a leftover temp file. Failures are ignored.
    fn remove(&self, path: &Path);
}

/// An open temp marker file.
pub trait MarkerFile {
    /// Writes all bytes.
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()>;
    /// Flushes to durable storage (best-effort per platform).
    fn sync_all(&mut self) -> std::io::Result<()>;
}

/// The real filesystem implementation.
pub struct RealMarkerFs;

impl MarkerFs for RealMarkerFs {
    fn create(&self, path: &Path) -> std::io::Result<Box<dyn MarkerFile>> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        Ok(Box::new(RealMarkerFile { file }))
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::rename(from, to)
    }
    fn remove(&self, path: &Path) {
        let _ = std::fs::remove_file(path);
    }
}

struct RealMarkerFile {
    file: std::fs::File,
}

impl MarkerFile for RealMarkerFile {
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        self.file.write_all(buf)
    }
    fn sync_all(&mut self) -> std::io::Result<()> {
        self.file.sync_all()
    }
}

/// The crash-safe marker temp path: `.cratevista-static-site.json.tmp-<nonce32>`.
///
/// The 32-hex nonce is the same fixed-width format P0 recognition accepts, so a
/// leftover temp from an interrupted marker write is recognizable (and ignorable)
/// rather than mistaken for unrelated content.
fn temp_marker_path(dir: &Path) -> PathBuf {
    dir.join(format!("{MARKER_FILENAME}.tmp-{}", generate_nonce()))
}

/// Writes `marker` as the authoritative marker in `dir`, crash-safely.
///
/// On **any** failure the authoritative `.cratevista-static-site.json` is left
/// exactly as it was (a failure before the final rename never touches it), and the
/// temp file is best-effort removed so a leftover cannot be mistaken for the
/// authoritative marker (its name ends in `.tmp-*`, and readers only ever open
/// `MARKER_FILENAME`).
pub fn write_marker(fs: &dyn MarkerFs, dir: &Path, marker: &Marker) -> Result<(), BuildError> {
    let bytes = marker.to_bytes();
    let temp = temp_marker_path(dir);
    let authoritative = dir.join(MARKER_FILENAME);

    let mut file = match fs.create(&temp) {
        Ok(file) => file,
        Err(_) => {
            return Err(BuildError::Filesystem {
                context: "marker-temp-create",
            });
        }
    };
    if file.write_all(&bytes).is_err() {
        drop(file);
        fs.remove(&temp);
        return Err(BuildError::Filesystem {
            context: "marker-write",
        });
    }
    if file.sync_all().is_err() {
        drop(file);
        fs.remove(&temp);
        return Err(BuildError::Filesystem {
            context: "marker-flush",
        });
    }
    drop(file); // close before rename (Windows-friendly).
    if fs.rename(&temp, &authoritative).is_err() {
        fs.remove(&temp);
        return Err(BuildError::Filesystem {
            context: "marker-rename",
        });
    }
    Ok(())
}

/// Convenience: write with the real filesystem.
pub fn write_marker_real(dir: &Path, marker: &Marker) -> Result<(), BuildError> {
    write_marker(&RealMarkerFs, dir, marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use tempfile::TempDir;

    fn clock() -> FixedClock {
        FixedClock("2026-07-17T00:00:00Z".to_string())
    }

    // --- state construction & validation ----------------------------------

    #[test]
    fn the_three_states_have_the_right_fields() {
        let a = Marker::staging("key1", &clock());
        assert_eq!(a.kind(), MarkerKind::Staging);
        assert_eq!(a.output_key(), Some("key1"));

        let b = Marker::complete("key1", &clock());
        assert_eq!(b.kind(), MarkerKind::Site);
        assert_eq!(b.output_key(), Some("key1"));

        let c = Marker::published(&clock());
        assert_eq!(c.kind(), MarkerKind::Site);
        assert_eq!(c.output_key(), None);
    }

    #[test]
    fn published_marker_c_serializes_without_output_key_or_path() {
        let json = String::from_utf8(Marker::published(&clock()).to_bytes()).unwrap();
        assert!(!json.contains("output_key"), "{json}");
        assert!(!json.contains('/') && !json.contains('\\'), "{json}");
        assert!(json.contains("\"kind\":\"site\""));
    }

    #[test]
    fn a_to_b_to_c_preserve_the_expected_fields() {
        let a = Marker::staging("k", &clock());
        let b = Marker::complete("k", &clock());
        let c = Marker::published(&clock());
        // A and B share the key; C drops it. Kind flips staging -> site at A->B.
        assert_eq!(a.output_key(), Some("k"));
        assert_eq!(b.output_key(), Some("k"));
        assert_eq!(c.output_key(), None);
        assert_eq!(a.kind(), MarkerKind::Staging);
        assert_eq!(b.kind(), MarkerKind::Site);
    }

    #[test]
    fn validation_distinguishes_the_roles() {
        let a = Marker::staging("k", &clock());
        let b = Marker::complete("k", &clock());
        let c = Marker::published(&clock());

        assert_eq!(c.validate_as(MarkerRole::Published), Ok(()));
        assert_eq!(a.validate_as(MarkerRole::Staging("k")), Ok(()));
        assert_eq!(b.validate_as(MarkerRole::Complete("k")), Ok(()));

        // Wrong role → invalid.
        assert!(a.validate_as(MarkerRole::Published).is_err());
        assert!(c.validate_as(MarkerRole::Staging("k")).is_err());
        assert!(b.validate_as(MarkerRole::Published).is_err()); // B has a key
    }

    #[test]
    fn mismatched_output_key_is_marker_invalid() {
        let b = Marker::complete("k", &clock());
        assert_eq!(
            b.validate_as(MarkerRole::Complete("other")),
            Err(BuildError::OutputMarkerInvalid {
                reason: "its output key belongs to a different output"
            })
        );
    }

    #[test]
    fn malformed_and_unsupported_markers_are_invalid() {
        assert!(matches!(
            Marker::parse(b"not json"),
            Err(BuildError::OutputMarkerInvalid { .. })
        ));
        assert!(matches!(
            Marker::parse(br#"{"format":"other","version":1,"kind":"site","generated_at":"t"}"#),
            Err(BuildError::OutputMarkerInvalid { .. })
        ));
        assert!(matches!(
            Marker::parse(br#"{"format":"cratevista-static-site","version":2,"kind":"site","generated_at":"t"}"#),
            Err(BuildError::OutputMarkerInvalid { .. })
        ));
        // staging with no key = invalid kind/key combination.
        assert!(matches!(
            Marker::parse(br#"{"format":"cratevista-static-site","version":1,"kind":"staging","generated_at":"t"}"#),
            Err(BuildError::OutputMarkerInvalid { .. })
        ));
    }

    #[test]
    fn an_absent_marker_is_not_invalid() {
        let dir = TempDir::new().unwrap();
        assert_eq!(Marker::read_as(dir.path(), MarkerRole::Published), Ok(None));
    }

    #[test]
    fn round_trips_through_the_authoritative_file() {
        let dir = TempDir::new().unwrap();
        write_marker_real(dir.path(), &Marker::published(&clock())).unwrap();
        let read = Marker::read_as(dir.path(), MarkerRole::Published).unwrap();
        assert_eq!(read.unwrap().kind(), MarkerKind::Site);
    }

    // --- crash-safe I/O ---------------------------------------------------

    /// A fake FS that fails at a chosen step.
    #[derive(Clone, Copy, PartialEq)]
    enum FailAt {
        Create,
        Write,
        Flush,
        Rename,
    }

    struct FakeFs {
        fail: FailAt,
    }

    struct FakeFile {
        fail: FailAt,
        buf: Vec<u8>,
        target: PathBuf,
    }

    impl MarkerFs for FakeFs {
        fn create(&self, path: &Path) -> std::io::Result<Box<dyn MarkerFile>> {
            if self.fail == FailAt::Create {
                return Err(std::io::Error::other("create failed"));
            }
            // Model temp creation as a real empty file so leftovers are observable.
            std::fs::write(path, b"").unwrap();
            Ok(Box::new(FakeFile {
                fail: self.fail,
                buf: Vec::new(),
                target: path.to_path_buf(),
            }))
        }
        fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
            if self.fail == FailAt::Rename {
                return Err(std::io::Error::other("rename failed"));
            }
            std::fs::rename(from, to)
        }
        fn remove(&self, path: &Path) {
            let _ = std::fs::remove_file(path);
        }
    }

    impl MarkerFile for FakeFile {
        fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
            if self.fail == FailAt::Write {
                return Err(std::io::Error::other("write failed"));
            }
            self.buf.extend_from_slice(buf);
            std::fs::write(&self.target, &self.buf).unwrap();
            Ok(())
        }
        fn sync_all(&mut self) -> std::io::Result<()> {
            if self.fail == FailAt::Flush {
                return Err(std::io::Error::other("flush failed"));
            }
            Ok(())
        }
    }

    fn marker_bytes(dir: &Path) -> Option<Vec<u8>> {
        std::fs::read(dir.join(MARKER_FILENAME)).ok()
    }

    fn temp_files(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.contains(".tmp-"))
            })
            .collect()
    }

    #[test]
    fn a_failed_transition_leaves_the_previous_marker_byte_identical() {
        for fail in [FailAt::Create, FailAt::Write, FailAt::Flush, FailAt::Rename] {
            let dir = TempDir::new().unwrap();
            // Establish an authoritative marker C first (real FS).
            write_marker_real(dir.path(), &Marker::published(&clock())).unwrap();
            let before = marker_bytes(dir.path()).unwrap();

            // A failing transition to a B marker must not touch the authoritative file.
            let result = write_marker(
                &FakeFs { fail },
                dir.path(),
                &Marker::complete("k", &clock()),
            );
            assert!(result.is_err(), "{:?} should fail", fail as u8);
            let after = marker_bytes(dir.path()).unwrap();
            assert_eq!(
                before, after,
                "authoritative marker changed on {:?}",
                fail as u8
            );

            // No leftover temp file is left behind (best-effort cleanup ran).
            assert!(
                temp_files(dir.path()).is_empty(),
                "leftover temp on {:?}",
                fail as u8
            );
        }
    }

    #[test]
    fn no_partially_written_authoritative_marker_is_observable() {
        // Inject a write failure while there is NO prior authoritative marker.
        let dir = TempDir::new().unwrap();
        let result = write_marker(
            &FakeFs {
                fail: FailAt::Write,
            },
            dir.path(),
            &Marker::published(&clock()),
        );
        assert!(result.is_err());
        // The authoritative file was never created (only a temp, now removed).
        assert!(marker_bytes(dir.path()).is_none());
        assert!(temp_files(dir.path()).is_empty());
    }

    #[test]
    fn a_leftover_temp_marker_is_not_read_as_authoritative() {
        let dir = TempDir::new().unwrap();
        // Simulate a crash that left a temp marker but no authoritative file.
        std::fs::write(
            dir.path().join(format!("{MARKER_FILENAME}.tmp-123-0")),
            Marker::published(&clock()).to_bytes(),
        )
        .unwrap();
        // Reading the authoritative marker sees nothing.
        assert_eq!(Marker::read_as(dir.path(), MarkerRole::Published), Ok(None));
    }
}
