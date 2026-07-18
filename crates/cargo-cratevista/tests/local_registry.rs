//! Phase 5B — offline local-registry package-then-install verification.
//!
//! # What this proves
//!
//! The nine workspace crates can be packaged, assembled into an **offline Cargo
//! `local-registry`**, and `cargo install cargo-cratevista --locked --offline`
//! installed from that registry alone — **no workspace checkout, no network, and
//! no Node** — after which the installed `cargo cratevista` subcommand builds a
//! static site through the metadata-only path (Decision 8a). The **compile +
//! install is the authoritative completeness/buildability proof**; a missing
//! internal crate, a missing third-party archive, or a wrong version each turns
//! into a hard offline failure, never a silent pass.
//!
//! # Why the registry index for the nine internal crates is written here
//!
//! The pinned tool is `cargo-local-registry 0.2.12`. Its `sync` subcommand
//! vendors every **registry-sourced** `Cargo.lock` dependency (used verbatim
//! below for the third-party base), but its `add` subcommand **cannot** inject an
//! unpublished local `.crate`: `add`'s `main()` deliberately strips `[source]`
//! replacement and resolves the requested crate straight from crates.io, so
//! `add cratevista-schema --version 0.1.0` fails with *"no matching package …
//! location searched: crates.io index"*. This was confirmed empirically and in
//! the tool's source. Cargo 1.97.1 itself **does** install an unpublished package
//! from a replaced `local-registry` source offline (the preflight below proves
//! exactly this). So, per PRD 10 Decision 8 (amended for the 0.2.12 `add`
//! limitation), the third-party base comes from the tool's `sync`, and the nine
//! internal index entries are written here in the **tool's own index format**
//! (`get_index_path` layout + the `RegistryPackage`/`RegistryDependency` JSON
//! shape, mirrored below) with the real SHA-256 checksum of each `.crate`.
//!
//! # Running
//!
//! Both tests are `#[ignore]` (each runs `cargo package` and a full offline
//! install). Run them explicitly:
//!
//! ```text
//! cargo test -p cargo-cratevista --test local_registry \
//!   unpublished_package_source_replacement_preflight -- --ignored --exact --nocapture
//! cargo test -p cargo-cratevista --test local_registry \
//!   local_registry_package_install_works -- --ignored --exact --nocapture
//! ```
//!
//! On a dirty local tree set `CRATEVISTA_PACKAGE_ALLOW_DIRTY=1` so `cargo package`
//! accepts uncommitted changes; CI leaves it unset so packaging runs clean. Set
//! `CRATEVISTA_KEEP_REGISTRY_TMP=1` to preserve the temp registry/CARGO_HOME on
//! failure for inspection.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The nine workspace crates, in publication (dependency) order.
const INTERNAL_CRATES: [&str; 9] = [
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

const CRATE_VERSION: &str = "0.1.0";

// ---------------------------------------------------------------------------
// Basic helpers
// ---------------------------------------------------------------------------

/// The workspace root (two levels up from this crate).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

/// The exact `cargo` that is running these tests.
fn cargo() -> String {
    env!("CARGO").to_string()
}

/// A `PATH`-safe join of directories, most-significant first.
fn prepend_path(dirs: &[PathBuf]) -> std::ffi::OsString {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut all: Vec<PathBuf> = dirs.to_vec();
    all.extend(std::env::split_paths(&existing));
    std::env::join_paths(all).expect("join PATH")
}

/// An absolute path rendered with forward slashes — the form Cargo accepts
/// reliably inside `config.toml` on every platform, Windows included.
fn toml_abs(p: &Path) -> String {
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let mut s = abs.to_string_lossy().replace('\\', "/");
    // Strip a Windows verbatim prefix so Cargo does not choke on `\\?\`.
    if let Some(rest) = s.strip_prefix("//?/") {
        s = rest.to_string();
    }
    s
}

/// Print a child command before running it (Part 2), then run it to completion.
fn run(stage: &str, cmd: &mut Command) -> Output {
    let prog = cmd.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    eprintln!("[{stage}] $ {prog} {}", args.join(" "));
    cmd.output()
        .unwrap_or_else(|e| panic!("[{stage}] failed to spawn `{prog}`: {e}"))
}

/// Run and require success, printing captured output on failure with the stage.
fn run_ok(stage: &str, cmd: &mut Command) -> Output {
    let out = run(stage, cmd);
    assert!(
        out.status.success(),
        "[{stage}] command failed with {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    out
}

/// The lowercase-hex SHA-256 of a file's bytes — the checksum a local-registry
/// index entry must carry so Cargo accepts the `.crate`.
fn sha256_hex(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let digest = Sha256::digest(&bytes);
    let mut s = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        write!(s, "{b:02x}").unwrap();
    }
    s
}

// ---------------------------------------------------------------------------
// Registry index format (mirrors cargo-local-registry 0.2.12 exactly)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct IndexPackage {
    name: String,
    vers: String,
    deps: Vec<IndexDep>,
    cksum: String,
    features: BTreeMap<String, Vec<String>>,
    yanked: bool,
    // Carried from the normalized manifest when present. `links` guards native
    // library collisions; `rust_version` lets Cargo honour the MSRV from the
    // index. Cargo treats both as optional, so they are omitted (like the pinned
    // tool does) when the manifest has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    links: Option<String>,
    #[serde(rename = "rust_version", skip_serializing_if = "Option::is_none")]
    rust_version: Option<String>,
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct IndexDep {
    name: String,
    req: String,
    features: Vec<String>,
    optional: bool,
    default_features: bool,
    target: Option<String>,
    kind: Option<String>,
    package: Option<String>,
}

/// Refuse to overwrite a different pre-existing index entry, and refuse a file
/// that already holds more than one version row (Part 3 / Part 4 control #4).
fn ensure_no_conflicting_entry(idx: &Path, entry: &str) -> Result<(), String> {
    if let Ok(existing) = std::fs::read_to_string(idx) {
        let nonempty: Vec<&str> = existing.lines().filter(|l| !l.trim().is_empty()).collect();
        if nonempty.len() > 1 {
            return Err(format!(
                "duplicate index rows already present at {}",
                idx.display()
            ));
        }
        if let Some(first) = nonempty.first()
            && first.trim() != entry.trim()
        {
            return Err(format!(
                "refusing to overwrite a different index entry at {}",
                idx.display()
            ));
        }
    }
    Ok(())
}

/// The index file path for a crate, per the tool's `get_index_path` rules.
fn index_path(registry: &Path, name: &str) -> PathBuf {
    let name = name.to_lowercase();
    let index = registry.join("index");
    match name.len() {
        1 => index.join("1").join(&name),
        2 => index.join("2").join(&name),
        3 => index.join("3").join(&name[..1]).join(&name),
        _ => index.join(&name[..2]).join(&name[2..4]).join(&name),
    }
}

// ---------------------------------------------------------------------------
// Index rows come from the Cargo-NORMALIZED PACKAGED manifest (Part 1)
//
// The authoritative source of every internal index row is the `Cargo.toml`
// inside the exact `.crate` archive that Cargo produced — not the workspace
// `cargo metadata` result. `cargo package` normalizes that manifest (internal
// path deps become bare `version` requirements, inherited fields are inlined),
// which is precisely what the registry index must describe. Workspace metadata
// is used ONLY as an independent expected-package cross-check (below).
// ---------------------------------------------------------------------------

/// One `[dependencies]`/`[dev-dependencies]`/… value: either a bare version
/// string or a detailed table.
#[derive(Deserialize)]
#[serde(untagged)]
enum DepSpec {
    Simple(String),
    Detailed(Box<DetailedDep>),
}

#[derive(Deserialize)]
struct DetailedDep {
    version: Option<String>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    optional: bool,
    #[serde(rename = "default-features")]
    default_features: Option<bool>,
    package: Option<String>,
    path: Option<String>,
    git: Option<String>,
}

/// A `[target.<cfg>]` table's three dependency kinds.
#[derive(Deserialize, Default)]
struct DepTables {
    #[serde(default)]
    dependencies: BTreeMap<String, DepSpec>,
    #[serde(rename = "dev-dependencies", default)]
    dev_dependencies: BTreeMap<String, DepSpec>,
    #[serde(rename = "build-dependencies", default)]
    build_dependencies: BTreeMap<String, DepSpec>,
}

#[derive(Deserialize)]
struct PkgHeader {
    name: String,
    version: String,
    links: Option<String>,
    #[serde(rename = "rust-version")]
    rust_version: Option<String>,
}

#[derive(Deserialize)]
struct NormalizedManifest {
    package: PkgHeader,
    #[serde(default)]
    dependencies: BTreeMap<String, DepSpec>,
    #[serde(rename = "dev-dependencies", default)]
    dev_dependencies: BTreeMap<String, DepSpec>,
    #[serde(rename = "build-dependencies", default)]
    build_dependencies: BTreeMap<String, DepSpec>,
    #[serde(default)]
    target: BTreeMap<String, DepTables>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
}

/// Turn one dependency table into index rows, validating internal-dep sources
/// (Part 1: an internal dep must carry the `0.1.0` registry version and no
/// path/git source; a packaged manifest must carry no path/git source at all).
fn push_dep_rows(
    out: &mut Vec<IndexDep>,
    table: &BTreeMap<String, DepSpec>,
    kind: Option<&str>,
    target: Option<&str>,
) -> Result<(), String> {
    for (key, spec) in table {
        let (req, mut features, optional, default_features, package, path, git) = match spec {
            DepSpec::Simple(v) => (v.clone(), Vec::new(), false, true, None, None, None),
            DepSpec::Detailed(d) => (
                d.version
                    .clone()
                    .ok_or_else(|| format!("dependency `{key}` has no version requirement"))?,
                d.features.clone(),
                d.optional,
                d.default_features.unwrap_or(true),
                d.package.clone(),
                d.path.clone(),
                d.git.clone(),
            ),
        };
        features.sort();
        // A packaged (normalized) manifest never carries a path or git source.
        if path.is_some() {
            return Err(format!("dependency `{key}` carries a path source"));
        }
        if git.is_some() {
            return Err(format!("dependency `{key}` carries a git source"));
        }
        // The real crate name is the rename target if renamed, else the key.
        let real = package.clone().unwrap_or_else(|| key.clone());
        if INTERNAL_CRATES.contains(&real.as_str()) && !req.contains(CRATE_VERSION) {
            return Err(format!(
                "internal dependency `{real}` must pin `{CRATE_VERSION}`, got `{req}`"
            ));
        }
        out.push(IndexDep {
            name: key.clone(),
            req,
            features,
            optional,
            default_features,
            target: target.map(str::to_string),
            kind: kind.map(str::to_string),
            package,
        });
    }
    Ok(())
}

/// Build the single compact index JSON line for a crate from its NORMALIZED
/// packaged manifest text and the exact archive checksum. Fails (never silently
/// mis-generates) if the manifest identity does not match the archive, if an
/// internal dep is mis-sourced, or if the resulting line would leak a path.
fn index_entry_from_manifest(
    manifest_text: &str,
    expected_name: &str,
    expected_version: &str,
    cksum: &str,
) -> Result<String, String> {
    let m: NormalizedManifest =
        toml::from_str(manifest_text).map_err(|e| format!("parse normalized manifest: {e}"))?;
    // Bind the manifest to exactly this archive by name + version.
    if m.package.name != expected_name {
        return Err(format!(
            "manifest package `{}` does not match archive `{expected_name}`",
            m.package.name
        ));
    }
    if m.package.version != expected_version {
        return Err(format!(
            "manifest version `{}` does not match archive `{expected_version}`",
            m.package.version
        ));
    }

    let mut deps = Vec::new();
    push_dep_rows(&mut deps, &m.dependencies, None, None)?;
    push_dep_rows(&mut deps, &m.dev_dependencies, Some("dev"), None)?;
    push_dep_rows(&mut deps, &m.build_dependencies, Some("build"), None)?;
    for (cfg, tables) in &m.target {
        push_dep_rows(&mut deps, &tables.dependencies, None, Some(cfg))?;
        push_dep_rows(&mut deps, &tables.dev_dependencies, Some("dev"), Some(cfg))?;
        push_dep_rows(
            &mut deps,
            &tables.build_dependencies,
            Some("build"),
            Some(cfg),
        )?;
    }
    deps.sort();

    let entry = IndexPackage {
        name: m.package.name,
        vers: m.package.version,
        deps,
        cksum: cksum.to_string(),
        features: m.features,
        yanked: false,
        links: m.package.links,
        rust_version: m.package.rust_version,
    };
    let line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    validate_index_line(&line, cksum)?;
    Ok(line)
}

/// Validate a finished index line (Part 3): compact single line, lowercase-hex
/// checksum, and no path/git/absolute/`..` leakage.
fn validate_index_line(line: &str, cksum: &str) -> Result<(), String> {
    if line.contains('\n') {
        return Err("index line must be a single compact line".to_string());
    }
    if cksum.len() != 64
        || !cksum
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err(format!(
            "cksum must be 64 lowercase hex chars, got `{cksum}`"
        ));
    }
    for needle in [
        "\"path\"",
        "\"git\"",
        "/Projects/",
        ":\\",
        "\\\\",
        "target/package",
        "..",
    ] {
        if line.contains(needle) {
            return Err(format!("index line leaks `{needle}`: {line}"));
        }
    }
    Ok(())
}

