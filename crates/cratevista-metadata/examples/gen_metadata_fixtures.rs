//! Generates checked-in `cargo metadata` fixtures under
//! `crates/cratevista-metadata/fixtures/` from real (path-only, offline)
//! workspaces, sanitizing all machine paths to a stable `/w` root so the
//! fixtures are portable and hermetic.
//!
//! Run with: `cargo run -p cratevista-metadata --example gen_metadata_fixtures`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn write_file(base: &Path, rel: &str, contents: &str) {
    let path = base.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Runs `cargo metadata` on `<base>/ws/Cargo.toml`, sanitizes paths, and writes
/// the fixture.
fn capture(base: &Path, fixture_name: &str) {
    let manifest = base.join("ws").join("Cargo.toml");
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed for {fixture_name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let sanitized = sanitize(&stdout);

    let mut out = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    out.push("fixtures");
    std::fs::create_dir_all(&out).unwrap();
    out.push(format!("{fixture_name}.metadata.json"));
    std::fs::write(&out, sanitized).unwrap();
    println!("wrote {}", out.display());
}

/// Replaces the tempdir base (the parent of `workspace_root`) with `/w` in every
/// JSON string, and normalizes separators to forward slashes.
fn sanitize(stdout: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(stdout).expect("valid metadata json");
    let root = value["workspace_root"]
        .as_str()
        .expect("workspace_root")
        .replace('\\', "/");
    let base = root
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or(root);
    walk(&mut value, &base);
    let mut text = serde_json::to_string_pretty(&value).unwrap();
    text.push('\n');
    text
}

fn walk(value: &mut serde_json::Value, base_fwd: &str) {
    match value {
        serde_json::Value::String(s) => {
            *s = s.replace('\\', "/").replace(base_fwd, "/w");
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(|v| walk(v, base_fwd)),
        serde_json::Value::Object(map) => map.values_mut().for_each(|v| walk(v, base_fwd)),
        _ => {}
    }
}

fn simple_lib(base: &Path, dir: &str, name: &str) {
    write_file(
        base,
        &format!("ws/{dir}/Cargo.toml"),
        &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    );
    write_file(base, &format!("ws/{dir}/src/lib.rs"), "pub fn x() {}\n");
}

fn gen_single_package(base: &Path) {
    write_file(
        base,
        "ws/Cargo.toml",
        "[package]\nname = \"solo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write_file(base, "ws/src/lib.rs", "pub fn solo() {}\n");
    capture(base, "single_package");
}

fn gen_workspace_deps(base: &Path) {
    write_file(
        base,
        "ws/Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"app\", \"core\", \"core2\", \"optdep\", \"builddep\", \"devdep\", \"mac\", \"plat\"]\n",
    );
    // app with many target kinds and dependency kinds.
    write_file(
        base,
        "ws/app/Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n\n\
         [dependencies]\ncore = { path = \"../core\" }\noptdep = { path = \"../optdep\", optional = true }\n\
         mac = { path = \"../mac\" }\nren = { path = \"../core2\", package = \"core2\" }\n\n\
         [build-dependencies]\nbuilddep = { path = \"../builddep\" }\n\n\
         [dev-dependencies]\ndevdep = { path = \"../devdep\" }\n\n\
         [target.'cfg(windows)'.dependencies]\nplat = { path = \"../plat\" }\n\n\
         [features]\ndefault = [\"extra\"]\nextra = [\"optdep\"]\n",
    );
    write_file(base, "ws/app/src/lib.rs", "pub fn app() {}\n");
    write_file(base, "ws/app/src/main.rs", "fn main() {}\n");
    write_file(base, "ws/app/build.rs", "fn main() {}\n");
    write_file(base, "ws/app/examples/demo.rs", "fn main() {}\n");
    write_file(base, "ws/app/tests/it.rs", "#[test]\nfn t() {}\n");
    write_file(base, "ws/app/benches/b.rs", "fn main() {}\n");

    for (dir, name) in [
        ("core", "core"),
        ("core2", "core2"),
        ("optdep", "optdep"),
        ("builddep", "builddep"),
        ("devdep", "devdep"),
        ("plat", "plat"),
    ] {
        simple_lib(base, dir, name);
    }
    // proc-macro crate.
    write_file(
        base,
        "ws/mac/Cargo.toml",
        "[package]\nname = \"mac\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\nproc-macro = true\n",
    );
    write_file(base, "ws/mac/src/lib.rs", "pub fn m() {}\n");

    capture(base, "workspace_deps");
}

fn gen_external_path(base: &Path) {
    write_file(
        base,
        "ws/Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"app\"]\n",
    );
    write_file(
        base,
        "ws/app/Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\next = { path = \"../../ext\" }\n",
    );
    write_file(base, "ws/app/src/lib.rs", "pub fn app() {}\n");
    // `ext` lives outside the workspace root (sibling of `ws`).
    write_file(
        base,
        "ext/Cargo.toml",
        "[package]\nname = \"ext\"\nversion = \"2.0.0\"\nedition = \"2021\"\n",
    );
    write_file(base, "ext/src/lib.rs", "pub fn ext() {}\n");

    capture(base, "external_path");
}

fn main() {
    for generator in [
        gen_single_package as fn(&Path),
        gen_workspace_deps,
        gen_external_path,
    ] {
        let dir = tempfile::tempdir().unwrap();
        generator(dir.path());
    }
}
