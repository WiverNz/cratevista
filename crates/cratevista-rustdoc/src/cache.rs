//! The rustdoc cache-key computation (issue 04 defines the key; the persistent
//! cache that would use it is `ISSUES/issue_12_persistent_cache.md`).
//!
//! A cache key must change whenever any semantic input changes: the target,
//! selected features, private-item mode, nightly toolchain, rustdoc format
//! version, adapter version, and the caller-supplied digest of relevant Cargo
//! inputs (source file hashes + `Cargo.lock` hash). The key is a domain-framed
//! BLAKE3 hash, so it never leaks an absolute path.

use crate::options::{RustdocOptions, RustdocTarget};
use crate::result::CompatibilityTuple;

const CACHE_DOMAIN: &[u8] = b"cratevista-rustdoc-cache:v1:";

/// Computes a deterministic cache key over all semantic inputs.
///
/// `input_digest` is a caller-computed digest of the target's source files and
/// `Cargo.lock` (`ISSUES/issue_12_persistent_cache.md` supplies it); it is
/// treated opaquely here.
pub fn cache_key(
    target: &RustdocTarget,
    options: &RustdocOptions,
    compat: &CompatibilityTuple,
    input_digest: &str,
) -> String {
    let mut features = options.features.features.clone();
    features.sort();
    features.dedup();

    let components: Vec<String> = vec![
        format!("target_id={}", target.target_id),
        format!("package={}", target.package_name),
        format!("target={}", target.target_name),
        format!("crate={}", target.crate_name),
        format!("kind={}", target.target_kind.as_str()),
        format!("features={}", features.join(",")),
        format!("all_features={}", options.features.all_features),
        format!(
            "no_default_features={}",
            options.features.no_default_features
        ),
        format!("include_private={}", options.include_private),
        format!("nightly={}", compat.nightly),
        format!("format_version={}", compat.format_version),
        format!("rustdoc_types={}", compat.rustdoc_types),
        format!("adapter={}", compat.adapter),
        format!("inputs={input_digest}"),
    ];

    let mut hasher = blake3::Hasher::new();
    hasher.update(CACHE_DOMAIN);
    for component in &components {
        hasher.update(component.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(component.as_bytes());
        hasher.update(b":");
    }
    hasher.finalize().to_hex().as_str()[..32].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::RustdocTargetKind;
    use std::path::PathBuf;

    fn target() -> RustdocTarget {
        RustdocTarget {
            package_id: cratevista_schema::EntityId::package("foo"),
            target_id: cratevista_schema::EntityId::target("foo", "lib", "foo"),
            package_name: "foo".into(),
            target_name: "foo".into(),
            crate_name: "foo".into(),
            target_kind: RustdocTargetKind::Library,
            manifest_path: PathBuf::from("/w/foo/Cargo.toml"),
            package_root: PathBuf::from("/w/foo"),
        }
    }

    #[test]
    fn key_is_stable_and_sensitive() {
        let compat = CompatibilityTuple::current("nightly-x");
        let base = cache_key(&target(), &RustdocOptions::default(), &compat, "digest-a");
        assert_eq!(
            base,
            cache_key(&target(), &RustdocOptions::default(), &compat, "digest-a")
        );
        assert_eq!(base.len(), 32);

        // Private mode changes the key.
        let private = RustdocOptions {
            include_private: true,
            ..Default::default()
        };
        assert_ne!(base, cache_key(&target(), &private, &compat, "digest-a"));

        // Different Cargo inputs change the key.
        assert_ne!(
            base,
            cache_key(&target(), &RustdocOptions::default(), &compat, "digest-b")
        );

        // A different nightly changes the key.
        let other = CompatibilityTuple::current("nightly-y");
        assert_ne!(
            base,
            cache_key(&target(), &RustdocOptions::default(), &other, "digest-a")
        );
    }
}
