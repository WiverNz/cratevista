//! End-to-end CLI tests for `cargo-cratevista`.

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("cargo-cratevista").expect("binary builds")
}

#[test]
fn help_lists_all_mvp_commands() {
    let assert = bin().arg("--help").assert().success();
    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    for command in ["init", "doctor", "generate", "serve", "open", "build"] {
        assert!(
            stdout.contains(command),
            "help should list `{command}`\n{stdout}"
        );
    }
}

#[test]
fn external_subcommand_token_is_stripped() {
    // Simulates `cargo cratevista --help`, which invokes us as
    // `cargo-cratevista cratevista --help`.
    bin()
        .arg("cratevista")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"));
}

#[test]
fn no_subcommand_is_a_usage_error() {
    bin().assert().failure().code(2);
}

#[test]
fn unknown_argument_is_a_usage_error() {
    bin().arg("--totally-unknown").assert().failure().code(2);
}

#[test]
fn init_creates_config_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();

    bin().arg("init").current_dir(dir.path()).assert().success();
    let config = dir.path().join("cratevista.toml");
    assert!(config.is_file());
    let first = std::fs::read_to_string(&config).unwrap();

    // Second run is a no-op success and leaves the file unchanged.
    bin().arg("init").current_dir(dir.path()).assert().success();
    let second = std::fs::read_to_string(&config).unwrap();
    assert_eq!(first, second);
}

#[test]
fn init_does_not_overwrite_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("cratevista.toml");
    std::fs::write(&config, "# user content\n").unwrap();

    bin().arg("init").current_dir(dir.path()).assert().success();
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "# user content\n"
    );

    bin()
        .args(["init", "--force"])
        .current_dir(dir.path())
        .assert()
        .success();
    assert_ne!(
        std::fs::read_to_string(&config).unwrap(),
        "# user content\n"
    );
}

#[test]
fn doctor_succeeds_inside_a_cargo_project() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();

    bin()
        .arg("doctor")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Cargo project detected"));
}

#[test]
fn doctor_is_fatal_outside_a_cargo_project() {
    let dir = tempfile::tempdir().unwrap();
    bin()
        .arg("doctor")
        .current_dir(dir.path())
        .assert()
        .failure()
        .code(3);
}

#[test]
fn json_format_emits_machine_readable_diagnostic() {
    // An invalid `--base-path` is a usage error rendered as JSON on stdout.
    bin()
        .args(["build", "--base-path", "http://x", "--format", "json"])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::contains(
            "\"code\":\"build_invalid_base_path\"",
        ));
}

#[test]
fn serve_without_artifacts_fails_with_exit_3() {
    // A workspace that was never generated: serve fails actionably (exit 3),
    // needs no nightly, and points at `generate`.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"srv\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src").join("main.rs"), "fn main() {}\n").unwrap();

    bin()
        .arg("serve")
        .current_dir(dir.path())
        .assert()
        .failure()
        .code(3)
        .stderr(predicate::str::contains("generate"));
}

// --- `--watch` is an `open` flag, and only an `open` flag (issue 09) --------

