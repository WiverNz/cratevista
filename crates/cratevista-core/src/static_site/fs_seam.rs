//! The narrow filesystem seam for static-site publication (PRD 10, Phase 2B).
//!
//! This is a **static-build-specific** seam, not a general virtual filesystem: it
//! names exactly the operations where a crash or fault must be injectable in a test
//! (`RealSiteFs` is the only production implementation). Each method corresponds to
//! one publication/recovery step, so a fault can be aimed precisely without guessing
//! at path shapes.

use std::ffi::OsString;
use std::path::Path;

use super::error::BuildError;
use super::marker::{Marker, MarkerFs, RealMarkerFs, write_marker};
use super::output_identity::{ResolvedOutput, resolve_output};

/// The kind of a directory entry as reported by `symlink_metadata` — a symlink is
/// **never** followed, so a symlinked entry is reported as [`EntryKind::Symlink`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A real directory.
    Dir,
    /// A symbolic link (or, on Windows, a reparse point) — not followed.
    Symlink,
    /// Anything else (device, socket, …).
    Other,
}

impl EntryKind {
    fn of(metadata: &std::fs::Metadata) -> EntryKind {
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            EntryKind::Symlink
        } else if file_type.is_dir() {
            EntryKind::Dir
        } else if file_type.is_file() {
            EntryKind::File
        } else {
            EntryKind::Other
        }
    }
}

/// One immediate entry of a directory: its name and its non-followed kind.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// The entry's file name (no path).
    pub name: OsString,
    /// The kind from `symlink_metadata` (symlinks not followed).
    pub kind: EntryKind,
}

/// Which marker transition a [`SiteFs::commit_marker`] call performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerStage {
    /// State A — first authoritative file in a fresh staging directory.
    A,
    /// State B — complete, not yet finalized.
    B,
    /// State C — final published site.
    C,
}

/// The injectable filesystem operations of static-site publication.
pub trait SiteFs {
    /// Resolves `<output>` to its stable identity, rejecting symlinked components.
    /// Called twice: once before, once after missing-parent creation.
    fn resolve(&self, output: &Path) -> Result<ResolvedOutput, BuildError>;

    /// Creates **only the missing parent components** of `resolved_output` — never
    /// `<output>` itself.
    fn create_parents(&self, resolved_output: &Path) -> Result<(), BuildError>;

    /// The non-followed kind of `path`, or `None` if it is absent.
    fn kind(&self, path: &Path) -> Result<Option<EntryKind>, BuildError>;

    /// The immediate entries of `dir` (candidate enumeration; P0-shape inspection).
    fn entries(&self, dir: &Path) -> Result<Vec<DirEntry>, BuildError>;

    /// The authoritative marker bytes in `dir`, or `None` if absent.
    fn read_marker(&self, dir: &Path) -> Result<Option<Vec<u8>>, BuildError>;

    /// Creates the single staging directory `path`.
    fn make_staging(&self, path: &Path) -> Result<(), BuildError>;

    /// Writes one asset to `path`, creating any missing parent directory under
    /// staging. Never creates or follows a symlink.
    fn write_asset(&self, path: &Path, bytes: &[u8]) -> Result<(), BuildError>;

    /// Copies one artifact byte-for-byte from `from` to `to`.
    fn copy_artifact(&self, from: &Path, to: &Path) -> Result<(), BuildError>;

    /// Commits a marker crash-safely (write-temp → rename-over). `stage` names the
    /// transition so a fault can target one of A/B/C.
    fn commit_marker(
        &self,
        dir: &Path,
        marker: &Marker,
        stage: MarkerStage,
    ) -> Result<(), BuildError>;

    /// Renames an owned `<output>` site to a keyed backup (make room).
    fn rename_output_to_backup(&self, output: &Path, backup: &Path) -> Result<(), BuildError>;

    /// Removes an empty `<output>` directory (adopt-empty predecessor).
    fn remove_empty_output(&self, output: &Path) -> Result<(), BuildError>;

    /// Renames staging over `<output>` (the publication rename).
    fn rename_staging_to_output(&self, staging: &Path, output: &Path) -> Result<(), BuildError>;