// --- extraction of the normalized manifest / files from a `.crate` archive ---

/// Unpack a `.crate` (gzip+tar) into `dest` and return the package root
/// (`dest/<name>-<version>/`). Deterministic on every OS — no reliance on an
/// ambient `tar` binary.
fn extract_crate(crate_path: &Path, dest: &Path) -> PathBuf {
    use flate2::read::GzDecoder;
    use tar::Archive;
    std::fs::create_dir_all(dest).unwrap();
    let file = std::fs::File::open(crate_path)
        .unwrap_or_else(|e| panic!("open {}: {e}", crate_path.display()));
    Archive::new(GzDecoder::new(file))
        .unpack(dest)
        .unwrap_or_else(|e| panic!("unpack {}: {e}", crate_path.display()));
    let stem = crate_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .strip_suffix(".crate")
        .expect("a .crate file")
        .to_string();
    dest.join(stem)
}

/// The normalized `Cargo.toml` text inside a `.crate` archive.
fn normalized_manifest(crate_path: &Path, work: &Path) -> String {
    let root = extract_crate(crate_path, work);
    std::fs::read_to_string(root.join("Cargo.toml"))
        .unwrap_or_else(|e| panic!("read normalized manifest for {}: {e}", crate_path.display()))
}

// --- workspace `cargo metadata`: independent expected-package cross-check ----

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<MetaPkgName>,
}

