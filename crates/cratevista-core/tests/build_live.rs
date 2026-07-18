//! Gated end-to-end `build` test: runs the real
//! `run_build → run_generate → materialize_static_site` pipeline on a tiny
//! path-only library crate using the pinned nightly (`nightly-2026-07-01`) for
//! rustdoc JSON, with the **real** embedded frontend bundle. Ignored by default;
//! **no network**, **no mocked generator**.
//!
//! ```text
//! cargo +nightly-2026-07-01 test -p cratevista-core --test build_live -- --ignored --exact build_live_materializes_a_static_site
//! ```

use std::path::{Path, PathBuf};

use cratevista_core::build::{BuildOptions, run_build};
use cratevista_core::clock::FixedClock;
use cratevista_core::exit::ExitCode;
use cratevista_core::generate::GenerateOptions;
use cratevista_core::static_site::{MARKER_FILENAME, Marker, MarkerKind};

fn write_lib_crate(dir: &Path, name: &str) {
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n"
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src").join("lib.rs"),
        "//! A tiny crate.\n\n/// A documented struct.\npub struct Widget {\n    /// The size.\n    pub size: u32,\n}\n",
    )
    .unwrap();
}

/// Every file under `dir`, as workspace-relative `/`-joined strings.
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

#[test]
#[ignore = "requires the pinned nightly toolchain; run with --ignored"]
fn build_live_materializes_a_static_site() {
    let workspace = tempfile::tempdir().unwrap();
    write_lib_crate(workspace.path(), "livelib");
    let output: PathBuf = workspace.path().join("dist-site");

    let options = BuildOptions {
        generate: GenerateOptions {
            manifest_path: Some(workspace.path().join("Cargo.toml")),
            ..Default::default()
        },
        output: output.clone(),
        base_path: None,
    };
    let clock = FixedClock("2026-07-18T00:00:00Z".into());
    let outcome = run_build(&options, &clock).expect("live build succeeds");
    assert_eq!(outcome, ExitCode::SUCCESS);

    // Frontend + artifacts are present.
    assert!(output.join("index.html").is_file(), "index.html present");
    assert!(
        output.join("document.json").is_file(),
        "document.json present"
    );
    assert!(
        output.join("generation.json").is_file(),
        "generation.json present"
    );
    assert!(
        output.join("diagnostics.json").is_file(),
        "diagnostics.json present"
    );

    // Static-mode meta exactly once.
    let index = std::fs::read_to_string(output.join("index.html")).unwrap();
    assert_eq!(
        index.matches(r#"name="cratevista-mode""#).count(),
        1,
        "exactly one static-mode meta"
    );

    // The final marker is C: a site marker with no output_key.
    let marker = Marker::parse(&std::fs::read(output.join(MARKER_FILENAME)).unwrap()).unwrap();
    assert_eq!(marker.kind(), MarkerKind::Site);
    assert_eq!(
        marker.output_key(),
        None,
        "published marker C omits the key"
    );

    // At least one embedded fingerprinted asset was written.
    let files = walk(&output);
    assert!(
        files
            .iter()
            .any(|f| f.starts_with("assets/") && f.ends_with(".js")),
        "a bundled script asset must be materialized: {files:?}"
    );

    // The materialized artifacts match the committed generation snapshot byte-for-byte.
    let committed = workspace.path().join("target").join("cratevista");
    for name in ["document.json", "generation.json", "diagnostics.json"] {
        assert_eq!(
            std::fs::read(output.join(name)).unwrap(),
            std::fs::read(committed.join(name)).unwrap(),
            "{name} differs from the committed snapshot"
        );
    }

    // No `/api` files and no source snippets are produced.
    assert!(
        !files.iter().any(|f| f.contains("api")),
        "no /api files: {files:?}"
    );
    assert!(
        !files.iter().any(|f| f.starts_with("source/")),
        "no source snippets: {files:?}"
    );

    // The rustdoc compatibility tuple: this real generation went through rustdoc JSON
    // `format_version` 60 (the observable half of the pinned tuple
    // nightly-2026-07-01 → format 60 → rustdoc-types 0.60.0 → adapter 1). The
    // rustdoc-types release and adapter version are compile-time constants of
    // `cratevista-rustdoc`, exercised by producing a real document here at all.
    let generation = std::fs::read_to_string(output.join("generation.json")).unwrap();
    let compact = generation.replace(' ', "");
    assert!(
        compact.contains("\"rustdoc_format_version\":60"),
        "generation.json must record rustdoc format 60: {generation}"
    );

    // Privacy scan over the produced public data contract: no absolute path,
    // username, toolchain home, argv fragment or credential leaks into the site.
    let mut needles: Vec<String> = vec![
        workspace.path().to_string_lossy().to_string(),
        workspace.path().to_string_lossy().replace('\\', "/"),
        "/home/".into(),
        "/Users/".into(),
        "\\Users\\".into(),
        "C:\\".into(),
        "\\\\?\\".into(),
        "--output-format".into(),
        "-----BEGIN".into(),
        "CARGO_REGISTRY_TOKEN".into(),
    ];
    if let Ok(user) = std::env::var("USERNAME").or_else(|_| std::env::var("USER"))
        && user.len() >= 3
    {
        needles.push(user);
    }
    for var in ["CARGO_HOME", "RUSTUP_HOME"] {
        if let Ok(val) = std::env::var(var) {
            needles.push(val);
        }
    }
    for name in [
        "index.html",
        "document.json",
        "generation.json",
        "diagnostics.json",
    ] {
        let text = std::fs::read_to_string(output.join(name)).unwrap();
        for needle in &needles {
            assert!(
                !needle.is_empty() && !text.contains(needle.as_str()),
                "produced {name} leaked `{needle}`"
            );
        }
    }

    // Temporary output is cleaned up when `workspace` drops.
}
