//! Nightly toolchain selection. **Never installs** a toolchain.
//!
//! The default is the pinned nightly (ADR-0004). Only an explicit
//! `RustdocOptions::toolchain` overrides it: neither `RUSTUP_TOOLCHAIN` nor a
//! `rustup toolchain list` scan is allowed to silently replace the pinned tuple,
//! because that would break the format-version guarantee.

use crate::compat::PINNED_NIGHTLY;
use crate::options::RustdocOptions;

/// Resolves the toolchain to invoke: the explicit override, else the pinned
/// nightly. Availability is checked at invocation time, never here.
pub fn resolve_toolchain(options: &RustdocOptions) -> String {
    options
        .toolchain
        .clone()
        .unwrap_or_else(|| PINNED_NIGHTLY.to_string())
}

/// Whether `toolchain` is the pinned nightly (used to pick the right fatal
/// error — `NightlyMissing` vs `ToolchainNotFound` — when it is absent).
pub fn is_pinned(toolchain: &str) -> bool {
    toolchain == PINNED_NIGHTLY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_pinned_nightly() {
        assert_eq!(
            resolve_toolchain(&RustdocOptions::default()),
            PINNED_NIGHTLY
        );
        assert!(is_pinned(&resolve_toolchain(&RustdocOptions::default())));
    }

    #[test]
    fn explicit_override_wins() {
        let options = RustdocOptions {
            toolchain: Some("nightly-custom".into()),
            ..Default::default()
        };
        assert_eq!(resolve_toolchain(&options), "nightly-custom");
        assert!(!is_pinned(&resolve_toolchain(&options)));
    }
}
