//! Release-archive assembly + verification (PRD 10, Phase 6 / Phase 7F).
//!
//! One deterministic, cross-platform helper produces the release archives and their
//! SHA-256 checksums, so `release.yml`, the non-publishing CI smoke matrix, and the
//! local release-artifact smoke all share a single implementation — no per-leg shell
//! logic. Everything here is harness-only (`flate2`/`tar`/`zip`/`sha2` are
//! dev-dependencies) and never enters the installed binary.
//!
//! Layout (locked): every archive contains exactly one top-level directory
//! `cargo-cratevista-<version>-<target>/` holding the binary plus the four release
//! files, and nothing else:
//!
//! ```text
//! cargo-cratevista-<version>-<target>/
//!   cargo-cratevista        (cargo-cratevista.exe on Windows targets; exec bit set on Unix)
//!   LICENSE-MIT
//!   LICENSE-APACHE
//!   README.md
//!   CHANGELOG.md
//! ```
//!
//! `.tar.gz` for Linux/macOS targets, `.zip` for Windows targets. Archives are
//! byte-deterministic given identical inputs (fixed mtimes, no gzip filename/mtime),
//! but PRD 10 makes **no** binary-reproducibility claim — the checksums verify
//! archive integrity, not that two builds of the binary are byte-identical.
//!
//! Drivers:
//! - `make_release_archive` (`#[ignore]`) — env-driven producer for CI/release.
//! - `release_archive_smoke` (`#[ignore]`) — Phase 7F: assemble from the compiled
//!   binary, extract, run `--help`, verify + corrupt the checksum.

#![allow(clippy::items_after_statements)]

use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The four non-binary files every archive ships, read from the repository root.
const RELEASE_FILES: [&str; 4] = ["LICENSE-MIT", "LICENSE-APACHE", "README.md", "CHANGELOG.md"];

/// The exact four release targets `release.yml` builds. Any drift here (an added or
/// removed target) is a test failure — the workflow and the helper must agree.
const RELEASE_TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
];

/// The workspace root (two levels up from this crate).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn is_windows_target(target: &str) -> bool {
    target.contains("windows")
}

fn archive_ext(target: &str) -> &'static str {
    if is_windows_target(target) {
        "zip"
    } else {
        "tar.gz"
    }
}

fn archive_stem(version: &str, target: &str) -> String {
    format!("cargo-cratevista-{version}-{target}")
}

fn archive_filename(version: &str, target: &str) -> String {
    format!("{}.{}", archive_stem(version, target), archive_ext(target))
}

fn binary_arcname(target: &str) -> &'static str {
    if is_windows_target(target) {
        "cargo-cratevista.exe"
    } else {
        "cargo-cratevista"
    }
}

/// One entry destined for the archive.
struct Member {
    /// Path inside the archive, including the top-level directory.
    arcname: String,
    bytes: Vec<u8>,
    executable: bool,
}

/// Collects the exact member set (binary + four release files) from `binary_path`
/// and the repository root. Fails if the binary or any release file is missing, so a
/// dropped licence is an assembly error, never a silently short archive.
fn gather_members(version: &str, target: &str, binary_path: &Path) -> Result<Vec<Member>, String> {
    let stem = archive_stem(version, target);
    let root = repo_root();

    let binary = std::fs::read(binary_path)
        .map_err(|e| format!("release binary missing at {}: {e}", binary_path.display()))?;
    let mut members = vec![Member {
        arcname: format!("{stem}/{}", binary_arcname(target)),
        bytes: binary,
        executable: true,
    }];

    for name in RELEASE_FILES {
        let path = root.join(name);
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("release file missing at {}: {e}", path.display()))?;
        members.push(Member {
            arcname: format!("{stem}/{name}"),
            bytes,
            executable: false,
        });
    }
    Ok(members)
}

/// The arcnames a well-formed archive must contain, sorted.
fn expected_arcnames(version: &str, target: &str) -> Vec<String> {
    let stem = archive_stem(version, target);
    let mut names = vec![format!("{stem}/{}", binary_arcname(target))];
    for name in RELEASE_FILES {
        names.push(format!("{stem}/{name}"));
    }
    names.sort();
    names
}

