//! Regenerates the checked-in `*.rustdoc.json` fixtures from the `sample_lib`
//! fixture crate using the pinned nightly (ADR-0004), then **sanitizes** them so
//! no absolute machine path is committed (matching `cratevista-metadata`'s
//! fixture policy).
//!
//! Run from the workspace root with the pinned nightly installed:
//!
//! ```text
//! cargo run -p cratevista-rustdoc --example gen_rustdoc_fixtures
//! ```
//!
//! It produces:
//! - `tests/fixtures/sample_lib.rustdoc.json`         (public items only)
//! - `tests/fixtures/sample_lib_private.rustdoc.json` (with private items)
//!
//! This is a developer tool, not part of the library. It is intentionally
//! excluded from the stable test suite (it invokes nightly `cargo rustdoc`).

use std::path::{Path, PathBuf};
use std::process::Command;

const PINNED_NIGHTLY: &str = "nightly-2026-07-01";

fn main() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_crate = crate_dir.join("tests").join("fixtures").join("sample_lib");
    let manifest = fixture_crate.join("Cargo.toml");

    generate(
        &manifest,
        false,
        &crate_dir.join("tests/fixtures/sample_lib.rustdoc.json"),
    );
    generate(
        &manifest,
        true,
        &crate_dir.join("tests/fixtures/sample_lib_private.rustdoc.json"),
    );
    println!("regenerated fixtures with {PINNED_NIGHTLY}");
}

fn generate(manifest: &Path, private: bool, out: &Path) {
    let target_dir = std::env::temp_dir().join(format!(
        "cratevista-fixture-{}",
        if private { "private" } else { "public" }
    ));
    let mut argv = vec![
        format!("+{PINNED_NIGHTLY}"),
        "rustdoc".into(),
        "-Z".into(),
        "unstable-options".into(),
        "--output-format".into(),
        "json".into(),
        "--manifest-path".into(),
        manifest.display().to_string(),
        "-p".into(),
        "sample_lib".into(),
        "--lib".into(),
        "--target-dir".into(),
        target_dir.display().to_string(),
        "--".into(),
    ];
    if private {
        argv.push("--document-private-items".into());
    }

    let status = Command::new("cargo")
        .args(&argv)
        .status()
        .expect("run cargo rustdoc (is the pinned nightly installed?)");
    assert!(status.success(), "cargo rustdoc failed");

    let produced = target_dir.join("doc").join("sample_lib.json");
    let raw = std::fs::read_to_string(&produced).expect("read produced JSON");
    let sanitized = sanitize(&raw);
    std::fs::write(out, sanitized).expect("write fixture");
    println!("wrote {}", out.display());
}

/// Replaces machine-specific absolute toolchain/cargo prefixes with stable
/// placeholders so no absolute path is committed.
///
/// Operates on the **raw** JSON text (never a global backslash replacement,
/// which would corrupt JSON escape sequences): for each occurrence of the
/// `.rustup`/`.cargo` marker, it collapses the whole quoted absolute prefix
/// (`"C:\\Users\\name\\.rustup`) down to a stable, username-free marker
/// (`"/rustup`), leaving the remainder of the path intact.
fn sanitize(json: &str) -> String {
    let collapsed = collapse_home_prefix(json, ".rustup", "/rustup");
    collapse_home_prefix(&collapsed, ".cargo", "/cargo")
}

fn collapse_home_prefix(input: &str, marker: &str, replacement: &str) -> String {
    let mut out = input.to_string();
    let mut from = 0usize;
    while let Some(offset) = out[from..].find(marker) {
        let index = from + offset;
        // Walk back to the start of the quoted absolute path segment.
        let start = out[..index].rfind('"').map(|quote| quote + 1).unwrap_or(0);
        out.replace_range(start..index + marker.len(), replacement);
        from = start + replacement.len();
    }
    out
}
