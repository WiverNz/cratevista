//! Broad Cargo source classification and portable-identity helpers.
//!
//! `cargo_metadata::Source::repr` is treated as an **opaque** Cargo identity: it
//! may only feed a domain-separated BLAKE3 discriminator (via
//! `cratevista_schema::EntityId::external_package_disambiguated`) for portable
//! sources, and is never exposed raw in a public id, nor parsed beyond the broad
//! classification below.

use cargo_metadata::Source;

/// A broad, reliable classification of a Cargo package source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A registry (crates.io or an alternate registry).
    Registry,
    /// A git source.
    Git,
    /// A path source.
    Path,
    /// No source, or an unrecognized representation.
    Unknown,
}

impl SourceKind {
    /// The stable attribute string for this source kind.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Registry => "registry",
            SourceKind::Git => "git",
            SourceKind::Path => "path",
            SourceKind::Unknown => "unknown",
        }
    }
}

/// Classifies a package source into a broad, reliable [`SourceKind`].
///
/// Workspace members have `source == None`; membership (not source) determines
/// member-vs-external, so `None` maps to `Unknown` here.
pub fn classify_source(source: Option<&Source>) -> SourceKind {
    match source {
        None => SourceKind::Unknown,
        Some(source) => {
            let repr = source.repr.as_str();
            if repr.starts_with("registry+") || repr.starts_with("sparse+") {
                SourceKind::Registry
            } else if repr.starts_with("git+") {
                SourceKind::Git
            } else if repr.starts_with("path+") {
                SourceKind::Path
            } else {
                SourceKind::Unknown
            }
        }
    }
}

/// Returns a **portable** normalized source string suitable for a deterministic
/// discriminator, or `None` when no portable identity exists.
///
/// Registry and git sources are URLs (portable). Path sources embed an absolute
/// filesystem path and must **never** be hashed or exposed, so they return
/// `None`.
pub fn portable_source(source: Option<&Source>) -> Option<String> {
    match classify_source(source) {
        SourceKind::Registry | SourceKind::Git => source.map(|s| s.repr.clone()),
        SourceKind::Path | SourceKind::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(repr: &str) -> Source {
        // `Source` deserializes from a bare JSON string.
        serde_json::from_value(serde_json::Value::String(repr.to_string())).unwrap()
    }

    #[test]
    fn classifies_broad_sources() {
        assert_eq!(
            classify_source(Some(&source(
                "registry+https://github.com/rust-lang/crates.io-index"
            ))),
            SourceKind::Registry
        );
        assert_eq!(
            classify_source(Some(&source("git+https://example.com/x#abc"))),
            SourceKind::Git
        );
        assert_eq!(
            classify_source(Some(&source("path+file:///abs/path"))),
            SourceKind::Path
        );
        assert_eq!(classify_source(None), SourceKind::Unknown);
    }

    #[test]
    fn path_sources_are_never_portable() {
        assert_eq!(portable_source(Some(&source("path+file:///abs"))), None);
        assert!(portable_source(Some(&source("git+https://x/y#r"))).is_some());
    }
}
