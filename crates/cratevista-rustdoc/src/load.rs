//! Deserializing rustdoc JSON behind the compatibility gate.
//!
//! Raw `rustdoc_types` never appear in a public signature: [`load_raw`] and
//! [`normalize_raw`] are crate-private; the public surface is [`normalize_json`]
//! and [`load_and_normalize`], which take JSON/paths and CrateVista-owned types.

use std::path::Path;

use rustdoc_types::Crate;
use serde::Deserialize;

use crate::compat;
use crate::error::RustdocError;
use crate::normalize::normalize_crate;
use crate::options::NormalizeContext;
use crate::result::CrateIngest;

/// A minimal probe read before the full parse so a wrong-but-parseable format
/// version yields a clean `UnsupportedFormatVersion` rather than a structural
/// deserialization error.
#[derive(Deserialize)]
struct VersionProbe {
    format_version: u32,
}

/// Deserializes rustdoc JSON and enforces the format-version gate. Crate-private.
pub(crate) fn load_raw(json: &str) -> Result<Crate, RustdocError> {
    let probe: VersionProbe = serde_json::from_str(json)
        .map_err(|error| RustdocError::MalformedRustdocJson(error.to_string()))?;
    compat::check_format_version(probe.format_version)?;
    let krate: Crate = serde_json::from_str(json)
        .map_err(|error| RustdocError::MalformedRustdocJson(error.to_string()))?;
    Ok(krate)
}

/// Normalizes an already-parsed crate. Crate-private (takes a raw type).
pub(crate) fn normalize_raw(
    krate: &Crate,
    context: &NormalizeContext,
) -> Result<CrateIngest, RustdocError> {
    normalize_crate(krate, context)
}

/// Parses rustdoc JSON (with the compat gate) and normalizes it. Stable-testable.
pub fn normalize_json(json: &str, context: &NormalizeContext) -> Result<CrateIngest, RustdocError> {
    let krate = load_raw(json)?;
    normalize_raw(&krate, context)
}

/// Reads a rustdoc JSON file, parses it (with the compat gate), and normalizes it.
pub fn load_and_normalize(
    path: &Path,
    context: &NormalizeContext,
) -> Result<CrateIngest, RustdocError> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| RustdocError::OutputFileMissing(format!("{}: {error}", path.display())))?;
    normalize_json(&json, context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incompatible_version_is_rejected() {
        let json = r#"{"format_version": 59}"#;
        let error = load_raw(json).unwrap_err();
        assert_eq!(error.code(), "unsupported_format_version");
    }

    #[test]
    fn broken_json_is_malformed() {
        let error = load_raw("{ not json").unwrap_err();
        assert_eq!(error.code(), "malformed_rustdoc_json");
    }
}
