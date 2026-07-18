//! Package-topology and packaging-file-set checks (PRD 10, Phase 5A).
//!
//! These are the reusable repository checks for the publishable package set: the
//! crate-local licence drift guard, the internal-dependency version audit, and the
//! `cargo package` file-set assertions. They live in one existing test crate rather
//! than a new xtask or a tenth publishable crate, and use only `std` + `cargo`, so
//! they run on every CI operating system.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace root (two levels up from this crate).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

// ---------------------------------------------------------------------------
// A/B — bundle relocation: web/dist is gone; no code reads it
// ---------------------------------------------------------------------------

#[test]
fn the_old_web_dist_bundle_no_longer_exists() {
    let root = repo_root();
    assert!(
        !root.join("web").join("dist").exists(),
        "web/dist must not exist after relocation to crates/cratevista-server/embedded"
    );
    assert!(
        root.join("crates/cratevista-server/embedded/index.html")
            .is_file(),
        "the authoritative bundle must live at crates/cratevista-server/embedded"
    );
}

#[test]
fn no_active_production_code_references_the_old_bundle_path() {
    let root = repo_root();
    for rel in [
        "crates/cratevista-server/src/assets.rs",
        "crates/cratevista-server/build.rs",
    ] {
        let text = std::fs::read_to_string(root.join(rel)).unwrap();
        assert!(
            !text.contains("../../web/dist"),
            "{rel} must not reference the old ../../web/dist path"
        );
    }
}

// ---------------------------------------------------------------------------
// G — crate-local licence drift
// ---------------------------------------------------------------------------

/// Run locally with: `cargo test -p cargo-cratevista --test packaging licenses`.
#[test]
fn licenses_match_root_byte_for_byte() {
    let root = repo_root();
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in ["LICENSE-MIT", "LICENSE-APACHE"] {
        let root_path = root.join(name);
        let local_path = crate_dir.join(name);
        let root_bytes = std::fs::read(&root_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", root_path.display()));
        let local_bytes = std::fs::read(&local_path).unwrap_or_else(|_| {
            panic!(
                "crate-local {} is missing — copy the root {name} byte-for-byte",
                local_path.display()
            )
        });
        assert_eq!(
            root_bytes, local_bytes,
            "{name} drifted: crates/cargo-cratevista/{name} differs from the root {name} \
             (byte or line-ending change)"
        );
    }
}

/// The `cargo-cratevista` manifest must reference a **crate-local** readme, not one
/// outside the package (which would not be published to crates.io).
#[test]
fn cargo_cratevista_readme_is_crate_local() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let readme = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("readme ="))
        .expect("cargo-cratevista must set `readme`");
    assert!(
        readme.contains("\"README.md\""),
        "readme must be the crate-local README.md, not an out-of-crate path: {readme}"
    );
    assert!(
        !readme.contains(".."),
        "readme must not point outside the crate: {readme}"
    );
}

// ---------------------------------------------------------------------------
// D — internal dependency edges declare both path and version
// ---------------------------------------------------------------------------

/// The nine workspace crates.
const WORKSPACE_CRATES: [&str; 9] = [
    "cratevista-schema",
    "cratevista-metadata",
    "cratevista-rustdoc",
    "cratevista-graph",
    "cratevista-config",
    "cratevista-server",
    "cratevista-watch",
    "cratevista-core",
    "cargo-cratevista",
];