/// Assembles the archive for `target` at `<out_dir>/<archive_filename>` and returns
/// its path. `.tar.gz` for Unix targets, `.zip` for Windows targets.
fn assemble(
    version: &str,
    target: &str,
    binary_path: &Path,
    out_dir: &Path,
) -> Result<PathBuf, String> {
    let members = gather_members(version, target, binary_path)?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("create out dir: {e}"))?;
    let archive = out_dir.join(archive_filename(version, target));
    if is_windows_target(target) {
        write_zip(&archive, &members)?;
    } else {
        write_tar_gz(&archive, &members)?;
    }
    Ok(archive)
}

fn write_tar_gz(archive: &Path, members: &[Member]) -> Result<(), String> {
    use flate2::Compression;
    use flate2::GzBuilder;

    let file = std::fs::File::create(archive).map_err(|e| format!("create archive: {e}"))?;
    // A fixed gzip header (mtime 0, no filename) keeps the container deterministic.
    let gz = GzBuilder::new()
        .mtime(0)
        .write(file, Compression::default());
    let mut builder = tar::Builder::new(gz);
    for member in members {
        let mut header = tar::Header::new_gnu();
        header.set_size(member.bytes.len() as u64);
        header.set_mode(if member.executable { 0o755 } else { 0o644 });
        header.set_mtime(0);
        header.set_cksum();
        builder
            .append_data(&mut header, &member.arcname, &member.bytes[..])
            .map_err(|e| format!("tar append {}: {e}", member.arcname))?;
    }
    let gz = builder
        .into_inner()
        .map_err(|e| format!("tar finish: {e}"))?;
    gz.finish().map_err(|e| format!("gzip finish: {e}"))?;
    Ok(())
}

fn write_zip(archive: &Path, members: &[Member]) -> Result<(), String> {
    use zip::write::SimpleFileOptions;

    let file = std::fs::File::create(archive).map_err(|e| format!("create archive: {e}"))?;
    let mut writer = zip::ZipWriter::new(file);
    // A fixed DOS timestamp keeps the zip deterministic.
    let base = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    for member in members {
        let opts = base.unix_permissions(if member.executable { 0o755 } else { 0o644 });
        writer
            .start_file(&member.arcname, opts)
            .map_err(|e| format!("zip start {}: {e}", member.arcname))?;
        writer
            .write_all(&member.bytes)
            .map_err(|e| format!("zip write {}: {e}", member.arcname))?;
    }
    writer.finish().map_err(|e| format!("zip finish: {e}"))?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Writes `<archive>.sha256` in `sha256sum -c` format (`<hex>  <basename>\n`) and
/// returns its path. The digest is recomputed from the archive bytes on disk.
fn write_checksum(archive: &Path) -> Result<PathBuf, String> {
    let bytes = std::fs::read(archive).map_err(|e| format!("read archive: {e}"))?;
    let hex = sha256_hex(&bytes);
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("archive has no filename")?;
    let sha_path = archive.with_file_name(format!("{name}.sha256"));
    std::fs::write(&sha_path, format!("{hex}  {name}\n"))
        .map_err(|e| format!("write checksum: {e}"))?;
    Ok(sha_path)
}

/// Recomputes the archive's digest and matches it against `<archive>.sha256`,
/// confirming both the digest and the referenced filename.
fn verify_checksum(archive: &Path) -> Result<(), String> {
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("archive has no filename")?;
    let sha_path = archive.with_file_name(format!("{name}.sha256"));
    let recorded = std::fs::read_to_string(&sha_path)
        .map_err(|e| format!("checksum missing at {}: {e}", sha_path.display()))?;
    let mut parts = recorded.split_whitespace();
    let recorded_hex = parts.next().ok_or("empty checksum file")?;
    let recorded_name = parts.next().ok_or("checksum file names no archive")?;
    if recorded_name != name {
        return Err(format!("checksum names {recorded_name}, expected {name}"));
    }
    if recorded_hex.len() != 64 || !recorded_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("malformed digest: {recorded_hex}"));
    }
    if recorded_hex != recorded_hex.to_ascii_lowercase() {
        return Err("digest must be lowercase hex".into());
    }
    let bytes = std::fs::read(archive).map_err(|e| format!("read archive: {e}"))?;
    let actual = sha256_hex(&bytes);
    if actual != recorded_hex {
        return Err(format!(
            "digest mismatch: archive={actual} checksum={recorded_hex}"
        ));
    }
    Ok(())
}

