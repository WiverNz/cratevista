//! Produced-site verification on the **metadata-only** path (PRD 10, Phase 7 B/C).
//!
//! These run the REAL `run_build → run_generate → materialize_static_site` pipeline
//! on a bin-only fixture whose default `RustdocPlan` is empty, so generation succeeds
//! **on stable with no nightly and no network** (Decision 8a). They assert:
//!
//! - **Privacy (7B):** the produced `index.html` and the three JSON artifacts contain
//!   no absolute path, username, Cargo/Rustup home, argv fragment or credential — and
//!   the scan is proven able to *detect* an injected leak before reverting it.
//! - **Determinism (7C):** two builds from unchanged input produce a byte-identical
//!   `document.json` and `diagnostics.json`, an identical embedded-asset set and
//!   bytes, and an `index.html` identical except for the controlled `generated_at`
//!   marker content.
//!
//! The fixture lives inside a recognizable temporary **user** path (the OS temp dir,
//! which on Windows sits under `C:\Users\<name>\…`), so a leaked absolute path would
//! carry the username — exactly what the scan rejects.

use std::path::{Path, PathBuf};

use cratevista_core::build::{BuildOptions, run_build};
use cratevista_core::clock::FixedClock;
use cratevista_core::exit::ExitCode;
use cratevista_core::generate::GenerateOptions;