#[test]
fn open_help_lists_watch() {
    let assert = bin().args(["open", "--help"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        stdout.contains("--watch"),
        "`open --help` must document --watch\n{stdout}"
    );
}

#[test]
fn serve_help_lists_neither_watch_nor_generation_flags() {
    // `serve` serves an existing snapshot and never regenerates, so a watcher
    // would have nothing to trigger. It shares `ServerArgs` with `open`, which is
    // exactly why `--watch` is declared on the `Open` variant instead.
    let assert = bin().args(["serve", "--help"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        !stdout.contains("--watch"),
        "`serve --help` must not offer --watch\n{stdout}"
    );
    for flag in [
        "--keep-going",
        "--all-features",
        "--document-private-items",
        "--toolchain",
        "--no-config",
    ] {
        assert!(
            !stdout.contains(flag),
            "`serve --help` must not offer the generation flag `{flag}`\n{stdout}"
        );
    }
}

#[test]
fn serve_rejects_watch() {
    bin()
        .args(["serve", "--watch"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--watch"));
}

#[test]
fn open_accepts_watch() {
    // Parsing only: a real `open --watch` would generate and bind. `--help` after
    // the flag proves clap accepted it and stops before anything runs.
    bin().args(["open", "--watch", "--help"]).assert().success();
}

#[test]
fn open_without_watch_still_parses_exactly_as_before() {
    bin().args(["open", "--help"]).assert().success();
    bin()
        .args([
            "open",
            "--port",
            "8080",
            "--source",
            "--all-features",
            "--help",
        ])
        .assert()
        .success();
}

// --- `build` CLI surface (issue 10, Phase 3) --------------------------------

/// Writes a minimal bin-only crate: `cargo metadata` succeeds offline and the
/// default `RustdocPlan` is empty, so `generate` is a metadata-only success that
/// needs **no nightly and no network**.
fn write_bin_crate(dir: &std::path::Path, name: &str) {
    std::fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();
}

// Parser / help ---------------------------------------------------------------

#[test]
fn build_help_no_longer_says_unimplemented_and_documents_defaults() {
    let assert = bin().args(["build", "--help"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        !stdout.to_lowercase().contains("not implemented"),
        "build help must not say unimplemented\n{stdout}"
    );
    assert!(
        stdout.contains("target/cratevista/site"),
        "build help must document the default output\n{stdout}"
    );
    assert!(
        stdout.contains("workspace root"),
        "build help must state relative output is workspace-root-relative\n{stdout}"
    );
}

#[test]
fn build_parses_with_defaults_and_output_and_base_path() {
    // `--help` after the flags proves clap accepted them and stops before work runs.
    for args in [
        vec!["build", "--help"],
        vec!["build", "--output", "dist", "--help"],
        vec!["build", "--output", "a/b/site", "--help"],
        vec!["build", "--base-path", "/demo/", "--help"],
        vec!["build", "--base-path", "repo", "--help"],
    ] {
        bin().args(&args).assert().success();
    }
}

#[test]
fn build_parses_an_absolute_output() {
    let abs = std::env::temp_dir().join("cratevista-cli-abs-site");
    bin()
        .args(["build", "--output"])
        .arg(&abs)
        .arg("--help")
        .assert()
        .success();
}

#[test]
fn build_accepts_the_full_generate_args_surface() {
    bin()
        .args([
            "build",
            "--keep-going",
            "--features",
            "a,b",
            "--all-features",
            "--no-default-features",
            "--document-private-items",
            "--toolchain",
            "nightly-x",
            "--external-deps",
            "full",
            "--document-bins",
            "--no-config",
            "--help",
        ])
        .assert()
        .success();
}

#[test]
fn build_rejects_watch_and_server_only_flags() {
    for flag in [
        vec!["build", "--watch"],
        vec!["build", "--port", "8080"],
        vec!["build", "--host", "127.0.0.1"],
        vec!["build", "--source"],
        vec!["build", "--include-source-snippets"],
    ] {
        bin().args(&flag).assert().failure().code(2).stderr(
            predicate::str::contains("unexpected argument")
                .or(predicate::str::contains("unexpected").or(predicate::str::contains("--"))),
        );
    }
}

// Diagnostics -----------------------------------------------------------------

#[test]
fn build_invalid_base_path_is_usage_error_code_2() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "bp");
    bin()
        .args(["build", "--base-path", "http://evil"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("build_invalid_base_path"));
}

#[test]
fn build_preserves_a_generation_error_code_and_exit() {
    // No Cargo workspace: generation fails first, unchanged (exit 3), and never
    // reaches materialization.
    let dir = tempfile::tempdir().unwrap();
    bin()
        .arg("build")
        .current_dir(dir.path())
        .assert()
        .failure()
        .code(3);
}

#[test]
fn build_maps_a_materialization_runtime_error() {
    // `--output .` resolves to the workspace root itself → refused by output
    // safety with the build_* runtime code (exit 1), after a committed generation.
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "forbid");
    bin()
        .args(["build", "--output", "."])
        .current_dir(dir.path())
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("build_output_forbidden"));
}

#[test]
fn build_never_returns_unimplemented() {
    let dir = tempfile::tempdir().unwrap();
    bin()
        .arg("build")
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not implemented yet").not())
        .stderr(predicate::str::contains("unimplemented").not());
}

// Output resolution (real CLI, metadata-only fixture) -------------------------

#[test]
fn no_output_publishes_to_the_workspace_local_default_from_root() {
    let ws = tempfile::tempdir().unwrap();
    write_bin_crate(ws.path(), "wslocal");
    bin().arg("build").current_dir(ws.path()).assert().success();
    assert!(
        ws.path()
            .join("target/cratevista/site/index.html")
            .is_file(),
        "default output must land under the workspace"
    );
}

#[test]
fn no_output_anchors_to_the_workspace_not_the_external_cwd() {
    let ws = tempfile::tempdir().unwrap();
    let ws_path = ws.path().canonicalize().unwrap();
    write_bin_crate(&ws_path, "wsanchor");
    let external = tempfile::tempdir().unwrap();
    let external_path = external.path().canonicalize().unwrap();

    bin()
        .arg("build")
        .arg("--manifest-path")
        .arg(ws_path.join("Cargo.toml"))
        .current_dir(&external_path)
        .assert()
        .success();

    assert!(
        ws_path.join("target/cratevista/site/index.html").is_file(),
        "site must be under the workspace root"
    );
    assert!(
        !external_path.join("target/cratevista/site").exists(),
        "nothing may be written under the external cwd"
    );
}

#[test]
fn relative_output_from_outside_resolves_against_the_workspace() {
    let ws = tempfile::tempdir().unwrap();
    let ws_path = ws.path().canonicalize().unwrap();
    write_bin_crate(&ws_path, "wsdist");
    let external = tempfile::tempdir().unwrap();
    let external_path = external.path().canonicalize().unwrap();

    bin()
        .args(["build", "--output", "dist"])
        .arg("--manifest-path")
        .arg(ws_path.join("Cargo.toml"))
        .current_dir(&external_path)
        .assert()
        .success();

    assert!(ws_path.join("dist/index.html").is_file());
    assert!(!external_path.join("dist").exists());
}

#[test]
fn absolute_output_is_used_exactly() {
    let ws = tempfile::tempdir().unwrap();
    let ws_path = ws.path().canonicalize().unwrap();
    write_bin_crate(&ws_path, "wsabs");
    let target = tempfile::tempdir().unwrap();
    let site = target.path().canonicalize().unwrap().join("exact-site");

    bin()
        .args(["build", "--output"])
        .arg(&site)
        .arg("--manifest-path")
        .arg(ws_path.join("Cargo.toml"))
        .current_dir(&ws_path)
        .assert()
        .success();

    assert!(site.join("index.html").is_file());
    assert!(!ws_path.join("target/cratevista/site").exists());
}

#[test]
fn a_generation_failure_creates_no_output_anywhere() {
    // Point at a non-workspace directory: generation fails, so neither the default
    // workspace output nor anything under the external cwd is created.
    let external = tempfile::tempdir().unwrap();
    bin()
        .arg("build")
        .current_dir(external.path())
        .assert()
        .failure();
    assert!(!external.path().join("target/cratevista/site").exists());
    assert!(!external.path().join("target").exists());
}

// The stable, non-ignored metadata-only end-to-end CLI proof --------------------

#[test]
fn build_cli_materializes_a_metadata_only_site() {
    let ws = tempfile::tempdir().unwrap();
    write_bin_crate(ws.path(), "metaonly");

    bin()
        .args(["build", "--output", "site"])
        .current_dir(ws.path())
        .assert()
        .success();

    let site = ws.path().join("site");
    for name in [
        "index.html",
        "document.json",
        "generation.json",
        "diagnostics.json",
        ".cratevista-static-site.json",
    ] {
        assert!(site.join(name).is_file(), "{name} must exist");
    }

    // Marker C: a site marker with no output_key.
    let marker = std::fs::read_to_string(site.join(".cratevista-static-site.json")).unwrap();
    assert!(marker.contains("\"kind\":\"site\""), "{marker}");
    assert!(
        !marker.contains("output_key"),
        "marker C omits the key: {marker}"
    );

    // The static-mode meta is present exactly once.
    let index = std::fs::read_to_string(site.join("index.html")).unwrap();
    assert_eq!(index.matches("cratevista-mode").count(), 1, "{index}");

    // At least one embedded fingerprinted asset, and no /api or source snippets.
    let mut has_asset_js = false;
    let mut walked = Vec::new();
    fn walk(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
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
    walk(&site, &site, &mut walked);
    for rel in &walked {
        if rel.starts_with("assets/") && rel.ends_with(".js") {
            has_asset_js = true;
        }
        assert!(!rel.contains("api"), "no /api files: {rel}");
        assert!(!rel.starts_with("source/"), "no source snippets: {rel}");
    }
    assert!(
        has_asset_js,
        "a bundled script asset must exist: {walked:?}"
    );
}
