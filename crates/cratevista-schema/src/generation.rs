//! The `generation.json` artifact: runtime metadata kept out of the
//! deterministic `document.json`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An RFC 3339 timestamp string. Non-deterministic; lives only in
/// `generation.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Timestamp(String);

impl Timestamp {
    /// Wraps an RFC 3339 timestamp string.
    pub fn new(value: impl Into<String>) -> Self {
        Timestamp(value.into())
    }

    /// The timestamp as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The tool that produced the artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Generator {
    /// Tool name (e.g. `cargo-cratevista`).
    pub name: String,
    /// Tool version.
    pub version: String,
}

/// BLAKE3 content hashes of the sibling artifacts, embedded in `generation.json`
/// so a reader can prove that a `document.json` / `diagnostics.json` pair belongs
/// to this generation (see `PRD/issue_06_server_and_embedded_ui.md`).
///
/// Each digest is BLAKE3 over the **exact canonical UTF-8 bytes** written to disk,
/// encoded as lowercase hexadecimal: exactly 64 ASCII characters, no `0x` prefix,
/// no whitespace. Content only — no absolute paths. `generation.json` does not
/// hash itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactHashes {
    /// BLAKE3 (lowercase hex, 64 chars) of `document.json`'s canonical bytes.
    pub document_blake3: String,
    /// BLAKE3 (lowercase hex, 64 chars) of `diagnostics.json`'s canonical bytes.
    pub diagnostics_blake3: String,
}

/// Counts of the produced elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Counts {
    /// Number of entities.
    pub entities: u64,
    /// Number of relations.
    pub relations: u64,
    /// Number of views.
    pub views: u64,
    /// Number of diagnostics.
    pub diagnostics: u64,
}

/// Runtime metadata for a generation run. Serialized to `generation.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GenerationReport {
    /// The producing tool.
    pub generator: Generator,
    /// When the run happened (non-deterministic).
    pub generated_at: Timestamp,
    /// The toolchain used for rustdoc JSON, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<String>,
    /// The rustdoc JSON format version, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rustdoc_format_version: Option<u32>,
    /// Hashes of the inputs that determined the output.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub input_hashes: BTreeMap<String, String>,
    /// Element counts.
    pub counts: Counts,
    /// Per-phase durations in milliseconds.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub durations_ms: BTreeMap<String, u64>,
    /// BLAKE3 content hashes of `document.json` / `diagnostics.json`.
    ///
    /// Optional for backward-compatible deserialization (a pre-amendment
    /// `generation.json` has none), but the current generator **always**
    /// populates it; the server requires it for snapshot integrity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_hashes: Option<ArtifactHashes>,
    /// `true` when produced under `--keep-going` with some targets skipped.
    pub partial: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_hashes_is_backward_compatible_optional() {
        // A pre-amendment generation.json (no artifact_hashes) still deserializes.
        let json = r#"{
            "generator": {"name": "cargo-cratevista", "version": "0.1.0"},
            "generated_at": "2026-07-14T00:00:00Z",
            "counts": {"entities": 0, "relations": 0, "views": 0, "diagnostics": 0},
            "partial": false
        }"#;
        let report: GenerationReport = serde_json::from_str(json).unwrap();
        assert!(report.artifact_hashes.is_none());
    }

    #[test]
    fn artifact_hashes_round_trips_when_present() {
        let report = GenerationReport {
            generator: Generator {
                name: "cargo-cratevista".into(),
                version: "0.1.0".into(),
            },
            generated_at: Timestamp::new("2026-07-14T00:00:00Z"),
            toolchain: None,
            rustdoc_format_version: None,
            input_hashes: BTreeMap::new(),
            counts: Counts {
                entities: 0,
                relations: 0,
                views: 0,
                diagnostics: 0,
            },
            durations_ms: BTreeMap::new(),
            artifact_hashes: Some(ArtifactHashes {
                document_blake3: "a".repeat(64),
                diagnostics_blake3: "b".repeat(64),
            }),
            partial: false,
        };
        let text = serde_json::to_string(&report).unwrap();
        let back: GenerationReport = serde_json::from_str(&text).unwrap();
        assert_eq!(report, back);
    }
}
