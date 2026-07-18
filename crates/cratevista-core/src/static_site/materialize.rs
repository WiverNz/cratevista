//! Cargo-free static-site materialization, recovery and transactional publication
//! (PRD 10, Decision 2 / Phase 2B).
//!
//! [`materialize_static_site`] turns an in-memory asset set plus the three
//! generation artifacts into a published static site at `<output>`, under a
//! per-output advisory lock, with crash-safe A/B/C ownership markers, key-scoped
//! recovery of any interrupted predecessor, and transactional publish-with-rollback.
//! It never runs cargo and never generates anything; the caller supplies the
//! artifacts and the embedded assets.
//!
//! The production entry point derives the [`ResolvedOutput`], `output_key` and
//! [`OutputSafety`] from `options.output` **itself** — no caller-supplied key is
//! trusted — and performs the locked ten-step preparation before any output state
//! is inspected or mutated.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use cratevista_server::ArtifactPaths;

use super::base_path::BasePath;
use super::error::BuildError;
use super::fs_seam::{EntryKind, MarkerStage, RealSiteFs, SiteFs};
use super::html::transform_index_html;
use super::lock::OutputLock;
use super::marker::{MARKER_FILENAME, Marker, MarkerKind};
use super::nonce::{
    Candidate, backup_name, classify_for_key, generate_nonce, is_marker_temp, staging_name,
};
use super::output_identity::output_key;
use super::safety::OutputSafety;
use crate::artifacts::{DIAGNOSTICS_FILE, DOCUMENT_FILE, GENERATION_FILE};
use crate::clock::Clock;

/// The three artifact file names that an asset must never shadow.
const RESERVED_NAMES: [&str; 4] = [
    DOCUMENT_FILE,
    GENERATION_FILE,
    DIAGNOSTICS_FILE,
    MARKER_FILENAME,
];

/// Options for one static-site materialization.
#[derive(Debug, Clone)]
pub struct SiteOptions {
    /// The requested output directory (absolute).
    pub output: PathBuf,
    /// The optional base path for `<base href>`; `None` or empty writes none.
    pub base_path: Option<BasePath>,
    /// The single timestamp stamped into every A/B/C marker of this build.
    pub generated_at: String,
}

impl SiteOptions {
    /// Builds options, reading the build timestamp from `clock` once.
    pub fn new(output: PathBuf, base_path: Option<BasePath>, clock: &dyn Clock) -> SiteOptions {
        SiteOptions {
            output,
            base_path,
            generated_at: clock.now_rfc3339(),
        }
    }
}

/// The result of a successful publication.
///
/// The `output` path is retained for a **success** report (e.g. `run_build`
/// printing where the site was written); it is never serialized into the site and
/// never rendered in a `BuildError` diagnostic, so no absolute path leaks through an
/// error surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedSite {
    output: PathBuf,
    asset_count: usize,
    base_path: Option<String>,
}

impl PublishedSite {
    /// The published output directory (success-path use only).
    pub fn output(&self) -> &Path {
        &self.output
    }

    /// How many embedded assets were written.
    pub fn asset_count(&self) -> usize {
        self.asset_count
    }

    /// The non-empty base path that was injected, if any.
    pub fn base_path(&self) -> Option<&str> {
        self.base_path.as_deref()
    }

    /// Constructs a result directly. **Tests only** — production results come only
    /// from a real publication.
    #[cfg(test)]
    pub(crate) fn new_for_test(
        output: PathBuf,
        asset_count: usize,
        base_path: Option<String>,
    ) -> PublishedSite {
        PublishedSite {
            output,
            asset_count,
            base_path,
        }
    }
}