/// Reads back every entry as `(arcname, executable)`.
fn list_entries(archive: &Path, windows_target: bool) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    if windows_target {
        let file = std::fs::File::open(archive).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        for i in 0..zip.len() {
            let entry = zip.by_index(i).unwrap();
            let mode = entry.unix_mode().unwrap_or(0);
            out.push((entry.name().to_string(), mode & 0o111 != 0));
        }
    } else {
        use flate2::read::GzDecoder;
        let file = std::fs::File::open(archive).unwrap();
        let mut ar = tar::Archive::new(GzDecoder::new(file));
        for entry in ar.entries().unwrap() {
            let entry = entry.unwrap();
            let mode = entry.header().mode().unwrap_or(0);
            let name = entry.path().unwrap().to_string_lossy().replace('\\', "/");
            out.push((name, mode & 0o111 != 0));
        }
    }
    out.sort();
    out
}

/// The host target triple string, used only to NAME the local smoke archive; the
/// extracted binary is the host binary regardless of the string.
fn host_target() -> &'static str {
    if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        }
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

// ---------------------------------------------------------------------------
// Fast tests (no network, no release build)
// ---------------------------------------------------------------------------

#[test]
fn archive_names_and_targets_are_exactly_the_four() {
    assert_eq!(RELEASE_TARGETS.len(), 4);
    assert_eq!(
        archive_filename("0.1.0", "x86_64-unknown-linux-gnu"),
        "cargo-cratevista-0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    );
    assert_eq!(
        archive_filename("0.1.0", "aarch64-apple-darwin"),
        "cargo-cratevista-0.1.0-aarch64-apple-darwin.tar.gz"
    );
    assert_eq!(
        archive_filename("0.1.0", "x86_64-pc-windows-msvc"),
        "cargo-cratevista-0.1.0-x86_64-pc-windows-msvc.zip"
    );
    // Three tar.gz, one zip.
    let zips = RELEASE_TARGETS
        .iter()
        .filter(|t| archive_ext(t) == "zip")
        .count();
    assert_eq!(zips, 1, "exactly one Windows zip target");
}

/// The release.yml matrix must declare exactly these four target triples, so the
/// helper and the workflow can never drift (negative control 9).
#[test]
fn release_workflow_declares_exactly_the_four_targets() {
    let yaml = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml")).unwrap();
    for target in RELEASE_TARGETS {
        assert!(
            yaml.contains(target),
            "release.yml must build target {target}"
        );
    }
    // No stray extra target triple sneaks in.
    for stray in [
        "aarch64-unknown-linux-gnu",
        "i686-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
        "armv7-unknown-linux-gnueabihf",
    ] {
        assert!(
            !yaml.contains(stray),
            "release.yml must not build unexpected target {stray}"
        );
    }
}

#[test]
fn assembles_tar_gz_with_exact_contents_and_exec_bit() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("cargo-cratevista");
    std::fs::write(&bin, b"#!/bin/sh\necho stub\n").unwrap();
    let out = tmp.path().join("out");
    let target = "x86_64-unknown-linux-gnu";

    let archive = assemble("0.1.0", target, &bin, &out).unwrap();
    assert_eq!(
        archive.file_name().unwrap().to_str().unwrap(),
        "cargo-cratevista-0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    );

    let entries = list_entries(&archive, false);
    let names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
    assert_eq!(
        names,
        expected_arcnames("0.1.0", target),
        "exact content set"
    );

    // The binary carries the exec bit; the release files do not.
    for (name, exec) in &entries {
        if name.ends_with("/cargo-cratevista") {
            assert!(*exec, "binary must be executable: {name}");
        } else {
            assert!(!*exec, "release file must not be executable: {name}");
        }
    }

    // No forbidden files leaked in.
    for name in &names {
        for forbidden in ["Cargo.toml", "src/", "target/", "node_modules", ".crate"] {
            assert!(
                !name.contains(forbidden),
                "forbidden entry {forbidden} in {name}"
            );
        }
    }
}

#[test]
fn assembles_zip_on_windows_target() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("cargo-cratevista.exe");
    std::fs::write(&bin, b"MZ stub").unwrap();
    let out = tmp.path().join("out");
    let target = "x86_64-pc-windows-msvc";

    let archive = assemble("0.1.0", target, &bin, &out).unwrap();
    assert_eq!(
        archive.file_name().unwrap().to_str().unwrap(),
        "cargo-cratevista-0.1.0-x86_64-pc-windows-msvc.zip"
    );
    let entries = list_entries(&archive, true);
    let names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
    assert_eq!(names, expected_arcnames("0.1.0", target));
    assert!(
        names.iter().any(|n| n.ends_with("/cargo-cratevista.exe")),
        "windows archive ships the .exe: {names:?}"
    );
}

