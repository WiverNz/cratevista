//! The compatibility tuple and the format-version gate.
//!
//! The authoritative record of the verified tuple (pinned nightly, format
//! version, `rustdoc-types` release, adapter version) is
//! `docs/adr/0004-rustdoc-toolchain-policy.md`. These constants mirror it. The
//! compile-time assertion below fails the build if the linked `rustdoc-types`
//! `FORMAT_VERSION` ever drifts from [`EXPECTED_FORMAT_VERSION`], forcing a
//! deliberate tuple bump.

use crate::error::RustdocError;

/// The pinned nightly toolchain verified to emit format version
/// [`EXPECTED_FORMAT_VERSION`] (see ADR-0004).
pub const PINNED_NIGHTLY: &str = "nightly-2026-07-01";

/// The rustdoc JSON `format_version` this adapter supports.
pub const EXPECTED_FORMAT_VERSION: u32 = 60;

/// The `rustdoc-types` release line (the exact patch is captured in `Cargo.lock`).
pub const RUSTDOC_TYPES_RELEASE: &str = "0.60";

/// The CrateVista rustdoc-adapter version. Bumping it signals an
/// intentional change to the normalized identity/shape.
pub const ADAPTER_VERSION: u32 = 1;

// Keep the adapter's expected version locked to the linked `rustdoc-types`.
const _: () = assert!(
    EXPECTED_FORMAT_VERSION == rustdoc_types::FORMAT_VERSION,
    "rustdoc-types FORMAT_VERSION drifted from EXPECTED_FORMAT_VERSION; bump the compatibility tuple (ADR-0004)"
);

/// Gates a parsed crate's `format_version` against [`EXPECTED_FORMAT_VERSION`].
pub fn check_format_version(found: u32) -> Result<(), RustdocError> {
    if found == EXPECTED_FORMAT_VERSION {
        Ok(())
    } else {
        Err(RustdocError::UnsupportedFormatVersion {
            found,
            supported: EXPECTED_FORMAT_VERSION,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_version_is_accepted() {
        assert!(check_format_version(EXPECTED_FORMAT_VERSION).is_ok());
    }

    #[test]
    fn mismatched_version_names_both_sides() {
        let error = check_format_version(59).unwrap_err();
        assert_eq!(error.code(), "unsupported_format_version");
        let message = error.to_string();
        assert!(message.contains("59"));
        assert!(message.contains("60"));
        assert!(message.contains(PINNED_NIGHTLY));
    }
}