#[test]
fn every_internal_workspace_dependency_edge_declares_a_version() {
    let root = repo_root();
    let root_manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();

    // In `[workspace.dependencies]`, each internal crate line must carry BOTH a
    // `path = "..."` and a `version = "..."`, so packaged manifests get a registry
    // requirement instead of a workspace-relative path.
    for crate_name in WORKSPACE_CRATES {
        let Some(line) = root_manifest
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("{crate_name} = {{")))
        else {
            // `cargo-cratevista` is a leaf binary; it need not be a dependency edge.
            continue;
        };
        assert!(
            line.contains("path ="),
            "internal edge `{crate_name}` must keep its `path`: {line}"
        );
        assert!(
            line.contains("version ="),
            "internal edge `{crate_name}` must declare a `version` for packaging: {line}"
        );
    }

    // No crate manifest may carry a RAW internal path dependency (outside the
    // workspace table) without a version — every internal edge must inherit the
    // versioned workspace definition (`{ workspace = true }`).
    for crate_name in WORKSPACE_CRATES {
        let manifest =
            std::fs::read_to_string(root.join("crates").join(crate_name).join("Cargo.toml"))
                .unwrap();
        for line in manifest.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("cratevista-")
                && trimmed.contains("path =")
                && !trimmed.contains("version =")
            {
                panic!("{crate_name}: raw internal path edge without a version: {line}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// E / H / J — packaged file sets (ignored: runs `cargo package`, run explicitly)
// ---------------------------------------------------------------------------

/// The files `cargo package --list -p <crate>` reports, in a dirty local tree.
fn package_list(crate_name: &str) -> Vec<String> {
    let output = Command::new(env!("CARGO"))
        .current_dir(repo_root())
        .args([
            "package",
            "--list",
            "--allow-dirty",
            "--quiet",
            "-p",
            crate_name,
        ])
        .output()
        .unwrap_or_else(|e| panic!("run cargo package --list -p {crate_name}: {e}"));
    assert!(
        output.status.success(),
        "cargo package --list -p {crate_name} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.replace('\\', "/"))
        .filter(|l| !l.is_empty())
        .collect()
}

/// The authoritative embedded asset set (relative to the embedded dir).
fn authoritative_embedded_files() -> Vec<String> {
    let dir = repo_root()
        .join("crates")
        .join("cratevista-server")
        .join("embedded");
    let mut out = Vec::new();
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.push(
                    path.strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    walk(&dir, &dir, &mut out);
    out.sort();
    out
}

/// Run with: `cargo test -p cargo-cratevista --test packaging -- --ignored`.
#[test]
#[ignore = "runs `cargo package --list`; run explicitly as the package-file-set gate"]
fn package_file_sets_are_correct() {
    // Every crate can be listed.
    for crate_name in WORKSPACE_CRATES {
        let files = package_list(crate_name);
        assert!(
            files
                .iter()
                .any(|f| f == "Cargo.toml" || f.ends_with("/Cargo.toml")),
            "{crate_name} package must contain Cargo.toml: {files:?}"
        );
        // No package may carry the old bundle location, node_modules, generated
        // site output, or the temporary comparison directory.
        for forbidden in [
            "web/dist",
            "node_modules",
            "target/cratevista/site",
            "cratevista-dist-",
        ] {
            assert!(
                !files.iter().any(|f| f.contains(forbidden)),
                "{crate_name} package must not contain `{forbidden}`: {files:?}"
            );
        }
    }

    // cratevista-server: the embedded bundle must be packaged, exactly.
    let server = package_list("cratevista-server");
    assert!(
        server.iter().any(|f| f == "embedded/index.html"),
        "server package must contain embedded/index.html: {server:?}"
    );
    assert!(
        server.iter().any(|f| f == "src/assets.rs"),
        "server package must contain src/**: {server:?}"
    );
    assert!(
        server.iter().any(|f| f == "build.rs"),
        "server package must contain build.rs: {server:?}"
    );
    let packaged_assets: Vec<String> = server
        .iter()
        .filter(|f| f.starts_with("embedded/"))
        .cloned()
        .collect();
    let mut expected: Vec<String> = authoritative_embedded_files()
        .into_iter()
        .map(|f| format!("embedded/{f}"))
        .collect();
    expected.sort();
    let mut actual = packaged_assets.clone();
    actual.sort();
    assert_eq!(
        actual, expected,
        "server packaged embedded set must equal the authoritative embedded set exactly"
    );

    // cargo-cratevista: crate-local README + both licences, no external paths.
    let cli = package_list("cargo-cratevista");
    for required in ["README.md", "LICENSE-MIT", "LICENSE-APACHE", "src/main.rs"] {
        assert!(
            cli.iter().any(|f| f == required),
            "cargo-cratevista package must contain {required}: {cli:?}"
        );
    }
}