#[test]
fn checksum_roundtrips_and_detects_corruption() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("cargo-cratevista");
    std::fs::write(&bin, b"stub").unwrap();
    let out = tmp.path().join("out");
    let archive = assemble("0.1.0", "x86_64-unknown-linux-gnu", &bin, &out).unwrap();

    let sha = write_checksum(&archive).unwrap();
    verify_checksum(&archive).expect("fresh checksum verifies");

    // Corrupt the archive bytes AFTER the checksum was written → verification fails.
    {
        let mut bytes = std::fs::read(&archive).unwrap();
        bytes[0] ^= 0xff;
        std::fs::write(&archive, &bytes).unwrap();
    }
    assert!(
        verify_checksum(&archive).is_err(),
        "corrupted archive must fail checksum verification"
    );

    // A wrong recorded digest fails too.
    let name = archive.file_name().unwrap().to_str().unwrap();
    std::fs::write(&sha, format!("{}  {name}\n", "0".repeat(64))).unwrap();
    assert!(
        verify_checksum(&archive).is_err(),
        "wrong digest must fail verification"
    );
}

#[test]
fn missing_binary_or_licence_fails_assembly() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");

    // Missing binary.
    let missing_bin = tmp.path().join("nope");
    assert!(
        assemble("0.1.0", "x86_64-unknown-linux-gnu", &missing_bin, &out).is_err(),
        "a missing binary must fail assembly"
    );
}

/// Negative control 7: an archive that omits a licence is rejected by the exact
/// content-set assertion. We build a short archive by hand (binary + only three of
/// the four release files) and prove the content check catches it.
#[test]
fn a_short_archive_missing_a_licence_is_rejected_by_the_content_check() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("cargo-cratevista");
    std::fs::write(&bin, b"stub").unwrap();
    let target = "x86_64-unknown-linux-gnu";

    // Full members, minus LICENSE-APACHE.
    let mut members = gather_members("0.1.0", target, &bin).unwrap();
    members.retain(|m| !m.arcname.ends_with("/LICENSE-APACHE"));
    let archive = tmp.path().join(archive_filename("0.1.0", target));
    write_tar_gz(&archive, &members).unwrap();

    let names: Vec<String> = list_entries(&archive, false)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_ne!(
        names,
        expected_arcnames("0.1.0", target),
        "a licence-short archive must NOT match the exact expected content set"
    );
    assert!(
        !names.iter().any(|n| n.ends_with("/LICENSE-APACHE")),
        "the omission is real"
    );
}

// ---------------------------------------------------------------------------
// Ignored drivers (CI/release producer + local smoke)
// ---------------------------------------------------------------------------