/// A bin-only crate: no lib/proc-macro target, so the rustdoc plan is empty and the
/// build is a metadata-only success needing no nightly.
fn write_bin_crate(dir: &Path) {
    std::fs::write(
        dir.join("Cargo.toml"),
        // `[workspace]` makes this its own root so it is not treated as a stray
        // package inside the CrateVista workspace it physically lives under.
        "[package]\nname = \"privfixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();
}

fn build_site(workspace: &Path, output: &Path) {
    let options = BuildOptions {
        generate: GenerateOptions {
            manifest_path: Some(workspace.join("Cargo.toml")),
            ..Default::default()
        },
        output: output.to_path_buf(),
        base_path: None,
    };
    let clock = FixedClock("2026-07-18T00:00:00Z".into());
    let outcome = run_build(&options, &clock).expect("metadata-only build succeeds");
    assert_eq!(outcome, ExitCode::SUCCESS);
}

/// Every file under `dir` as sorted `/`-joined relative strings.
fn walk(dir: &Path) -> Vec<String> {
    fn go(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                go(base, &path, out);
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
    let mut out = Vec::new();
    go(dir, dir, &mut out);
    out.sort();
    out
}

/// The public generated-data contract to scan: the index shell and the three
/// artifacts. (We deliberately do NOT scan minified JS for generic substrings, which
/// would produce meaningless false positives.)
const SCANNED: [&str; 4] = [
    "index.html",
    "document.json",
    "generation.json",
    "diagnostics.json",
];

/// Returns the needles found in any scanned file (empty = clean).
fn scan_leaks(site: &Path, needles: &[(&str, String)]) -> Vec<String> {
    let mut found = Vec::new();
    for name in SCANNED {
        let text = std::fs::read_to_string(site.join(name)).unwrap();
        for (label, needle) in needles {
            if !needle.is_empty() && text.contains(needle.as_str()) {
                found.push(format!("{label} in {name}"));
            }
        }
    }
    found
}

#[test]
fn produced_site_leaks_no_private_paths_and_the_scan_can_detect_one() {
    let workspace = tempfile::tempdir().unwrap();
    write_bin_crate(workspace.path());
    let site = workspace.path().join("site");
    build_site(workspace.path(), &site);

    // The site is complete.
    for name in SCANNED {
        assert!(site.join(name).is_file(), "{name} present");
    }

    // Build the forbidden-needle set from the real environment + build paths.
    let mut needles: Vec<(&str, String)> = Vec::new();
    let ws = workspace.path().to_string_lossy().to_string();
    needles.push(("workspace absolute path", ws.clone()));
    needles.push(("workspace path (forward-slash)", ws.replace('\\', "/")));
    needles.push(("site absolute path", site.to_string_lossy().to_string()));
    if let Ok(user) = std::env::var("USERNAME").or_else(|_| std::env::var("USER"))
        && user.len() >= 3
    {
        needles.push(("username", user));
    }
    for var in ["CARGO_HOME", "RUSTUP_HOME"] {
        if let Ok(val) = std::env::var(var) {
            needles.push(("toolchain home", val));
        }
    }
    for marker in ["/home/", "/Users/", "\\Users\\"] {
        needles.push(("home-dir marker", marker.to_string()));
    }
    // Windows drive-absolute spellings and UNC.
    for marker in ["C:\\", "C:/", "\\\\?\\"] {
        needles.push(("drive/UNC path", marker.to_string()));
    }
    for marker in [
        "--output-format",
        "rustdoc ",
        "CARGO_REGISTRY_TOKEN",
        "-----BEGIN",
    ] {
        needles.push(("argv/credential fragment", marker.to_string()));
    }

    let leaks = scan_leaks(&site, &needles);
    assert!(leaks.is_empty(), "produced site leaked: {leaks:?}");

    // Prove the scan actually detects a leak: inject an absolute path + a credential
    // into a COPY of index.html, scan, confirm detection, then revert.
    let index = site.join("index.html");
    let original = std::fs::read_to_string(&index).unwrap();
    let injected = format!(
        "{original}\n<!-- C:\\Users\\victim\\secret and https://user:pass@example.com -->\n"
    );
    std::fs::write(&index, &injected).unwrap();
    let control = scan_leaks(
        &site,
        &[
            ("injected drive path", "C:\\".to_string()),
            ("injected credential url", "user:pass@".to_string()),
        ],
    );
    assert!(
        control.len() >= 2,
        "the privacy scan must DETECT an injected leak, found: {control:?}"
    );
    std::fs::write(&index, &original).unwrap();
}

/// The output lives outside every protected input (the workspace + its artifact
/// dir). `build` publishes to a sibling `site/`, never over an input.
#[test]
fn produced_site_output_is_outside_protected_inputs() {
    let workspace = tempfile::tempdir().unwrap();
    write_bin_crate(workspace.path());
    let site = workspace.path().join("site");
    build_site(workspace.path(), &site);

    let artifacts = workspace.path().join("target").join("cratevista");
    // The site is not the artifact dir, not the workspace root, and does not contain
    // either.
    assert!(site.exists() && site != artifacts && site != workspace.path());
    assert!(
        !artifacts.starts_with(&site) && !workspace.path().starts_with(&site),
        "the output must not be an ancestor of a protected input"
    );
}

#[test]
fn two_builds_from_unchanged_input_are_deterministic() {
    let workspace = tempfile::tempdir().unwrap();
    write_bin_crate(workspace.path());

    let site_a = workspace.path().join("a");
    let site_b = workspace.path().join("b");
    build_site(workspace.path(), &site_a);
    build_site(workspace.path(), &site_b);

    // document.json + diagnostics.json byte-identical.
    for name in ["document.json", "diagnostics.json"] {
        assert_eq!(
            std::fs::read(site_a.join(name)).unwrap(),
            std::fs::read(site_b.join(name)).unwrap(),
            "{name} must be byte-identical across unchanged builds"
        );
    }

    // The embedded asset SET is identical.
    let assets = |root: &Path| -> Vec<String> {
        walk(root)
            .into_iter()
            .filter(|f| f.starts_with("assets/"))
            .collect()
    };
    let (aa, ba) = (assets(&site_a), assets(&site_b));
    assert_eq!(aa, ba, "embedded asset set differs");
    assert!(!aa.is_empty(), "there is at least one embedded asset");

    // And the embedded asset BYTES are identical.
    for rel in &aa {
        assert_eq!(
            std::fs::read(site_a.join(rel)).unwrap(),
            std::fs::read(site_b.join(rel)).unwrap(),
            "asset {rel} differs between builds"
        );
    }

    // index.html is identical under the fixed clock (its only generated-at-dependent
    // content is the marker, which the FixedClock pins).
    assert_eq!(
        std::fs::read(site_a.join("index.html")).unwrap(),
        std::fs::read(site_b.join("index.html")).unwrap(),
        "index.html must be identical when generated_at is controlled by the clock"
    );
}

/// The three artifacts materialized into the site are the exact committed generation
/// snapshot bytes (no re-serialization or summarizing).
#[test]
fn materialized_artifacts_equal_the_committed_generation_snapshot() {
    let workspace = tempfile::tempdir().unwrap();
    write_bin_crate(workspace.path());
    let site: PathBuf = workspace.path().join("site");
    build_site(workspace.path(), &site);

    let committed = workspace.path().join("target").join("cratevista");
    for name in ["document.json", "generation.json", "diagnostics.json"] {
        assert_eq!(
            std::fs::read(site.join(name)).unwrap(),
            std::fs::read(committed.join(name)).unwrap(),
            "{name} differs from the committed snapshot"
        );
    }
}