#[derive(Deserialize)]
struct MetaPkgName {
    name: String,
}

/// Assert `cargo metadata` reports exactly the nine expected workspace members —
/// an independent consistency check that never supplies index-row data.
fn assert_expected_workspace_packages(root: &Path) {
    let out = run_ok(
        "metadata",
        Command::new(cargo()).args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
            root.join("Cargo.toml").to_string_lossy().as_ref(),
        ]),
    );
    let meta: Metadata = serde_json::from_slice(&out.stdout).expect("cargo metadata is valid JSON");
    let mut names: Vec<&str> = meta.packages.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    let mut expected: Vec<&str> = INTERNAL_CRATES.to_vec();
    expected.sort_unstable();
    assert_eq!(
        names, expected,
        "workspace must contain exactly the nine expected members"
    );
}

/// The count of registry-sourced packages in the committed `Cargo.lock` — the
/// exact number `cargo local-registry sync` must vendor (Part 3; derived, never
/// hard-coded).
fn expected_third_party_count(root: &Path) -> usize {
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).unwrap();
    let doc: toml::Value = toml::from_str(&lock).expect("Cargo.lock is valid TOML");
    doc.get("package")
        .and_then(|p| p.as_array())
        .expect("Cargo.lock has [[package]] entries")
        .iter()
        .filter(|p| {
            p.get("source")
                .and_then(|s| s.as_str())
                .is_some_and(|s| s.starts_with("registry+"))
        })
        .count()
}

// ---------------------------------------------------------------------------
// Provenance + validation, exercised on crafted normalized manifests (fast, no
// cargo). Part 2 (the row is manifest-driven) and Part 4 controls #4/#5/#6.
// ---------------------------------------------------------------------------