    /// Restores a backup back to `<output>` (rollback / recovery restore).
    fn rename_backup_to_output(&self, backup: &Path, output: &Path) -> Result<(), BuildError>;

    /// Recreates an empty `<output>` (rollback of an adopted-empty predecessor).
    fn recreate_empty_output(&self, output: &Path) -> Result<(), BuildError>;

    /// Recursively removes a staging/backup directory (cleanup).
    fn cleanup(&self, path: &Path) -> Result<(), BuildError>;
}

/// The production implementation over `std::fs`.
pub struct RealSiteFs;

impl SiteFs for RealSiteFs {
    fn resolve(&self, output: &Path) -> Result<ResolvedOutput, BuildError> {
        resolve_output(output)
    }

    fn create_parents(&self, resolved_output: &Path) -> Result<(), BuildError> {
        match resolved_output.parent() {
            Some(parent) => std::fs::create_dir_all(parent).map_err(|_| fs_err("parent-create")),
            None => Ok(()),
        }
    }

    fn kind(&self, path: &Path) -> Result<Option<EntryKind>, BuildError> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => Ok(Some(EntryKind::of(&metadata))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(fs_err("kind")),
        }
    }

    fn entries(&self, dir: &Path) -> Result<Vec<DirEntry>, BuildError> {
        let mut out = Vec::new();
        let reader = std::fs::read_dir(dir).map_err(|_| fs_err("enumerate"))?;
        for entry in reader {
            let entry = entry.map_err(|_| fs_err("enumerate"))?;
            let metadata = entry
                .path()
                .symlink_metadata()
                .map_err(|_| fs_err("enumerate"))?;
            out.push(DirEntry {
                name: entry.file_name(),
                kind: EntryKind::of(&metadata),
            });
        }
        Ok(out)
    }

    fn read_marker(&self, dir: &Path) -> Result<Option<Vec<u8>>, BuildError> {
        let path = dir.join(super::marker::MARKER_FILENAME);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(fs_err("marker-read")),
        }
    }

    fn make_staging(&self, path: &Path) -> Result<(), BuildError> {
        std::fs::create_dir(path).map_err(|_| fs_err("staging-mkdir"))
    }

    fn write_asset(&self, path: &Path, bytes: &[u8]) -> Result<(), BuildError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|_| fs_err("asset-dir"))?;
        }
        std::fs::write(path, bytes).map_err(|_| fs_err("asset-write"))
    }

    fn copy_artifact(&self, from: &Path, to: &Path) -> Result<(), BuildError> {
        std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|_| fs_err("artifact-copy"))
    }

    fn commit_marker(
        &self,
        dir: &Path,
        marker: &Marker,
        _stage: MarkerStage,
    ) -> Result<(), BuildError> {
        write_marker(&RealMarkerFs as &dyn MarkerFs, dir, marker)
    }

    fn rename_output_to_backup(&self, output: &Path, backup: &Path) -> Result<(), BuildError> {
        std::fs::rename(output, backup).map_err(|_| fs_err("output-to-backup"))
    }

    fn remove_empty_output(&self, output: &Path) -> Result<(), BuildError> {
        std::fs::remove_dir(output).map_err(|_| fs_err("remove-empty-output"))
    }

    fn rename_staging_to_output(&self, staging: &Path, output: &Path) -> Result<(), BuildError> {
        std::fs::rename(staging, output).map_err(|_| fs_err("staging-to-output"))
    }

    fn rename_backup_to_output(&self, backup: &Path, output: &Path) -> Result<(), BuildError> {
        std::fs::rename(backup, output).map_err(|_| fs_err("backup-to-output"))
    }

    fn recreate_empty_output(&self, output: &Path) -> Result<(), BuildError> {
        std::fs::create_dir(output).map_err(|_| fs_err("recreate-empty-output"))
    }

    fn cleanup(&self, path: &Path) -> Result<(), BuildError> {
        std::fs::remove_dir_all(path).map_err(|_| fs_err("cleanup"))
    }
}

fn fs_err(context: &'static str) -> BuildError {
    BuildError::Filesystem { context }
}