/// Materializes a static site at `options.output` from `assets` + `artifacts`.
///
/// Derives the output identity, key and safety context from `options.output`,
/// prepares any missing parent, acquires the per-output advisory lock, recovers any
/// interrupted predecessor, then publishes transactionally with rollback.
pub fn materialize_static_site(
    artifacts: &ArtifactPaths,
    assets: impl Iterator<Item = (String, Cow<'static, [u8]>)>,
    options: &SiteOptions,
    protected_paths: &[PathBuf],
) -> Result<PublishedSite, BuildError> {
    // The safety context derives the key from the output itself (private field).
    let safety = OutputSafety::for_output(&options.output, protected_paths.to_vec())?;
    let assets: Vec<(String, Cow<'static, [u8]>)> = assets.collect();
    run_materialize(&RealSiteFs, artifacts, &assets, options, &safety)
}

/// The seam-parameterized core, used by production (with `RealSiteFs`) and by the
/// fault-injection tests. `safety` carries the key; it is **validated** against a
/// freshly derived key before any lock/scan/mutation, so a forged key is rejected.
pub(crate) fn run_materialize(
    fs: &dyn SiteFs,
    artifacts: &ArtifactPaths,
    assets: &[(String, Cow<'static, [u8]>)],
    options: &SiteOptions,
    safety: &OutputSafety,
) -> Result<PublishedSite, BuildError> {
    // Validate the asset set up front (pure) — before any filesystem mutation.
    validate_assets(assets)?;

    // --- locked ten-step pre-lock preparation --------------------------------
    // a. resolve (rejects symlinked components)
    let resolved1 = fs.resolve(&options.output)?;
    // b. protected-path safety
    safety.check_resolved(&resolved1)?;
    // c. derive key + validate the caller-independent invariant
    let key = output_key(&resolved1);
    if safety.output_key() != key {
        return Err(internal("output-key-mismatch"));
    }
    // d. create ONLY the missing parent chain (never <output>)
    fs.create_parents(resolved1.path())?;
    // e. re-resolve + re-check
    let resolved2 = fs.resolve(&options.output)?;
    safety.check_resolved(&resolved2)?;
    // f. identity and key must be unchanged
    if resolved2 != resolved1 || output_key(&resolved2) != key {
        return Err(internal("output-identity-changed"));
    }

    let output = resolved2.path().to_path_buf();
    let parent = resolved2
        .parent()
        .ok_or_else(|| internal("output-parent"))?
        .to_path_buf();

    // g. acquire the keyed lock (build_output_busy on contention — no scan/mutation
    //    has happened; only a safely created empty parent chain may remain).
    let lock = OutputLock::acquire(&parent, &key)?;
    // h. recovery + publication, all under the lock.
    let result = publish_locked(fs, artifacts, assets, options, &key, &output, &parent);
    drop(lock);
    result
}

// ---------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------

/// The state of `<output>` after recovery, which drives the publication rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Predecessor {
    /// `<output>` is absent.
    Absent,
    /// `<output>` is an empty, adoptable directory.
    Empty,
    /// `<output>` is an owned marker-C site (to be backed up and replaced).
    Site,
}

/// How a keyed staging directory classifies for this exact key.
enum StagingClass {
    /// Owned staging (marker A, marker B) or a strict-shape P0 shell — deletable.
    Stale,
    /// Anything else — preserved, never touched.
    Preserve,
}

/// Runs interrupted-publication recovery, then publishes.
fn publish_locked(
    fs: &dyn SiteFs,
    artifacts: &ArtifactPaths,
    assets: &[(String, Cow<'static, [u8]>)],
    options: &SiteOptions,
    key: &str,
    output: &Path,
    parent: &Path,
) -> Result<PublishedSite, BuildError> {
    let predecessor = recover(fs, output, parent, key, &options.generated_at)?;
    publish(
        fs,
        artifacts,
        assets,
        options,
        key,
        output,
        parent,
        predecessor,
    )
}

/// Interrupted-publication recovery, scoped entirely to `key`.
fn recover(
    fs: &dyn SiteFs,
    output: &Path,
    parent: &Path,
    key: &str,
    generated_at: &str,
) -> Result<Predecessor, BuildError> {
    let out_state = classify_output(fs, output, key, generated_at)?;
    let (stale_staging, valid_backups) = gather_candidates(fs, parent, key)?;

    match out_state {
        // Row A: a stable published site → drop this key's stale staging, then its
        // stale backups (only after the output is confirmed valid).
        OutState::Site => {
            for dir in &stale_staging {
                fs.cleanup(dir)?;
            }
            for dir in &valid_backups {
                fs.cleanup(dir)?;
            }
            Ok(Predecessor::Site)
        }
        // Adopt an empty predecessor; drop stale staging. Backups (unusual here) are
        // preserved and cleaned by a later Row-A run.
        OutState::Empty => {
            for dir in &stale_staging {
                fs.cleanup(dir)?;
            }
            Ok(Predecessor::Empty)
        }
        OutState::Absent => match valid_backups.len() {
            // Row D: no backup → first publication.
            0 => {
                for dir in &stale_staging {
                    fs.cleanup(dir)?;
                }
                Ok(Predecessor::Absent)
            }
            // Row B: exactly one valid backup → restore first, then drop staging.
            1 => {
                fs.rename_backup_to_output(&valid_backups[0], output)
                    // Row F: a restoration rename failure is unrecoverable.
                    .map_err(|_| BuildError::PublishUnrecoverable)?;
                for dir in &stale_staging {
                    fs.cleanup(dir)?;
                }
                Ok(Predecessor::Site)
            }
            // Row C: multiple valid backups → do not guess.
            _ => Err(BuildError::RecoveryAmbiguous),
        },
    }
}

/// The classified state of `<output>` itself.
enum OutState {
    Absent,
    Empty,
    Site,
}

/// Classifies `<output>`'s own marker, finalizing a matching post-rename marker B.
fn classify_output(
    fs: &dyn SiteFs,
    output: &Path,
    key: &str,
    generated_at: &str,
) -> Result<OutState, BuildError> {
    match fs.kind(output)? {
        None => Ok(OutState::Absent),
        // resolve() already rejects a symlinked output; this is defensive.
        Some(EntryKind::Symlink) => Err(BuildError::OutputSymlink),
        // A non-directory, non-empty output was not created by CrateVista.
        Some(EntryKind::File | EntryKind::Other) => Err(BuildError::OutputNotOwned),
        Some(EntryKind::Dir) => match fs.read_marker(output)? {
            None => {
                if fs.entries(output)?.is_empty() {
                    Ok(OutState::Empty)
                } else {
                    Err(BuildError::OutputNotOwned)
                }
            }
            Some(bytes) => {
                let marker = Marker::parse(&bytes)?; // malformed → OutputMarkerInvalid
                match (marker.kind(), marker.output_key()) {
                    // Marker C — a stable published site.
                    (MarkerKind::Site, None) => Ok(OutState::Site),
                    // Marker B with a matching key — the post-rename/pre-finalization
                    // window: finalize to C and keep the newly published output.
                    (MarkerKind::Site, Some(marker_key)) if marker_key == key => {
                        fs.commit_marker(
                            output,
                            &Marker::published_at(generated_at),
                            MarkerStage::C,
                        )?;
                        Ok(OutState::Site)
                    }
                    // Marker B for another key, or a marker A, at <output>.
                    (MarkerKind::Site, Some(_)) => Err(BuildError::OutputMarkerInvalid {
                        reason: "a published output carries another output's key",
                    }),
                    (MarkerKind::Staging, _) => Err(BuildError::OutputMarkerInvalid {
                        reason: "a published output is marked as incomplete staging",
                    }),
                }
            }
        },
    }
}

/// Enumerates this key's staging (stale ones) and valid backups under `parent`.
/// Everything else — other keys, symlinked candidates, malformed markers, unmarked
/// non-P0 directories — is enumerated out and never returned for deletion.
fn gather_candidates(
    fs: &dyn SiteFs,
    parent: &Path,
    key: &str,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>), BuildError> {
    let mut stale_staging = Vec::new();
    let mut valid_backups = Vec::new();

    for entry in fs.entries(parent)? {
        // A non-UTF-8 name can never be one of our fixed lowercase-hex candidates.
        let Some(name) = entry.name.to_str() else {
            continue;
        };
        let Some(candidate) = classify_for_key(name, key) else {
            continue;
        };
        // The candidate itself must be a REAL directory; a symlink (or reparse
        // point) sharing a candidate name is unrelated and is never traversed.
        if entry.kind != EntryKind::Dir {
            continue;
        }
        let path = parent.join(&entry.name);
        match candidate {
            Candidate::Staging => {
                if let StagingClass::Stale = classify_staging(fs, &path, key)? {
                    stale_staging.push(path);
                }
            }
            Candidate::Backup => {
                if is_valid_backup(fs, &path)? {
                    valid_backups.push(path);
                }
            }
        }
    }
    Ok((stale_staging, valid_backups))
}

/// Classifies a keyed staging directory as owned-stale (A/B/P0) or preserve.
fn classify_staging(fs: &dyn SiteFs, dir: &Path, key: &str) -> Result<StagingClass, BuildError> {
    match fs.read_marker(dir)? {
        Some(bytes) => match Marker::parse(&bytes) {
            // Malformed authoritative marker → preserve (Row E).
            Err(_) => Ok(StagingClass::Preserve),
            Ok(marker) => match (marker.kind(), marker.output_key()) {
                // Marker A or marker B for this exact key → owned, deletable.
                (MarkerKind::Staging, Some(marker_key)) if marker_key == key => {
                    Ok(StagingClass::Stale)
                }
                (MarkerKind::Site, Some(marker_key)) if marker_key == key => {
                    Ok(StagingClass::Stale)
                }
                _ => Ok(StagingClass::Preserve),
            },
        },
        // No authoritative marker → deletable ONLY if the strict P0 shape holds.
        None => {
            let entries = fs.entries(dir)?;
            let is_p0 = entries.iter().all(|entry| {
                entry.kind == EntryKind::File
                    && entry.name.to_str().map(is_marker_temp).unwrap_or(false)
            });
            Ok(if is_p0 {
                StagingClass::Stale
            } else {
                StagingClass::Preserve
            })
        }
    }
}

/// Whether a keyed backup directory carries a valid marker C (Site, no key).
fn is_valid_backup(fs: &dyn SiteFs, dir: &Path) -> Result<bool, BuildError> {
    match fs.read_marker(dir)? {
        Some(bytes) => Ok(matches!(
            Marker::parse(&bytes).map(|m| (m.kind(), m.output_key().is_none())),
            Ok((MarkerKind::Site, true))
        )),
        None => Ok(false),
    }
}

// ---------------------------------------------------------------------------
// Transactional publication
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn publish(
    fs: &dyn SiteFs,
    artifacts: &ArtifactPaths,
    assets: &[(String, Cow<'static, [u8]>)],
    options: &SiteOptions,
    key: &str,
    output: &Path,
    parent: &Path,
    predecessor: Predecessor,
) -> Result<PublishedSite, BuildError> {
    let generated_at = &options.generated_at;
    let staging = parent.join(staging_name(key, &generate_nonce()));

    // 1. mkdir staging.
    fs.make_staging(&staging)?;
    // 2. commit marker A (the first authoritative file; before any content). A
    //    failure here leaves a strict-P0 shell we just created — safe to remove.
    if let Err(error) = fs.commit_marker(
        &staging,
        &Marker::staging_at(key, generated_at),
        MarkerStage::A,
    ) {
        let _ = fs.cleanup(&staging);
        return Err(error);
    }
    // 3. materialize all content.
    if let Err(error) = write_content(fs, artifacts, assets, options, &staging) {
        let _ = fs.cleanup(&staging);
        return Err(error);
    }
    // 4. commit marker B (complete; output_key kept).
    if let Err(error) = fs.commit_marker(
        &staging,
        &Marker::complete_at(key, generated_at),
        MarkerStage::B,
    ) {
        let _ = fs.cleanup(&staging);
        return Err(error);
    }
    // 5. make room for the rename target.
    let backup = match predecessor {
        Predecessor::Site => {
            let backup = parent.join(backup_name(key, &generate_nonce()));
            if let Err(error) = fs.rename_output_to_backup(output, &backup) {
                let _ = fs.cleanup(&staging);
                return Err(error);
            }
            Some(backup)
        }
        Predecessor::Empty => {
            if let Err(error) = fs.remove_empty_output(output) {
                let _ = fs.cleanup(&staging);
                return Err(error);
            }
            None
        }
        Predecessor::Absent => None,
    };
    // 6. rename staging -> output.
    if fs.rename_staging_to_output(&staging, output).is_err() {
        return Err(rollback(
            fs,
            &staging,
            output,
            predecessor,
            backup.as_deref(),
        ));
    }
    // 7. finalize marker B -> C. On failure, preserve output (marker B) and the
    //    backup; the next run finalizes B -> C. The backup is NOT deleted.
    if fs
        .commit_marker(output, &Marker::published_at(generated_at), MarkerStage::C)
        .is_err()
    {
        return Err(BuildError::PublishUnrecoverable);
    }
    // 8. success: delete the backup (only now that marker C exists).
    if let Some(backup) = &backup {
        let _ = fs.cleanup(backup);
    }

    Ok(PublishedSite {
        output: output.to_path_buf(),
        asset_count: assets.len(),
        base_path: options
            .base_path
            .as_ref()
            .map(|base| base.as_str().to_string())
            .filter(|value| !value.is_empty()),
    })
}

/// Rolls back a failed `staging -> output` rename, restoring the predecessor.
fn rollback(
    fs: &dyn SiteFs,
    staging: &Path,
    output: &Path,
    predecessor: Predecessor,
    backup: Option<&Path>,
) -> BuildError {
    match predecessor {
        Predecessor::Site => {
            let backup = backup.expect("a site predecessor always has a backup");
            if fs.rename_backup_to_output(backup, output).is_err() {
                // Preserve staging AND backup; nothing is deleted.
                return BuildError::PublishUnrecoverable;
            }
            let _ = fs.cleanup(staging);
            BuildError::PublishFailed
        }
        Predecessor::Empty => {
            if fs.recreate_empty_output(output).is_err() {
                // Preserve staging; nothing is deleted.
                return BuildError::PublishUnrecoverable;
            }
            let _ = fs.cleanup(staging);
            BuildError::PublishFailed
        }
        Predecessor::Absent => {
            let _ = fs.cleanup(staging);
            BuildError::PublishFailed
        }
    }
}

/// Writes every asset (transforming `index.html`) then copies the three artifacts.
fn write_content(
    fs: &dyn SiteFs,
    artifacts: &ArtifactPaths,
    assets: &[(String, Cow<'static, [u8]>)],
    options: &SiteOptions,
    staging: &Path,
) -> Result<(), BuildError> {
    for (rel, bytes) in assets {
        let target = staging_join(staging, rel);
        if rel == "index.html" {
            let html = std::str::from_utf8(bytes).map_err(|_| internal("index-not-utf8"))?;
            let transformed = transform_index_html(html, options.base_path.as_ref())?;
            fs.write_asset(&target, transformed.as_bytes())?;
        } else {
            fs.write_asset(&target, bytes)?;
        }
    }
    fs.copy_artifact(&artifacts.document, &staging.join(DOCUMENT_FILE))?;
    fs.copy_artifact(&artifacts.generation, &staging.join(GENERATION_FILE))?;
    fs.copy_artifact(&artifacts.diagnostics, &staging.join(DIAGNOSTICS_FILE))?;
    Ok(())
}

/// Joins a validated relative asset path onto `staging`, component by component (so
/// a `/`-separated asset name lands correctly on every platform).
fn staging_join(staging: &Path, rel: &str) -> PathBuf {
    let mut path = staging.to_path_buf();
    for component in rel.split('/') {
        path.push(component);
    }
    path
}

/// Validates the asset set (pure) before any filesystem work. Invalid input maps to
/// the internal `build_filesystem_error` — never a new public error code.
fn validate_assets(assets: &[(String, Cow<'static, [u8]>)]) -> Result<(), BuildError> {
    if assets.is_empty() {
        return Err(internal("assets-empty"));
    }
    let mut seen = HashSet::new();
    let mut index_count = 0usize;
    for (rel, _) in assets {
        if !is_relative_normalized(rel) {
            return Err(internal("asset-path-invalid"));
        }
        if !seen.insert(rel.as_str()) {
            return Err(internal("asset-path-duplicate"));
        }
        if RESERVED_NAMES.contains(&rel.as_str()) {
            return Err(internal("asset-reserved-name"));
        }
        if rel == "index.html" {
            index_count += 1;
        }
    }
    if index_count != 1 {
        return Err(internal("asset-index-count"));
    }
    Ok(())
}

/// Whether `rel` is a relative, `/`-separated, traversal-free asset path.
fn is_relative_normalized(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.starts_with('/')
        && !rel.contains('\\')
        && rel
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn internal(context: &'static str) -> BuildError {
    BuildError::Filesystem { context }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::fs;
    use tempfile::TempDir;

    use crate::static_site::lock::OutputLock;
    use crate::static_site::marker::{MARKER_FILENAME, write_marker_real};
    use crate::static_site::nonce::{lock_name, staging_name};
    use crate::static_site::output_identity::{ResolvedOutput, resolve_output, resolve_output_key};

    const GENERATED_AT: &str = "2026-07-18T00:00:00Z";
    const INDEX: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<link rel=\"stylesheet\" href=\"./assets/index.aa11bb22.css\"></head>\
<body><script src=\"./assets/index.cc33dd44.js\"></script></body></html>";
    const APP_JS: &[u8] = b"console.log('cratevista');";
    const CSS: &[u8] = b"body{color:#000}";
    const DOC_BYTES: &[u8] = b"{\"document\":1}";
    const GEN_BYTES: &[u8] = b"{\"generation\":1}";
    const DIAG_BYTES: &[u8] = b"{\"diagnostics\":1}";

    // --- fixtures ----------------------------------------------------------

    fn write_artifacts(root: &Path) -> ArtifactPaths {
        let dir = root.join("artifacts");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(DOCUMENT_FILE), DOC_BYTES).unwrap();
        fs::write(dir.join(GENERATION_FILE), GEN_BYTES).unwrap();
        fs::write(dir.join(DIAGNOSTICS_FILE), DIAG_BYTES).unwrap();
        ArtifactPaths::in_dir(&dir)
    }

    fn assets() -> Vec<(String, Cow<'static, [u8]>)> {
        vec![
            ("index.html".to_string(), Cow::Borrowed(INDEX.as_bytes())),
            (
                "assets/index.cc33dd44.js".to_string(),
                Cow::Borrowed(APP_JS),
            ),
            ("assets/index.aa11bb22.css".to_string(), Cow::Borrowed(CSS)),
        ]
    }

    fn options(output: &Path, base: Option<&str>) -> SiteOptions {
        SiteOptions {
            output: output.to_path_buf(),
            base_path: base.map(|b| BasePath::parse(b).unwrap()),
            generated_at: GENERATED_AT.to_string(),
        }
    }

    fn safety_for(output: &Path) -> OutputSafety {
        OutputSafety::for_output(output, vec![]).unwrap()
    }

    /// The canonical (output, parent, key) triple a build will compute.
    fn resolved_parts(output: &Path) -> (PathBuf, PathBuf, String) {
        let resolved = resolve_output(output).unwrap();
        (
            resolved.path().to_path_buf(),
            resolved.parent().unwrap().to_path_buf(),
            output_key(&resolved),
        )
    }

    fn run(
        fs: &dyn SiteFs,
        output: &Path,
        base: Option<&str>,
        art: &ArtifactPaths,
    ) -> Result<PublishedSite, BuildError> {
        run_materialize(
            fs,
            art,
            &assets(),
            &options(output, base),
            &safety_for(output),
        )
    }

    fn keyed_names(parent: &Path, key: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(parent)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| classify_for_key(name, key).is_some())
            .collect();
        names.sort();
        names
    }

    /// Builds an existing marker-C site at `output` with the given extra files.
    fn make_site_c(output: &Path, files: &[(&str, &[u8])]) {
        fs::create_dir_all(output).unwrap();
        for (name, bytes) in files {
            fs::write(output.join(name), bytes).unwrap();
        }
        write_marker_real(output, &Marker::published_at("2026-01-01T00:00:00Z")).unwrap();
    }

    fn read_output_marker(output: &Path) -> Marker {
        let bytes = fs::read(output.join(MARKER_FILENAME)).unwrap();
        Marker::parse(&bytes).unwrap()
    }

    // --- the fault-injection double ---------------------------------------

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Fault {
        ParentCreate,
        Reresolve,
        Enumerate,
        StagingMkdir,
        MarkerA,
        MarkerB,
        MarkerC,
        AssetWrite,
        ArtifactCopy,
        OutputToBackup,
        StagingToOutput,
        BackupRestore,
        EmptyRecreate,
        Cleanup,
    }

    struct FaultFs {
        real: RealSiteFs,
        faults: Vec<Fault>,
        resolve_count: Cell<u32>,
        second_resolve: Option<ResolvedOutput>,
        before_reresolve: RefCell<Option<Box<dyn FnMut()>>>,
    }

    impl FaultFs {
        fn none() -> FaultFs {
            FaultFs {
                real: RealSiteFs,
                faults: Vec::new(),
                resolve_count: Cell::new(0),
                second_resolve: None,
                before_reresolve: RefCell::new(None),
            }
        }
        fn failing(faults: &[Fault]) -> FaultFs {
            FaultFs {
                faults: faults.to_vec(),
                ..FaultFs::none()
            }
        }
        fn hit(&self, fault: Fault) -> bool {
            self.faults.contains(&fault)
        }
    }

    fn ferr(context: &'static str) -> BuildError {
        BuildError::Filesystem { context }
    }

    impl SiteFs for FaultFs {
        fn resolve(&self, output: &Path) -> Result<ResolvedOutput, BuildError> {
            let n = self.resolve_count.get() + 1;
            self.resolve_count.set(n);
            if n == 2 {
                if let Some(hook) = self.before_reresolve.borrow_mut().as_mut() {
                    hook();
                }
                if self.hit(Fault::Reresolve) {
                    return Err(ferr("reresolve"));
                }
                if let Some(resolved) = &self.second_resolve {
                    return Ok(resolved.clone());
                }
            }
            self.real.resolve(output)
        }
        fn create_parents(&self, path: &Path) -> Result<(), BuildError> {
            if self.hit(Fault::ParentCreate) {
                return Err(ferr("parent"));
            }
            self.real.create_parents(path)
        }
        fn kind(&self, path: &Path) -> Result<Option<EntryKind>, BuildError> {
            self.real.kind(path)
        }
        fn entries(&self, dir: &Path) -> Result<Vec<super::super::fs_seam::DirEntry>, BuildError> {
            if self.hit(Fault::Enumerate) {
                return Err(ferr("enumerate"));
            }
            self.real.entries(dir)
        }
        fn read_marker(&self, dir: &Path) -> Result<Option<Vec<u8>>, BuildError> {
            self.real.read_marker(dir)
        }
        fn make_staging(&self, path: &Path) -> Result<(), BuildError> {
            if self.hit(Fault::StagingMkdir) {
                return Err(ferr("staging-mkdir"));
            }
            self.real.make_staging(path)
        }
        fn write_asset(&self, path: &Path, bytes: &[u8]) -> Result<(), BuildError> {
            if self.hit(Fault::AssetWrite) {
                return Err(ferr("asset-write"));
            }
            self.real.write_asset(path, bytes)
        }
        fn copy_artifact(&self, from: &Path, to: &Path) -> Result<(), BuildError> {
            if self.hit(Fault::ArtifactCopy) {
                return Err(ferr("artifact-copy"));
            }
            self.real.copy_artifact(from, to)
        }
        fn commit_marker(
            &self,
            dir: &Path,
            marker: &Marker,
            stage: MarkerStage,
        ) -> Result<(), BuildError> {
            let targeted = match stage {
                MarkerStage::A => Fault::MarkerA,
                MarkerStage::B => Fault::MarkerB,
                MarkerStage::C => Fault::MarkerC,
            };
            if self.hit(targeted) {
                return Err(ferr("marker"));
            }
            self.real.commit_marker(dir, marker, stage)
        }
        fn rename_output_to_backup(&self, output: &Path, backup: &Path) -> Result<(), BuildError> {
            if self.hit(Fault::OutputToBackup) {
                return Err(ferr("output-to-backup"));
            }
            self.real.rename_output_to_backup(output, backup)
        }
        fn remove_empty_output(&self, output: &Path) -> Result<(), BuildError> {
            self.real.remove_empty_output(output)
        }
        fn rename_staging_to_output(
            &self,
            staging: &Path,
            output: &Path,
        ) -> Result<(), BuildError> {
            if self.hit(Fault::StagingToOutput) {
                return Err(ferr("staging-to-output"));
            }
            self.real.rename_staging_to_output(staging, output)
        }
        fn rename_backup_to_output(&self, backup: &Path, output: &Path) -> Result<(), BuildError> {
            if self.hit(Fault::BackupRestore) {
                return Err(ferr("backup-restore"));
            }
            self.real.rename_backup_to_output(backup, output)
        }
        fn recreate_empty_output(&self, output: &Path) -> Result<(), BuildError> {
            if self.hit(Fault::EmptyRecreate) {
                return Err(ferr("empty-recreate"));
            }
            self.real.recreate_empty_output(output)
        }
        fn cleanup(&self, path: &Path) -> Result<(), BuildError> {
            if self.hit(Fault::Cleanup) {
                return Err(ferr("cleanup"));
            }
            self.real.cleanup(path)
        }
    }

    // =====================================================================
    // Parent preparation and locking
    // =====================================================================

    #[test]
    fn existing_parent_normal_path_publishes() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let art = write_artifacts(temp.path());
        let published = run(&RealSiteFs, &output, None, &art).unwrap();
        assert_eq!(published.asset_count(), 3);
        assert!(output.join("index.html").is_file());
        assert!(output.join(MARKER_FILENAME).is_file());
    }

    #[test]
    fn nested_missing_parents_are_created_with_a_stable_key() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("a/b/c/site");
        let key_before = resolve_output_key(&output).unwrap();
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        // The whole missing chain was created, <output> published, key unchanged.
        assert!(output.join("index.html").is_file());
        let key_after = resolve_output_key(&output).unwrap();
        assert_eq!(key_before, key_after);
    }

    #[test]
    fn reresolution_identity_mismatch_fails_before_any_scan_or_mutation() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        let art = write_artifacts(temp.path());
        let mut fs = FaultFs::none();
        // The re-resolve returns a different identity → key/identity guard fires.
        fs.second_resolve = Some(ResolvedOutput::from_path_for_test(parent.join("elsewhere")));
        let result = run(&fs, &output, None, &art);
        assert!(matches!(result, Err(BuildError::Filesystem { .. })));
        // No staging/backup was created and the output was never touched.
        assert!(keyed_names(&parent, &key).is_empty());
        assert!(!output.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_introduced_during_preparation_fails_before_scan() {
        use std::os::unix::fs::symlink;
        let temp = TempDir::new().unwrap();
        let real_parent = temp.path().join("real");
        fs::create_dir_all(&real_parent).unwrap();
        let output = temp.path().join("link/site");
        let art = write_artifacts(temp.path());
        // Before the second resolve, replace the (missing) parent with a symlink.
        let link = temp.path().join("link");
        let mut fs = FaultFs::none();
        let real_clone = real_parent.clone();
        *fs.before_reresolve.borrow_mut() = Some(Box::new(move || {
            let _ = symlink(&real_clone, &link);
        }));
        let result = run(&fs, &output, None, &art);
        assert_eq!(result, Err(BuildError::OutputSymlink));
    }

    #[test]
    fn same_output_contention_returns_busy_without_mutation() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        let art = write_artifacts(temp.path());
        // A concurrent build already holds the lock for this key.
        let _held = OutputLock::acquire(&parent, &key).unwrap();
        let result = run(&RealSiteFs, &output, None, &art);
        assert_eq!(result, Err(BuildError::OutputBusy));
        // No staging/backup was created and the output was never touched; only the
        // lock file (created by the held lock above) is present.
        assert!(keyed_names(&parent, &key).is_empty());
        assert!(parent.join(lock_name(&key)).is_file());
        assert!(!output.exists());
    }

    #[test]
    fn different_nested_outputs_use_different_locks() {
        let temp = TempDir::new().unwrap();
        let art = write_artifacts(temp.path());
        let site_a = temp.path().join("group/a/site");
        let site_b = temp.path().join("group/b/site");
        run(&RealSiteFs, &site_a, None, &art).unwrap();
        // Holding a's lock does not block b (different key, and different parent).
        let (_o, parent_a, key_a) = resolved_parts(&site_a);
        let _held = OutputLock::acquire(&parent_a, &key_a).unwrap();
        run(&RealSiteFs, &site_b, None, &art).unwrap();
        assert!(site_a.join("index.html").is_file());
        assert!(site_b.join("index.html").is_file());
    }

    #[test]
    fn forged_output_key_is_rejected_before_lock_or_recovery() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        let art = write_artifacts(temp.path());
        // A safety context carrying a key that does not match the output.
        let forged = OutputSafety::from_parts(vec![], "ffffffffffffffff".to_string());
        let result = run_materialize(
            &RealSiteFs,
            &art,
            &assets(),
            &options(&output, None),
            &forged,
        );
        assert!(matches!(result, Err(BuildError::Filesystem { .. })));
        // Neither the (real) lock nor any staging/backup for the true key appeared.
        assert!(keyed_names(&parent, &key).is_empty());
        assert!(!output.exists());
    }

    // =====================================================================
    // P0 pre-marker shells
    // =====================================================================

    /// Places a keyed staging directory with the given entries and no marker.
    fn make_bare_staging(parent: &Path, key: &str, entries: &[(&str, EntryShape)]) -> PathBuf {
        let path = parent.join(staging_name(key, &"a".repeat(32)));
        fs::create_dir_all(&path).unwrap();
        for (name, shape) in entries {
            match shape {
                EntryShape::File(bytes) => fs::write(path.join(name), bytes).unwrap(),
                EntryShape::Dir => fs::create_dir_all(path.join(name)).unwrap(),
                #[cfg(unix)]
                EntryShape::Symlink => {
                    use std::os::unix::fs::symlink;
                    symlink(parent, path.join(name)).unwrap();
                }
            }
        }
        path
    }

    enum EntryShape {
        File(&'static [u8]),
        Dir,
        #[cfg(unix)]
        Symlink,
    }

    fn marker_temp_name() -> String {
        format!("{MARKER_FILENAME}.tmp-{}", "b".repeat(32))
    }

    #[test]
    fn an_empty_p0_shell_is_removed_during_recovery() {
        // Crash right after mkdir, before any marker-temp.
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        let shell = make_bare_staging(&parent, &key, &[]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(!shell.exists(), "an empty P0 shell must be removed");
        assert!(output.join("index.html").is_file());
    }

    #[test]
    fn a_temp_only_p0_shell_is_removed_during_recovery() {
        // Crash during the marker-temp write, before the rename-over commits marker A.
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        let name = marker_temp_name();
        let shell = make_bare_staging(&parent, &key, &[(name.as_str(), EntryShape::File(b"half"))]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(!shell.exists(), "a temp-only P0 shell must be removed");
        assert!(output.join("index.html").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn a_staging_shell_with_a_symlinked_entry_is_preserved() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        // A marker-temp-named entry that is a SYMLINK, not a regular file → not P0.
        let name = marker_temp_name();
        let shell = make_bare_staging(&parent, &key, &[(name.as_str(), EntryShape::Symlink)]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(
            shell.exists(),
            "a symlinked entry defeats the strict P0 shape"
        );
    }

    #[test]
    fn a_staging_shell_with_user_content_is_preserved() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        // An unmarked keyed staging dir that holds a real file is NOT P0.
        let shell = make_bare_staging(&parent, &key, &[("keep.txt", EntryShape::File(b"mine"))]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(shell.exists(), "a non-P0 shell must be preserved");
        assert!(shell.join("keep.txt").is_file());
    }

    #[test]
    fn a_staging_shell_with_a_subdirectory_is_preserved() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        let shell = make_bare_staging(&parent, &key, &[("sub", EntryShape::Dir)]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(shell.exists(), "a shell with a subdirectory is not P0");
    }

    #[test]
    fn a_p0_shell_for_another_output_key_is_untouched() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, _key) = resolved_parts(&output);
        // A P0-shaped shell keyed to a DIFFERENT output.
        let other_key = "abcdef0123456789";
        let foreign = make_bare_staging(&parent, other_key, &[]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(foreign.exists(), "another key's shell must be untouched");
    }

    // --- more candidate fixtures ------------------------------------------

    fn make_staging_with_marker(parent: &Path, key: &str, marker: &Marker) -> PathBuf {
        let path = parent.join(staging_name(key, &generate_nonce()));
        fs::create_dir_all(&path).unwrap();
        write_marker_real(&path, marker).unwrap();
        path
    }

    fn make_backup(parent: &Path, key: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let path = parent.join(backup_name(key, &generate_nonce()));
        fs::create_dir_all(&path).unwrap();
        for (name, bytes) in files {
            fs::write(path.join(name), bytes).unwrap();
        }
        write_marker_real(&path, &Marker::published_at("2026-01-01T00:00:00Z")).unwrap();
        path
    }

    fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
        fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            for entry in fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else {
                    let rel = path
                        .strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .to_string();
                    out.push((rel, fs::read(&path).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    // =====================================================================
    // Interrupted-publication recovery (rows A–F)
    // =====================================================================

    #[test]
    fn row_a_stable_output_drops_this_keys_stale_staging_and_backups() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        make_site_c(&output, &[("old.txt", b"old")]);
        let stale_staging = make_staging_with_marker(&parent, &key, &Marker::staging_at(&key, "t"));
        let stale_backup = make_backup(&parent, &key, &[("b.txt", b"b")]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        // The new site replaced the old; both stale candidates are gone; no leftovers.
        assert!(output.join("index.html").is_file());
        assert!(!output.join("old.txt").exists());
        assert!(!stale_staging.exists());
        assert!(!stale_backup.exists());
        assert!(keyed_names(&parent, &key).is_empty());
    }

    #[test]
    fn row_b_absent_output_with_one_backup_restores_before_publishing() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        // One valid backup carrying a sentinel; a stale staging alongside it.
        make_backup(&parent, &key, &[("restored.txt", b"restored")]);
        make_staging_with_marker(&parent, &key, &Marker::staging_at(&key, "t"));
        let art = write_artifacts(temp.path());
        // Stop right after recovery (fail marker A) so the restored predecessor is
        // observable before publication would overwrite it.
        let fs = FaultFs::failing(&[Fault::MarkerA]);
        let result = run(&fs, &output, None, &art);
        assert!(result.is_err());
        // The backup was restored to <output> first; the stale staging was removed.
        assert!(output.join("restored.txt").is_file());
        assert_eq!(read_output_marker(&output).output_key(), None); // finalized C
        let leftovers = keyed_names(&parent, &key);
        assert!(
            leftovers.is_empty(),
            "backups/staging cleaned: {leftovers:?}"
        );
    }

    #[test]
    fn row_c_absent_output_with_multiple_backups_is_ambiguous_and_preserves_all() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        let backup_one = make_backup(&parent, &key, &[("one.txt", b"1")]);
        let backup_two = make_backup(&parent, &key, &[("two.txt", b"2")]);
        let art = write_artifacts(temp.path());
        let result = run(&RealSiteFs, &output, None, &art);
        assert_eq!(result, Err(BuildError::RecoveryAmbiguous));
        assert!(backup_one.exists() && backup_two.exists());
        assert!(!output.exists());
    }

    #[test]
    fn row_d_absent_output_no_backup_drops_stale_staging_and_builds_fresh() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        let stale = make_staging_with_marker(&parent, &key, &Marker::staging_at(&key, "t"));
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(!stale.exists());
        assert!(output.join("index.html").is_file());
    }

    #[test]
    fn row_e_invalid_candidates_are_never_touched() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        // A staging with a mismatched-key marker, and one with a malformed marker.
        let mismatched =
            make_staging_with_marker(&parent, &key, &Marker::staging_at("aaaaaaaaaaaaaaaa", "t"));
        let malformed = parent.join(staging_name(&key, &generate_nonce()));
        fs::create_dir_all(&malformed).unwrap();
        fs::write(malformed.join(MARKER_FILENAME), b"not json").unwrap();
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(mismatched.exists(), "mismatched-key staging preserved");
        assert!(malformed.exists(), "malformed-marker staging preserved");
    }

    #[test]
    fn row_f_backup_restoration_failure_is_unrecoverable_and_preserves_paths() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        let backup = make_backup(&parent, &key, &[("restored.txt", b"r")]);
        let art = write_artifacts(temp.path());
        let fs = FaultFs::failing(&[Fault::BackupRestore]);
        let result = run(&fs, &output, None, &art);
        assert_eq!(result, Err(BuildError::PublishUnrecoverable));
        assert!(backup.exists(), "the backup must be preserved");
        assert!(!output.exists());
    }

    #[test]
    fn a_matching_marker_b_at_output_is_finalized_to_c() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, _parent, key) = resolved_parts(&output);
        // <output> carries marker B (post-rename/pre-finalization crash), matching key.
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("published.txt"), b"published").unwrap();
        write_marker_real(&output, &Marker::complete_at(&key, "t")).unwrap();
        let art = write_artifacts(temp.path());
        // Stop after recovery so we can observe the finalized C at <output>.
        let fs = FaultFs::failing(&[Fault::MarkerA]);
        let _ = run(&fs, &output, None, &art);
        assert!(output.join("published.txt").is_file(), "output preserved");
        assert_eq!(
            read_output_marker(&output).output_key(),
            None,
            "finalized to C"
        );
    }

    #[test]
    fn a_mismatched_marker_b_at_output_fails_untouched() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("keep.txt"), b"keep").unwrap();
        write_marker_real(&output, &Marker::complete_at("aaaaaaaaaaaaaaaa", "t")).unwrap();
        let art = write_artifacts(temp.path());
        let result = run(&RealSiteFs, &output, None, &art);
        assert!(matches!(
            result,
            Err(BuildError::OutputMarkerInvalid { .. })
        ));
        assert!(output.join("keep.txt").is_file());
    }

    #[test]
    fn a_completed_marker_b_staging_is_discarded_never_promoted() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        // A completed marker-B staging with distinctive content.
        let staging = make_staging_with_marker(&parent, &key, &Marker::complete_at(&key, "t"));
        fs::write(staging.join("stale-candidate.txt"), b"stale").unwrap();
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        // The candidate was discarded, not published; a fresh site was built.
        assert!(!staging.exists());
        assert!(output.join("index.html").is_file());
        assert!(!output.join("stale-candidate.txt").exists());
    }

    // =====================================================================
    // Cross-output isolation
    // =====================================================================

    #[test]
    fn a_build_never_sees_another_outputs_candidates() {
        let temp = TempDir::new().unwrap();
        let site_a = temp.path().join("site-a");
        let site_b = temp.path().join("site-b");
        let (_oa, parent, key_a) = resolved_parts(&site_a);
        let (_ob, _pb, key_b) = resolved_parts(&site_b);
        // site-b's staging and backup live under the same parent.
        let b_staging = make_staging_with_marker(&parent, &key_b, &Marker::staging_at(&key_b, "t"));
        let b_backup = make_backup(&parent, &key_b, &[("b.txt", b"b")]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &site_a, None, &art).unwrap();
        assert!(b_staging.exists(), "site-b staging untouched by site-a");
        assert!(b_backup.exists(), "site-b backup untouched by site-a");
        assert!(!key_a.is_empty());
    }

    #[test]
    fn a_backup_for_another_output_is_not_restored() {
        let temp = TempDir::new().unwrap();
        let site_a = temp.path().join("site-a");
        let site_b = temp.path().join("site-b");
        let (_oa, parent, _key_a) = resolved_parts(&site_a);
        let (_ob, _pb, key_b) = resolved_parts(&site_b);
        // Only a site-b backup exists; site-a's output is absent.
        let b_backup = make_backup(&parent, &key_b, &[("b.txt", b"b")]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &site_a, None, &art).unwrap();
        // site-a built fresh (Row D); site-b's backup was never restored to site-a.
        assert!(site_a.join("index.html").is_file());
        assert!(!site_a.join("b.txt").exists());
        assert!(b_backup.exists());
    }

    // =====================================================================
    // Output ownership
    // =====================================================================

    #[test]
    fn a_non_empty_unowned_output_is_refused_untouched() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("theirs.txt"), b"theirs").unwrap();
        let art = write_artifacts(temp.path());
        let result = run(&RealSiteFs, &output, None, &art);
        assert_eq!(result, Err(BuildError::OutputNotOwned));
        assert!(output.join("theirs.txt").is_file());
    }

    #[test]
    fn a_malformed_output_marker_is_refused_untouched() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join(MARKER_FILENAME), b"{ not valid").unwrap();
        let art = write_artifacts(temp.path());
        let result = run(&RealSiteFs, &output, None, &art);
        assert!(matches!(
            result,
            Err(BuildError::OutputMarkerInvalid { .. })
        ));
        assert!(output.join(MARKER_FILENAME).is_file());
    }

    #[test]
    fn an_empty_output_directory_is_adopted() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        fs::create_dir_all(&output).unwrap();
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(output.join("index.html").is_file());
        assert_eq!(read_output_marker(&output).output_key(), None);
    }

    #[test]
    fn empty_output_publish_failure_recreates_emptiness() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        fs::create_dir_all(&output).unwrap();
        let art = write_artifacts(temp.path());
        let fs = FaultFs::failing(&[Fault::StagingToOutput]);
        let result = run(&fs, &output, None, &art);
        assert_eq!(result, Err(BuildError::PublishFailed));
        assert!(output.is_dir(), "empty output recreated");
        assert!(
            fs::read_dir(&output).unwrap().next().is_none(),
            "output is empty"
        );
    }

    #[test]
    fn empty_output_recreation_failure_preserves_staging() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        fs::create_dir_all(&output).unwrap();
        let art = write_artifacts(temp.path());
        let fs = FaultFs::failing(&[Fault::StagingToOutput, Fault::EmptyRecreate]);
        let result = run(&fs, &output, None, &art);
        assert_eq!(result, Err(BuildError::PublishUnrecoverable));
        // The complete staging (marker B) is preserved for manual recovery.
        let leftovers = keyed_names(&parent, &key);
        assert!(
            leftovers.iter().any(|n| n.contains("-staging-")),
            "staging preserved: {leftovers:?}"
        );
    }

    #[test]
    fn an_existing_owned_site_is_byte_identical_after_a_pre_publication_failure() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        make_site_c(
            &output,
            &[("existing.txt", b"existing"), ("index.html", b"OLD")],
        );
        let before = snapshot(&output);
        let art = write_artifacts(temp.path());
        // Fail while writing assets — before the output is ever touched.
        let fs = FaultFs::failing(&[Fault::AssetWrite]);
        let result = run(&fs, &output, None, &art);
        assert!(result.is_err());
        assert_eq!(
            before,
            snapshot(&output),
            "the existing site must be untouched"
        );
    }

    #[test]
    fn a_successful_rebuild_contains_no_stale_old_files() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        make_site_c(&output, &[("stale.txt", b"stale")]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(
            !output.join("stale.txt").exists(),
            "old file must not survive"
        );
        assert!(output.join("index.html").is_file());
    }

    #[test]
    fn staging_rename_failure_restores_the_previous_site() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        make_site_c(&output, &[("sentinel.txt", b"sentinel")]);
        let before = snapshot(&output);
        let art = write_artifacts(temp.path());
        let fs = FaultFs::failing(&[Fault::StagingToOutput]);
        let result = run(&fs, &output, None, &art);
        assert_eq!(result, Err(BuildError::PublishFailed));
        assert_eq!(
            before,
            snapshot(&output),
            "predecessor restored byte-identical"
        );
    }

    #[test]
    fn marker_c_failure_preserves_output_b_and_backup_then_next_run_finalizes() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        make_site_c(&output, &[("v1.txt", b"v1")]);
        let art = write_artifacts(temp.path());
        // First run fails to finalize C after the publish rename.
        let fs = FaultFs::failing(&[Fault::MarkerC]);
        let result = run(&fs, &output, None, &art);
        assert_eq!(result, Err(BuildError::PublishUnrecoverable));
        // <output> now carries the new site with marker B (matching key) …
        assert!(output.join("index.html").is_file());
        assert_eq!(read_output_marker(&output).output_key().unwrap(), key);
        // … and the backup is preserved (never deleted before C).
        let leftovers = keyed_names(&parent, &key);
        assert!(
            leftovers.iter().any(|n| n.contains("-backup-")),
            "backup preserved: {leftovers:?}"
        );
        // The next run finalizes B -> C and keeps the newly published output.
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert_eq!(read_output_marker(&output).output_key(), None, "now C");
        assert!(
            keyed_names(&parent, &key).is_empty(),
            "backup cleaned after C"
        );
    }

    // =====================================================================
    // Content
    // =====================================================================

    #[test]
    fn exact_asset_and_artifact_bytes_are_written() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert_eq!(
            fs::read(output.join("assets/index.cc33dd44.js")).unwrap(),
            APP_JS
        );
        assert_eq!(
            fs::read(output.join("assets/index.aa11bb22.css")).unwrap(),
            CSS
        );
        assert_eq!(fs::read(output.join(DOCUMENT_FILE)).unwrap(), DOC_BYTES);
        assert_eq!(fs::read(output.join(GENERATION_FILE)).unwrap(), GEN_BYTES);
        assert_eq!(fs::read(output.join(DIAGNOSTICS_FILE)).unwrap(), DIAG_BYTES);
    }

    #[test]
    fn the_published_output_carries_marker_c_without_a_key() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        let marker = read_output_marker(&output);
        assert_eq!(marker.kind(), MarkerKind::Site);
        assert_eq!(marker.output_key(), None);
    }

    #[test]
    fn index_carries_the_static_meta_once_and_base_only_when_supplied() {
        let temp = TempDir::new().unwrap();
        let art = write_artifacts(temp.path());

        let plain = temp.path().join("plain");
        run(&RealSiteFs, &plain, None, &art).unwrap();
        let plain_html = fs::read_to_string(plain.join("index.html")).unwrap();
        assert_eq!(plain_html.matches(r#"name="cratevista-mode""#).count(), 1);
        assert!(!plain_html.contains("<base "));
        // Relative asset references survive the transform untouched.
        assert!(plain_html.contains(r#"href="./assets/index.aa11bb22.css""#));
        assert!(plain_html.contains(r#"src="./assets/index.cc33dd44.js""#));

        let based = temp.path().join("based");
        run(&RealSiteFs, &based, Some("/demo/"), &art).unwrap();
        let based_html = fs::read_to_string(based.join("index.html")).unwrap();
        assert_eq!(based_html.matches("<base ").count(), 1);
        assert!(based_html.contains(r#"<base href="/demo/" />"#));
    }

    #[test]
    fn the_published_directory_contains_exactly_the_contract_entries() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        let mut top: Vec<String> = fs::read_dir(&output)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        top.sort();
        assert_eq!(
            top,
            vec![
                ".cratevista-static-site.json".to_string(),
                "assets".to_string(),
                "diagnostics.json".to_string(),
                "document.json".to_string(),
                "generation.json".to_string(),
                "index.html".to_string(),
            ]
        );
    }

    #[test]
    fn invalid_asset_sets_are_rejected() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let art = write_artifacts(temp.path());
        let opts = options(&output, None);
        let safety = safety_for(&output);
        let idx = || ("index.html".to_string(), Cow::Borrowed(INDEX.as_bytes()));

        // Empty set.
        assert!(run_materialize(&RealSiteFs, &art, &[], &opts, &safety).is_err());
        // No index.html.
        let no_index = vec![("a.js".to_string(), Cow::Borrowed(APP_JS))];
        assert!(run_materialize(&RealSiteFs, &art, &no_index, &opts, &safety).is_err());
        // Duplicate index.html.
        let dup = vec![idx(), idx()];
        assert!(run_materialize(&RealSiteFs, &art, &dup, &opts, &safety).is_err());
        // Reserved-name collision.
        let reserved = vec![idx(), (DOCUMENT_FILE.to_string(), Cow::Borrowed(APP_JS))];
        assert!(run_materialize(&RealSiteFs, &art, &reserved, &opts, &safety).is_err());
        // Marker-name collision.
        let marker = vec![idx(), (MARKER_FILENAME.to_string(), Cow::Borrowed(APP_JS))];
        assert!(run_materialize(&RealSiteFs, &art, &marker, &opts, &safety).is_err());
        // Path traversal.
        let traverse = vec![idx(), ("../evil.js".to_string(), Cow::Borrowed(APP_JS))];
        assert!(run_materialize(&RealSiteFs, &art, &traverse, &opts, &safety).is_err());
        // Absolute path.
        let absolute = vec![idx(), ("/evil.js".to_string(), Cow::Borrowed(APP_JS))];
        assert!(run_materialize(&RealSiteFs, &art, &absolute, &opts, &safety).is_err());
        // Backslash.
        let backslash = vec![idx(), ("a\\b.js".to_string(), Cow::Borrowed(APP_JS))];
        assert!(run_materialize(&RealSiteFs, &art, &backslash, &opts, &safety).is_err());
        // A well-formed set still succeeds afterwards.
        assert!(run_materialize(&RealSiteFs, &art, &assets(), &opts, &safety).is_ok());
    }

    #[test]
    fn a_successful_build_leaves_no_staging_or_backup() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let (_out, parent, key) = resolved_parts(&output);
        make_site_c(&output, &[("old.txt", b"old")]);
        let art = write_artifacts(temp.path());
        run(&RealSiteFs, &output, None, &art).unwrap();
        assert!(
            keyed_names(&parent, &key).is_empty(),
            "no leftovers after success"
        );
    }

    // =====================================================================
    // Subprocess: the advisory lock is released on process termination
    // =====================================================================

    #[test]
    fn subprocess_lock_released_on_process_kill() {
        use std::process::{Command, Stdio};
        use std::time::Duration;

        let key = "0123456789abcdef";

        // Child mode: acquire the lock, signal readiness via a file, hold until killed.
        if std::env::var("CV_LOCK_CHILD").is_ok() {
            let dir = PathBuf::from(std::env::var("CV_LOCK_DIR").unwrap());
            let _lock = OutputLock::acquire(&dir, key).expect("child acquires the lock");
            fs::write(dir.join("CHILD_READY"), b"1").unwrap();
            // Bounded so an orphaned child cannot linger indefinitely.
            std::thread::sleep(Duration::from_secs(30));
            return;
        }

        let temp = TempDir::new().unwrap();
        let dir = temp.path().to_path_buf();
        let ready_file = dir.join("CHILD_READY");
        let test_path = {
            let full = module_path!(); // cratevista_core::static_site::materialize::tests
            let stripped = full.split_once("::").map(|(_, rest)| rest).unwrap_or(full);
            format!("{stripped}::subprocess_lock_released_on_process_kill")
        };

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([test_path.as_str(), "--exact", "--test-threads=1"])
            .env("CV_LOCK_CHILD", "1")
            .env("CV_LOCK_DIR", &dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn child");

        // Wait (bounded) for the child to signal it holds the lock.
        let mut ready = false;
        for _ in 0..500 {
            if ready_file.exists() {
                ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(ready, "child never reported holding the lock");

        // While the child lives, the lock is contended.
        assert_eq!(
            OutputLock::acquire(&dir, key).err(),
            Some(BuildError::OutputBusy),
            "the lock must be held by the live child"
        );

        // Killing the child terminates it; the OS releases the advisory lock.
        child.kill().unwrap();
        child.wait().unwrap();

        // Retry briefly: handle close after kill can lag slightly on some platforms.
        let mut reacquired = false;
        for _ in 0..100 {
            if OutputLock::acquire(&dir, key).is_ok() {
                reacquired = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            reacquired,
            "the lock must be released once the process dies"
        );
    }
}