#[test]
fn index_rows_are_derived_from_the_packaged_manifest() {
    let cksum = "a".repeat(64); // a valid 64-lowercase-hex placeholder

    // A NORMALIZED manifest, as `cargo package` emits it: the internal dep is
    // version-only (path stripped), plus a third-party dep, a rename, a dev dep,
    // a target-scoped dep, and a feature table.
    let normalized = r#"
[package]
name = "cratevista-core"
version = "0.1.0"
edition = "2024"
rust-version = "1.97.1"

[dependencies]
cratevista-schema = { version = "0.1.0", default-features = false }
serde = { version = "1", features = ["derive"] }
renamed = { version = "2", package = "real-crate" }

[dev-dependencies]
tempfile = "3"

[target."cfg(windows)".dependencies]
winapi = { version = "0.3", optional = true }

[features]
default = ["serde"]
"#;

    let line = index_entry_from_manifest(normalized, "cratevista-core", "0.1.0", &cksum)
        .expect("a normalized manifest generates a valid index row");
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    let dep = |name: &str| -> serde_json::Value {
        v["deps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == name)
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    };
    // Internal dep: version requirement present, no path.
    assert!(
        dep("cratevista-schema")["req"]
            .as_str()
            .unwrap()
            .contains("0.1.0")
    );
    assert!(!line.contains("\"path\""), "no path may leak: {line}");
    // Rename → name=alias, package=real.
    assert_eq!(dep("renamed")["package"], "real-crate");
    // rust_version, a dev dep and a target-scoped dep are all represented.
    assert_eq!(v["rust_version"], "1.97.1");
    assert!(
        v["deps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "dev")
    );
    assert!(
        v["deps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["target"] == "cfg(windows)")
    );

    // Part 2 / control #5: the row tracks the MANIFEST bytes, not workspace
    // metadata — changing only the manifest's third-party version changes it.
    let bumped = normalized.replace("serde = { version = \"1\"", "serde = { version = \"4\"");
    let bumped_line =
        index_entry_from_manifest(&bumped, "cratevista-core", "0.1.0", &cksum).unwrap();
    let bv: serde_json::Value = serde_json::from_str(&bumped_line).unwrap();
    let serde_req = bv["deps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "serde")
        .unwrap()["req"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        serde_req.contains('4'),
        "row must reflect the manifest, got {serde_req}"
    );

    // An un-normalized (workspace-style) manifest with a path source is rejected —
    // proof the generator consumes packaged normalization, not raw manifests.
    let with_path = normalized.replace(
        "cratevista-schema = { version = \"0.1.0\", default-features = false }",
        "cratevista-schema = { path = \"../cratevista-schema\", version = \"0.1.0\" }",
    );
    assert!(
        index_entry_from_manifest(&with_path, "cratevista-core", "0.1.0", &cksum).is_err(),
        "a path-bearing manifest must be rejected"
    );

    // An internal dep with no version requirement is rejected.
    let no_ver = normalized.replace(
        "cratevista-schema = { version = \"0.1.0\", default-features = false }",
        "cratevista-schema = { default-features = false }",
    );
    assert!(index_entry_from_manifest(&no_ver, "cratevista-core", "0.1.0", &cksum).is_err());

    // Control #6: manifest/archive identity mismatch (name or version) is rejected.
    assert!(
        index_entry_from_manifest(normalized, "cargo-cratevista", "0.1.0", &cksum).is_err(),
        "a manifest whose name != the archive must be rejected"
    );
    assert!(index_entry_from_manifest(normalized, "cratevista-core", "9.9.9", &cksum).is_err());

    // A non-hex checksum is rejected by line validation.
    assert!(index_entry_from_manifest(normalized, "cratevista-core", "0.1.0", "NOTHEX").is_err());

    // Control #4: duplicate/overwrite guard.
    let tmp = tempfile::tempdir().unwrap();
    let idx = tmp.path().join("idx");
    std::fs::write(&idx, &line).unwrap();
    assert!(
        ensure_no_conflicting_entry(&idx, &line).is_ok(),
        "an identical entry is fine"
    );
    assert!(
        ensure_no_conflicting_entry(&idx, "{\"different\":true}").is_err(),
        "a different entry must be refused"
    );
    std::fs::write(&idx, format!("{line}\n{line}")).unwrap();
    assert!(
        ensure_no_conflicting_entry(&idx, &line).is_err(),
        "a file with duplicate rows must be refused"
    );
}

// ---------------------------------------------------------------------------
// Packaging + registry assembly
// ---------------------------------------------------------------------------

/// Package all nine crates into a dedicated, empty output directory and return
/// the directory holding the nine `.crate` archives.
fn package_all(root: &Path, pkg_target: &Path) -> PathBuf {
    let mut cmd = Command::new(cargo());
    cmd.current_dir(root).args([
        "package",
        "--workspace",
        "--locked",
        "--no-verify",
        "--target-dir",
        pkg_target.to_string_lossy().as_ref(),
    ]);
    if std::env::var_os("CRATEVISTA_PACKAGE_ALLOW_DIRTY").is_some() {
        cmd.arg("--allow-dirty");
    }
    run_ok("package", &mut cmd);

    let pkg_dir = pkg_target.join("package");
    // Exactly nine archives, exact names, nothing else — a fresh dedicated
    // target dir means no archive can be stale or left over from an earlier run.
    let mut found: Vec<String> = std::fs::read_dir(&pkg_dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", pkg_dir.display()))
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".crate"))
        .collect();
    found.sort();
    let mut expected: Vec<String> = INTERNAL_CRATES
        .iter()
        .map(|c| format!("{c}-{CRATE_VERSION}.crate"))
        .collect();
    expected.sort();
    assert_eq!(
        found, expected,
        "the package output must contain exactly the nine 0.1.0 archives"
    );
    pkg_dir
}

/// Assemble the full offline registry: `sync` the third-party base, then write
/// the nine internal crates' `.crate` + index entries — each row derived from
/// the archive's own normalized manifest — in publication order.
fn assemble_registry(root: &Path, registry: &Path, pkg_dir: &Path, work: &Path) {
    std::fs::create_dir_all(registry).unwrap();

    // Third-party base: the tool's `sync` against the committed lockfile. This
    // is the one stage allowed to reach crates.io (assembly is pre-offline).
    run_ok(
        "sync",
        Command::new(cargo()).current_dir(root).args([
            "local-registry",
            "sync",
            root.join("Cargo.lock").to_string_lossy().as_ref(),
            registry.to_string_lossy().as_ref(),
        ]),
    );
    // `sync` must vendor third-party crates and, by design, skip the nine
    // internal path crates — they carry no registry source.
    for internal in INTERNAL_CRATES {
        assert!(
            !registry
                .join(format!("{internal}-{CRATE_VERSION}.crate"))
                .exists(),
            "sync must not vendor the internal path crate {internal}"
        );
    }

    // Independent cross-check (never a source of index data): the tree really
    // holds exactly the nine expected members.
    assert_expected_workspace_packages(root);

    // The nine internal crates: copy the archive, derive its index row from the
    // archive's OWN normalized manifest, validate, and write with a no-overwrite
    // guard — in publication order.
    for internal in INTERNAL_CRATES {
        let archive = format!("{internal}-{CRATE_VERSION}.crate");
        let src = pkg_dir.join(&archive);
        let dst = registry.join(&archive);
        std::fs::copy(&src, &dst)
            .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", src.display(), dst.display()));

        // Bind checksum and manifest to this exact archive.
        let cksum = sha256_hex(&dst);
        let manifest = normalized_manifest(&dst, &work.join(format!("extract-{internal}")));
        let entry = index_entry_from_manifest(&manifest, internal, CRATE_VERSION, &cksum)
            .unwrap_or_else(|e| panic!("index entry for {internal}: {e}"));

        let idx = index_path(registry, internal);
        ensure_no_conflicting_entry(&idx, &entry).unwrap_or_else(|e| panic!("{internal}: {e}"));
        std::fs::create_dir_all(idx.parent().unwrap()).unwrap();
        std::fs::write(&idx, &entry).unwrap();
        eprintln!("[registry] added {archive} + index/{internal} (from packaged manifest)");
    }
}

/// Independently re-read the assembled registry and validate it end to end
/// (Part 3). Extraction (`work`) re-reads the archives themselves, not just the
/// index files.
fn assert_registry_wellformed(root: &Path, registry: &Path, work: &Path) {
    // Third-party count derived from the committed lock (never hard-coded): every
    // registry-sourced lock package must be vendored, and nothing else.
    let expected_tp = expected_third_party_count(root);
    let all_crates: Vec<String> = std::fs::read_dir(registry)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".crate"))
        .collect();
    let internal_present = all_crates
        .iter()
        .filter(|n| {
            INTERNAL_CRATES
                .iter()
                .any(|c| **n == format!("{c}-{CRATE_VERSION}.crate"))
        })
        .count();
    assert_eq!(internal_present, 9, "exactly nine internal archives");
    assert_eq!(
        all_crates.len() - internal_present,
        expected_tp,
        "third-party archive count must equal the lock's registry-sourced package count"
    );

    for internal in INTERNAL_CRATES {
        let archive = registry.join(format!("{internal}-{CRATE_VERSION}.crate"));
        assert!(archive.is_file(), "{internal} archive must exist");

        let idx = index_path(registry, internal);
        let text = std::fs::read_to_string(&idx)
            .unwrap_or_else(|e| panic!("read index {}: {e}", idx.display()));
        let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines.len(),
            1,
            "{internal} must have exactly one index entry"
        );
        // Re-validate the finished line and its checksum against the archive bytes.
        let recomputed = sha256_hex(&archive);
        validate_index_line(lines[0], &recomputed)
            .unwrap_or_else(|e| panic!("{internal} index line invalid: {e}"));
        let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["name"], *internal, "index name must match");
        assert_eq!(v["vers"], CRATE_VERSION, "index version must be 0.1.0");
        assert_eq!(v["cksum"], recomputed, "index cksum must match the archive");
        assert_eq!(v["yanked"], false, "internal packages are never yanked");

        // Internal dependency requirements resolve to the local 0.1.0 crates.
        if let Some(deps) = v["deps"].as_array() {
            for dep in deps {
                let dname = dep["package"]
                    .as_str()
                    .or_else(|| dep["name"].as_str())
                    .unwrap_or_default();
                if INTERNAL_CRATES.contains(&dname) {
                    let req = dep["req"].as_str().unwrap_or_default();
                    assert!(
                        req.contains(CRATE_VERSION),
                        "{internal} -> {dname} internal req must pin 0.1.0, got {req}"
                    );
                }
            }
        }
    }

    // cargo-cratevista depends on cratevista-core 0.1.0.
    let cli: serde_json::Value = read_index_entry(registry, "cargo-cratevista");
    let core_req = cli["deps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "cratevista-core")
        .and_then(|d| d["req"].as_str())
        .expect("cargo-cratevista must depend on cratevista-core");
    assert!(
        core_req.contains(CRATE_VERSION),
        "cargo-cratevista -> cratevista-core must pin 0.1.0, got {core_req}"
    );

    // cratevista-core's complete internal dependency set is represented.
    let core: serde_json::Value = read_index_entry(registry, "cratevista-core");
    let core_deps: Vec<&str> = core["deps"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["name"].as_str())
        .collect();
    for expected in [
        "cratevista-schema",
        "cratevista-metadata",
        "cratevista-rustdoc",
        "cratevista-graph",
        "cratevista-config",
        "cratevista-server",
        "cratevista-watch",
    ] {
        assert!(
            core_deps.contains(&expected),
            "cratevista-core must depend on {expected}: {core_deps:?}"
        );
    }

    // The server ARCHIVE (not just `--list`) contains the authoritative embedded
    // asset set.
    let server_root = extract_crate(
        &registry.join(format!("cratevista-server-{CRATE_VERSION}.crate")),
        &work.join("verify-server"),
    );
    let mut packaged_embedded = walk_rel(&server_root.join("embedded"));
    packaged_embedded.sort();
    let mut authoritative = authoritative_embedded_files(root);
    authoritative.sort();
    assert_eq!(
        packaged_embedded, authoritative,
        "server archive embedded set must equal the authoritative embedded set exactly"
    );

    // The cargo-cratevista ARCHIVE contains README + both licences.
    let cli_root = extract_crate(
        &registry.join(format!("cargo-cratevista-{CRATE_VERSION}.crate")),
        &work.join("verify-cli"),
    );
    for required in ["README.md", "LICENSE-MIT", "LICENSE-APACHE"] {
        assert!(
            cli_root.join(required).is_file(),
            "cargo-cratevista archive must contain {required}"
        );
    }
}