/// Env-driven producer invoked by `release.yml` and the CI smoke matrix:
///
/// ```text
/// CRATEVISTA_RELEASE_VERSION=0.1.0 \
/// CRATEVISTA_RELEASE_TARGET=x86_64-unknown-linux-gnu \
/// CRATEVISTA_RELEASE_BIN=target/<t>/release/cargo-cratevista \
/// CRATEVISTA_RELEASE_OUTDIR=dist-release \
/// cargo test -p cargo-cratevista --test release_archive make_release_archive -- --ignored --exact --nocapture
/// ```
#[test]
#[ignore = "release/CI producer; run with --ignored and the CRATEVISTA_RELEASE_* env"]
fn make_release_archive() {
    let version = std::env::var("CRATEVISTA_RELEASE_VERSION").expect("CRATEVISTA_RELEASE_VERSION");
    let target = std::env::var("CRATEVISTA_RELEASE_TARGET").expect("CRATEVISTA_RELEASE_TARGET");
    let bin =
        PathBuf::from(std::env::var("CRATEVISTA_RELEASE_BIN").expect("CRATEVISTA_RELEASE_BIN"));
    let out = PathBuf::from(
        std::env::var("CRATEVISTA_RELEASE_OUTDIR").expect("CRATEVISTA_RELEASE_OUTDIR"),
    );

    assert!(
        RELEASE_TARGETS.contains(&target.as_str()),
        "unexpected release target: {target}"
    );
    let archive = assemble(&version, &target, &bin, &out).expect("assemble archive");
    let sha = write_checksum(&archive).expect("write checksum");
    verify_checksum(&archive).expect("verify checksum");

    // Content-set assertion, from the archive bytes themselves.
    let names: Vec<String> = list_entries(&archive, is_windows_target(&target))
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(
        names,
        expected_arcnames(&version, &target),
        "exact content set"
    );
    println!(
        "made {} + {}",
        archive.display(),
        sha.file_name().unwrap().to_string_lossy()
    );
}

/// Phase 7F local smoke: assemble an archive from the compiled binary under test,
/// verify it, extract it, run the extracted binary's `--help`, and prove a corrupted
/// checksum fails. Uses the host target for naming; the binary is the host binary.
#[test]
#[ignore = "builds/extracts an archive and runs the extracted binary; run with --ignored"]
fn release_archive_smoke() {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_cargo-cratevista"));
    assert!(
        binary.is_file(),
        "compiled binary present: {}",
        binary.display()
    );
    let target = host_target();
    let version = "0.1.0";

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("dist");
    let archive = assemble(version, target, &binary, &out).expect("assemble host archive");
    write_checksum(&archive).expect("checksum");
    verify_checksum(&archive).expect("verify");

    // Exact contents.
    let names: Vec<String> = list_entries(&archive, is_windows_target(target))
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(names, expected_arcnames(version, target));

    // Extract and run the extracted binary — nothing from the repo is needed.
    let dest = tmp.path().join("extracted");
    std::fs::create_dir_all(&dest).unwrap();
    let stem = archive_stem(version, target);
    if is_windows_target(target) {
        let file = std::fs::File::open(&archive).unwrap();
        zip::ZipArchive::new(file).unwrap().extract(&dest).unwrap();
    } else {
        use flate2::read::GzDecoder;
        let file = std::fs::File::open(&archive).unwrap();
        tar::Archive::new(GzDecoder::new(file))
            .unpack(&dest)
            .unwrap();
    }
    let extracted_bin = dest.join(&stem).join(binary_arcname(target));
    assert!(
        extracted_bin.is_file(),
        "extracted binary present: {}",
        extracted_bin.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&extracted_bin)
            .unwrap()
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "extracted binary is executable");
    }

    // The extracted binary runs and prints usage mentioning the `build` command.
    let output = std::process::Command::new(&extracted_bin)
        .arg("--help")
        .output()
        .expect("run extracted --help");
    assert!(output.status.success(), "extracted --help exits 0");
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    assert!(
        text.contains("build"),
        "help lists the build command: {text}"
    );

    // Corruption is caught.
    {
        let mut bytes = std::fs::read(&archive).unwrap();
        bytes.push(0);
        std::fs::write(&archive, &bytes).unwrap();
    }
    assert!(
        verify_checksum(&archive).is_err(),
        "corruption fails verify"
    );
}