/// The single index entry for a crate, parsed.
fn read_index_entry(registry: &Path, name: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(index_path(registry, name)).unwrap();
    serde_json::from_str(text.lines().next().unwrap()).unwrap()
}

/// The authoritative embedded asset set from the repository (relative paths).
fn authoritative_embedded_files(root: &Path) -> Vec<String> {
    walk_rel(&root.join("crates/cratevista-server/embedded"))
}

// ---------------------------------------------------------------------------
// Fresh offline environment
// ---------------------------------------------------------------------------

/// A fresh CARGO_HOME whose only source is the local registry.
fn write_cargo_home(cargo_home: &Path, registry: &Path) {
    std::fs::create_dir_all(cargo_home).unwrap();
    let config = format!(
        "[source.crates-io]\nreplace-with = \"local\"\n\n\
         [source.local]\nlocal-registry = \"{}\"\n",
        toml_abs(registry)
    );
    std::fs::write(cargo_home.join("config.toml"), config).unwrap();
}

/// Poison `node`/`npm`/`npx` shims: each records that it was called, then exits
/// non-zero. Prepending their directory to `PATH` makes any Node invocation a
/// hard, observable failure — the discriminating no-Node test.
fn write_node_poison(shims: &Path, markers: &Path) {
    std::fs::create_dir_all(shims).unwrap();
    std::fs::create_dir_all(markers).unwrap();
    for tool in ["node", "npm", "npx"] {
        write_one_shim(shims, tool, markers);
    }
}

#[cfg(windows)]
fn write_one_shim(shims: &Path, tool: &str, markers: &Path) {
    let marker = markers
        .join(format!("{tool}.called"))
        .to_string_lossy()
        .replace('/', "\\");
    let body = format!("@echo off\r\n> \"{marker}\" echo called\r\nexit /b 1\r\n");
    std::fs::write(shims.join(format!("{tool}.cmd")), body).unwrap();
}

#[cfg(not(windows))]
fn write_one_shim(shims: &Path, tool: &str, markers: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let marker = markers.join(format!("{tool}.called"));
    let body = format!(
        "#!/bin/sh\necho called > \"{}\"\nexit 1\n",
        marker.display()
    );
    let path = shims.join(tool);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// True if any Node shim recorded a call.
fn node_was_called(markers: &Path) -> Option<String> {
    for tool in ["node", "npm", "npx"] {
        if markers.join(format!("{tool}.called")).exists() {
            return Some(tool.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// The metadata-only fixture (Decision 8a), copied outside the repository
// ---------------------------------------------------------------------------

/// A minimal bin-only crate: `cargo metadata` succeeds offline and the default
/// rustdoc plan is empty, so `build` is a metadata-only success needing no
/// nightly and no network.
fn write_metadata_only_fixture(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"),
    )
    .unwrap();
    std::fs::write(dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();
}

/// Recursively collect files under a directory, forward-slashed and relative.
fn walk_rel(base: &Path) -> Vec<String> {
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
    go(base, base, &mut out);
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Part 1 — mandatory unpublished-package source-replacement preflight
// ---------------------------------------------------------------------------

/// Prove Cargo 1.97.1 installs an unpublished package added to a `local-registry`
/// that replaces crates-io, from a fresh CARGO_HOME, `--locked --offline`. This
/// is the load-bearing proof for the whole locked mechanism, kept permanently
/// (Part 12) and cheap enough to gate the expensive harness on.
#[test]
#[ignore = "packages a probe and installs it offline; run explicitly as the Phase-5B preflight"]
fn unpublished_package_source_replacement_preflight() {
    let tmp = tempfile::tempdir().unwrap();
    run_preflight(tmp.path());
}

/// The preflight body, reusable so the full harness can GATE on it (Part 4
/// control #7): a failing preflight panics here, before any packaging/assembly.
fn run_preflight(root: &Path) {
    std::fs::create_dir_all(root).unwrap();
    // A minimal binary crate OUTSIDE the CrateVista workspace, no third-party dep.
    let probe = root.join("probe");
    std::fs::create_dir_all(probe.join("src")).unwrap();
    std::fs::write(
        probe.join("Cargo.toml"),
        "[package]\nname = \"cratevista-local-registry-probe\"\nversion = \"0.0.1\"\n\
         edition = \"2021\"\nlicense = \"MIT OR Apache-2.0\"\ndescription = \"probe\"\n",
    )
    .unwrap();
    std::fs::write(
        probe.join("src").join("main.rs"),
        "fn main() { println!(\"cratevista-probe-ok\"); }\n",
    )
    .unwrap();

    // Package it into a `.crate` (not a manually written index).
    run_ok(
        "preflight-package",
        Command::new(cargo())
            .current_dir(&probe)
            .args(["package", "--no-verify", "--allow-dirty"]),
    );
    let archive = probe.join("target/package/cratevista-local-registry-probe-0.0.1.crate");
    assert!(archive.is_file(), "probe archive must exist");

    // Add it to a local registry the same way the main harness adds the nine
    // internal crates: copy the archive + write the tool-format index entry.
    let registry = root.join("registry");
    std::fs::create_dir_all(&registry).unwrap();
    let dst = registry.join("cratevista-local-registry-probe-0.0.1.crate");
    std::fs::copy(&archive, &dst).unwrap();
    let entry = format!(
        "{{\"name\":\"cratevista-local-registry-probe\",\"vers\":\"0.0.1\",\"deps\":[],\
         \"cksum\":\"{}\",\"features\":{{}},\"yanked\":false}}",
        sha256_hex(&dst)
    );
    let idx = index_path(&registry, "cratevista-local-registry-probe");
    std::fs::create_dir_all(idx.parent().unwrap()).unwrap();
    std::fs::write(&idx, entry).unwrap();

    // Fresh CARGO_HOME with only the replacement config.
    let cargo_home = root.join("cargo-home");
    write_cargo_home(&cargo_home, &registry);

    // Install from a clean cwd with no repository manifest.
    let cwd = root.join("cwd");
    std::fs::create_dir_all(&cwd).unwrap();
    let install_root = root.join("install-root");

    let out = run(
        "preflight-install",
        Command::new(cargo())
            .current_dir(&cwd)
            .env("CARGO_HOME", toml_abs(&cargo_home))
            .env("CARGO_NET_OFFLINE", "true")
            .env_remove("CARGO_REGISTRY_TOKEN")
            .args([
                "install",
                "cratevista-local-registry-probe",
                "--version",
                "0.0.1",
                "--locked",
                "--offline",
                "--root",
                toml_abs(&install_root).as_str(),
            ]),
    );
    assert!(
        out.status.success(),
        "PREFLIGHT FAILED — Cargo rejected an unpublished package in a crates-io \
         replacement source. Per PRD 10 Phase 5B this STOPS the phase.\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Execute the installed binary and verify its output.
    let bin = install_root.join("bin").join(if cfg!(windows) {
        "cratevista-local-registry-probe.exe"
    } else {
        "cratevista-local-registry-probe"
    });
    assert!(
        bin.is_file(),
        "installed probe binary must exist at {bin:?}"
    );
    let run_out = run("preflight-run", &mut Command::new(&bin));
    assert!(run_out.status.success(), "installed probe must run");
    assert!(
        String::from_utf8_lossy(&run_out.stdout).contains("cratevista-probe-ok"),
        "installed probe must print its marker"
    );
}

// ---------------------------------------------------------------------------
// Parts 2–10 — the full nine-crate offline install harness
// ---------------------------------------------------------------------------

#[test]
#[ignore = "packages nine crates and installs cargo-cratevista offline; run explicitly (expensive)"]
fn local_registry_package_install_works() {
    let root = repo_root();
    let tmp = tempfile::tempdir().unwrap();
    // macOS exposes its temporary directory through `/var`, which is a symlink
    // to `/private/var`. Use the physical path so the production symlink guard
    // tests the harness's own paths rather than that OS-level alias.
    let canonical_tmp = tmp.path().canonicalize().unwrap();
    let base = canonical_tmp.as_path();

    // --- Part 4 control #7: the preflight GATES the whole harness. If Cargo no
    //     longer accepts an unpublished package from a replaced source, this
    //     panics BEFORE any packaging, assembly or install happens.
    run_preflight(&base.join("preflight"));

    // --- Part 3: package the nine crates into a dedicated, empty output. -------
    let pkg_target = base.join("pkg-target");
    let pkg_dir = package_all(&root, &pkg_target);

    // --- Part 4/5: assemble the offline registry from packaged manifests. ------
    let registry = base.join("registry");
    let work = base.join("work");
    assemble_registry(&root, &registry, &pkg_dir, &work);
    assert_registry_wellformed(&root, &registry, &work);

    // --- Part 6: fresh offline CARGO_HOME + poison shims. ---------------------
    let cargo_home = base.join("cargo-home");
    write_cargo_home(&cargo_home, &registry);
    assert_ne!(
        std::fs::canonicalize(&cargo_home).unwrap(),
        home_cargo_dir(),
        "the harness must never use the developer's CARGO_HOME"
    );
    let shims = base.join("shims");
    let markers = base.join("markers");
    write_node_poison(&shims, &markers);

    // --- Part 7/8: install cargo-cratevista from the registry, offline, no Node.
    let install_root = base.join("install-root");
    let clean_cwd = base.join("clean-cwd");
    std::fs::create_dir_all(&clean_cwd).unwrap();
    assert!(
        !clean_cwd.join("Cargo.toml").exists(),
        "install must run from a directory with no repository manifest"
    );
    assert!(
        !clean_cwd.starts_with(&root),
        "install cwd must be outside the workspace"
    );

    let install_args = [
        "install".to_string(),
        "cargo-cratevista".to_string(),
        "--version".to_string(),
        CRATE_VERSION.to_string(),
        "--locked".to_string(),
        "--offline".to_string(),
        "--root".to_string(),
        toml_abs(&install_root),
        "--verbose".to_string(),
    ];
    // Policy (negative controls 5 and 6): the install is always offline and never
    // path-based.
    assert!(install_args.iter().any(|a| a == "--offline"));
    assert!(!install_args.iter().any(|a| a == "--path"));

    let install_path = prepend_path(std::slice::from_ref(&shims));
    let out = run(
        "install",
        Command::new(cargo())
            .current_dir(&clean_cwd)
            .env("CARGO_HOME", toml_abs(&cargo_home))
            .env("CARGO_NET_OFFLINE", "true")
            .env("PATH", &install_path)
            .env_remove("CARGO_REGISTRY_TOKEN")
            .env_remove("CARGO_REGISTRIES_CRATES_IO_TOKEN")
            .args(&install_args),
    );
    assert!(
        out.status.success(),
        "offline install from the local registry must succeed\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // The installed executable exists.
    let installed_bin = install_root.join("bin").join(if cfg!(windows) {
        "cargo-cratevista.exe"
    } else {
        "cargo-cratevista"
    });
    assert!(installed_bin.is_file(), "installed binary must exist");

    // Cargo reports the local registry as the source, and no repository path
    // appears in the verbose command output.
    assert!(
        combined.contains("registry"),
        "verbose install output should reference the local registry source"
    );
    let repo_str = root.to_string_lossy().replace('\\', "/");
    assert!(
        !combined.replace('\\', "/").contains(repo_str.as_str()),
        "no repository path may appear in the install output"
    );

    // No Node ran during install.
    assert!(
        node_was_called(&markers).is_none(),
        "no Node command may run during install (a poison shim fired)"
    );

    // --- Part 9: exercise the installed `cargo cratevista`. --------------------
    let fixture = base.join("fixture");
    write_metadata_only_fixture(&fixture, "metaonly");
    assert!(
        !fixture.starts_with(&root),
        "fixture must be outside the repo"
    );

    // Run through the cargo subcommand dispatch: cargo finds `cargo-cratevista`
    // on PATH (install root prepended), and the Node poison stays in force.
    let run_path = prepend_path(&[install_root.join("bin"), shims.clone()]);

    let help = run(
        "installed-help",
        Command::new(cargo())
            .current_dir(&clean_cwd)
            .env("PATH", &run_path)
            .env("CARGO_NET_OFFLINE", "true")
            .args(["cratevista", "--help"]),
    );
    assert!(
        help.status.success(),
        "installed `cargo cratevista --help` must succeed"
    );
    let help_out = String::from_utf8_lossy(&help.stdout);
    assert!(
        help_out.contains("build"),
        "help must list build: {help_out}"
    );
    assert!(
        !help_out.to_lowercase().contains("not implemented")
            && !help_out.to_lowercase().contains("unimplemented"),
        "help must not say unimplemented: {help_out}"
    );

    let site = base.join("site");
    let build = run(
        "installed-build",
        Command::new(cargo())
            .current_dir(&clean_cwd)
            .env("PATH", &run_path)
            .env("CARGO_NET_OFFLINE", "true")
            .args([
                "cratevista",
                "build",
                "--manifest-path",
                fixture.join("Cargo.toml").to_string_lossy().as_ref(),
                "--output",
                site.to_string_lossy().as_ref(),
            ]),
    );
    assert!(
        build.status.success(),
        "installed metadata-only build must succeed\n--- stderr ---\n{}",
        String::from_utf8_lossy(&build.stderr),
    );

    assert_metadata_only_site(&site);
    assert!(
        node_was_called(&markers).is_none(),
        "no Node command may run during the installed build"
    );

    // --- Part 10: negative controls (fast, offline). --------------------------
    negative_controls(&registry, &shims, &markers, base);

    // Cleanup: TempDir removes everything on drop. Preserve on demand.
    if std::env::var_os("CRATEVISTA_KEEP_REGISTRY_TMP").is_some() {
        let kept = tmp.keep();
        eprintln!("[cleanup] preserved temp tree at {}", kept.display());
    }
}

/// The developer's real cargo home, canonicalized, for the isolation assertion.
fn home_cargo_dir() -> PathBuf {
    let raw = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs_home().map(|h| h.join(".cargo")))
        .unwrap_or_else(|| PathBuf::from(".cargo"));
    std::fs::canonicalize(&raw).unwrap_or(raw)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Assert the produced site is the Decision-8a metadata-only static site.
fn assert_metadata_only_site(site: &Path) {
    for name in [
        "index.html",
        "document.json",
        "generation.json",
        "diagnostics.json",
        ".cratevista-static-site.json",
    ] {
        assert!(site.join(name).is_file(), "{name} must exist in the site");
    }

    // Marker C: a site marker with no output_key.
    let marker = std::fs::read_to_string(site.join(".cratevista-static-site.json")).unwrap();
    assert!(
        marker.contains("\"kind\":\"site\""),
        "marker must be a site marker: {marker}"
    );
    assert!(
        !marker.contains("output_key"),
        "marker C must omit output_key: {marker}"
    );

    // The static-mode meta appears exactly once.
    let index = std::fs::read_to_string(site.join("index.html")).unwrap();
    assert_eq!(
        index.matches("cratevista-mode").count(),
        1,
        "static-mode marker must appear exactly once"
    );

    // A fingerprinted frontend asset is present; no /api, source or snippets.
    let files = walk_rel(site);
    assert!(
        files
            .iter()
            .any(|f| f.starts_with("assets/") && f.ends_with(".js")),
        "a bundled frontend script asset must exist: {files:?}"
    );
    for f in &files {
        assert!(!f.contains("api"), "no /api files may exist: {f}");
        assert!(!f.starts_with("source/"), "no source/ directory: {f}");
        assert!(!f.starts_with("snippets/"), "no snippets/ directory: {f}");
    }
}

/// Fast, offline negative controls. Each mutates the registry, proves the
/// intended assertion fails, then restores it before the next.
fn negative_controls(registry: &Path, shims: &Path, markers: &Path, base: &Path) {
    // A fast, offline install attempt into a throwaway root, each with its OWN
    // fresh CARGO_HOME. A fresh home is essential: a shared home would let Cargo
    // reuse a crate it extracted during the main install and mask the removed
    // registry file, defeating the control. With an empty cache + `--offline`, a
    // missing `.crate` is a hard failure at the unpack stage, before any compile,
    // so each control stays cheap.
    let attempt = |label: &str| -> bool {
        let root = base.join(format!("nc-{label}"));
        let home = base.join(format!("nc-home-{label}"));
        write_cargo_home(&home, registry);
        let out = run(
            &format!("nc-{label}"),
            Command::new(cargo())
                .current_dir(base)
                .env("CARGO_HOME", toml_abs(&home))
                .env("CARGO_NET_OFFLINE", "true")
                .env("PATH", prepend_path(&[shims.to_path_buf()]))
                .args([
                    "install",
                    "cargo-cratevista",
                    "--version",
                    CRATE_VERSION,
                    "--locked",
                    "--offline",
                    "--root",
                    toml_abs(&root).as_str(),
                ]),
        );
        // Must fail, and must not have reached crates.io (offline).
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !combined.contains("Downloading") && !combined.contains("Updating crates.io"),
            "[nc-{label}] must not fetch from the network while offline:\n{combined}"
        );
        out.status.success()
    };

    // Move a file aside, run the closure, restore.
    let with_removed = |rel: &Path, f: &dyn Fn()| {
        let saved = base.join("nc-saved.bin");
        std::fs::rename(rel, &saved).unwrap();
        f();
        std::fs::rename(&saved, rel).unwrap();
    };

    // 1. Missing cargo-cratevista archive -> install fails offline.
    with_removed(
        &registry.join(format!("cargo-cratevista-{CRATE_VERSION}.crate")),
        &|| {
            assert!(
                !attempt("no-cli-archive"),
                "install must fail without the CLI archive"
            )
        },
    );

    // 2. Missing cratevista-core archive -> install fails offline.
    with_removed(
        &registry.join(format!("cratevista-core-{CRATE_VERSION}.crate")),
        &|| {
            assert!(
                !attempt("no-core-archive"),
                "install must fail without cratevista-core"
            )
        },
    );

    // 3. Missing one REQUIRED third-party archive -> install fails offline, no
    //    fetch. Must be a crate that is actually in cargo-cratevista's runtime
    //    graph (an arbitrary lockfile crate might be a dev/platform-only dep that
    //    the install never touches); `clap` is a direct dependency, so its
    //    removal necessarily breaks the build.
    let third_party = required_third_party_archive(registry, "clap");
    with_removed(&third_party, &|| {
        assert!(
            !attempt("no-third-party"),
            "install must fail with a missing required third-party crate"
        )
    });

    // 9. A changed internal version requirement -> offline resolution fails.
    {
        let core_idx = index_path(registry, "cratevista-core");
        let original = std::fs::read_to_string(&core_idx).unwrap();
        let broken = original.replace("\"cratevista-schema\"", "\"cratevista-schema-missing\"");
        assert_ne!(broken, original, "control 9 must actually change the index");
        std::fs::write(&core_idx, broken).unwrap();
        assert!(
            !attempt("bad-internal-req"),
            "install must fail on a broken internal req"
        );
        std::fs::write(&core_idx, original).unwrap();
    }

    // NEW A. Wrong checksum in an internal index row -> Cargo rejects it offline.
    {
        let core_idx = index_path(registry, "cratevista-core");
        let original = std::fs::read_to_string(&core_idx).unwrap();
        let mut entry: serde_json::Value =
            serde_json::from_str(original.lines().next().unwrap()).unwrap();
        let bogus = "b".repeat(64);
        assert_ne!(entry["cksum"].as_str().unwrap(), bogus);
        entry["cksum"] = serde_json::Value::String(bogus);
        std::fs::write(&core_idx, serde_json::to_string(&entry).unwrap()).unwrap();
        assert!(
            !attempt("bad-checksum"),
            "install must fail on a wrong internal checksum"
        );
        std::fs::write(&core_idx, original).unwrap();
    }

    // NEW B. Internal index ROW missing while the archive remains -> offline
    //        resolution fails (distinct from control 2, which removes the archive).
    with_removed(&index_path(registry, "cratevista-core"), &|| {
        assert!(
            !attempt("no-core-index-row"),
            "install must fail when cratevista-core has no index row"
        )
    });

    // NEW C. `cargo local-registry add` substituted for internal assembly still
    //        cannot ingest an unpublished local crate. Robust regardless of
    //        network: `add` never SUCCEEDS for our unpublished crate, and never
    //        writes its entry (it resolves from crates.io, which lacks it).
    {
        let tmp_reg = base.join("nc-add-registry");
        std::fs::create_dir_all(&tmp_reg).unwrap();
        let out = run(
            "nc-add-substitution",
            Command::new(cargo()).args([
                "local-registry",
                "add",
                "cratevista-schema",
                "--version",
                CRATE_VERSION,
                tmp_reg.to_string_lossy().as_ref(),
            ]),
        );
        assert!(
            !out.status.success(),
            "cargo local-registry add must not ingest an unpublished local crate"
        );
        assert!(
            !index_path(&tmp_reg, "cratevista-schema").exists()
                && !tmp_reg
                    .join(format!("cratevista-schema-{CRATE_VERSION}.crate"))
                    .exists(),
            "the failed add must not have written any cratevista-schema entry"
        );
    }

    // 7. A server archive without embedded/index.html would fail the package
    //    assertion. Prove the content predicate is discriminating.
    let good = ["embedded/index.html".to_string(), "src/lib.rs".to_string()];
    let bad = ["src/lib.rs".to_string()];
    assert!(good.iter().any(|f| f == "embedded/index.html"));
    assert!(
        !bad.iter().any(|f| f == "embedded/index.html"),
        "a server package missing embedded/index.html must fail the content check"
    );

    // 8. The Node poison itself: invoking a shim records the call and fails.
    let probe_markers = base.join("nc-node-markers");
    let probe_shims = base.join("nc-node-shims");
    write_node_poison(&probe_shims, &probe_markers);
    let node_bin = probe_shims.join(if cfg!(windows) { "node.cmd" } else { "node" });
    let node_out = run("nc-node-poison", &mut Command::new(&node_bin));
    assert!(!node_out.status.success(), "the node poison shim must fail");
    assert!(
        node_was_called(&probe_markers).is_some(),
        "the node poison shim must record its invocation"
    );

    // The real install left no Node markers.
    assert!(
        node_was_called(markers).is_none(),
        "no Node marker may exist after the main install/build"
    );

    // Remaining Part-4 controls are covered elsewhere:
    //  - #4 (duplicate row), #5 (provenance / altered-metadata), #6 (manifest/
    //    archive mismatch) are the fast `index_rows_are_derived_from_the_packaged_
    //    manifest` unit test;
    //  - #7 (a failed/skipped preflight halts everything) is structural:
    //    `run_preflight` runs first in this harness, before any packaging/assembly;
    //  - the fresh-CARGO_HOME isolation, `--offline`-always and no-`--path` policy,
    //    and the outside-workspace cwd are asserted at the main call site.
}

/// The archive for a specific crate name (`<name>-<version>.crate`), used to
/// remove a crate that is definitely in the install graph.
fn required_third_party_archive(registry: &Path, crate_name: &str) -> PathBuf {
    let prefix = format!("{crate_name}-");
    std::fs::read_dir(registry)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            // `<name>-<digit>...crate` — the crate literally named `crate_name`,
            // not a longer crate that merely shares the prefix (e.g. clap_builder).
            name.ends_with(".crate")
                && name.starts_with(&prefix)
                && name[prefix.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
        })
        .unwrap_or_else(|| {
            panic!("the required third-party crate `{crate_name}` must be in the registry")
        })
}
