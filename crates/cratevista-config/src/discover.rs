//! Locating configuration files, deterministically.
//!
//! Discovery is **sorted, not filesystem order**: `read_dir` yields entries in
//! whatever order the OS feels like, which differs between Linux, macOS and
//! Windows and even between runs. Since load order decides which duplicate wins
//! (and therefore the generated document), it must be a property of the *names*,
//! never of the disk.
//!
//! Absence of configuration is **normal**, not an error: it yields an empty set.

use std::path::{Path, PathBuf};

/// Where configuration lives, relative to the workspace root.
pub const ROOT_CONFIG: &str = "cratevista.toml";
/// The configuration directory.
pub const CONFIG_DIR: &str = ".cratevista";
/// Flow files: `.cratevista/flows/*.toml`.
pub const FLOWS_DIR: &str = "flows";
/// Override files: `.cratevista/overrides/*.toml`.
pub const OVERRIDES_DIR: &str = "overrides";

/// The configuration files present under `workspace_root`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovered {
    /// `cratevista.toml`, when it exists.
    pub root: Option<PathBuf>,
    /// `.cratevista/flows/*.toml`, sorted by file name.
    pub flows: Vec<PathBuf>,
    /// `.cratevista/overrides/*.toml`, sorted by file name.
    pub overrides: Vec<PathBuf>,
}

impl Discovered {
    /// True when there is nothing to load (the zero-configuration default).
    pub fn is_empty(&self) -> bool {
        self.root.is_none() && self.flows.is_empty() && self.overrides.is_empty()
    }
}

/// Lists `*.toml` files directly inside `dir`, sorted by file name.
///
/// Not recursive: a nested directory is not a flow file, and silently walking
/// into one would make the config set depend on layout choices the format does
/// not define. A missing/unreadable directory yields an empty list — discovery
/// reports nothing; `load` reports what it cannot read.
fn toml_files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    // Sort by the OS string of the file name: stable across platforms and runs,
    // and independent of the parent path.
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    files
}

/// Finds configuration under `workspace_root`. Never fails: absence is normal.
pub fn discover(workspace_root: &Path) -> Discovered {
    let root_config = workspace_root.join(ROOT_CONFIG);
    let config_dir = workspace_root.join(CONFIG_DIR);
    Discovered {
        root: root_config.is_file().then_some(root_config),
        flows: toml_files_in(&config_dir.join(FLOWS_DIR)),
        overrides: toml_files_in(&config_dir.join(OVERRIDES_DIR)),
    }
}

/// Renders `path` as a workspace-relative, `/`-normalized string.
///
/// Diagnostics must never carry an absolute path (the same rule the rest of the
/// tool follows). If `path` somehow escapes the root, the file name alone is
/// used rather than leaking the layout above it.
pub fn relative_label(workspace_root: &Path, path: &Path) -> String {
    let relative = match path.strip_prefix(workspace_root) {
        Ok(relative) => relative,
        // Outside the root: fall back to the bare file name rather than leaking
        // the layout above it.
        Err(_) => Path::new(path.file_name().unwrap_or_default()),
    };
    relative.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn no_configuration_is_normal_and_empty() {
        let dir = tempfile::tempdir().unwrap();
        let found = discover(dir.path());
        assert!(found.is_empty());
        assert_eq!(found, Discovered::default());
    }

    #[test]
    fn finds_root_flows_and_overrides() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "cratevista.toml", "");
        write(dir.path(), ".cratevista/flows/a.toml", "");
        write(dir.path(), ".cratevista/overrides/o.toml", "");

        let found = discover(dir.path());
        assert!(found.root.is_some());
        assert_eq!(found.flows.len(), 1);
        assert_eq!(found.overrides.len(), 1);
        assert!(!found.is_empty());
    }

    #[test]
    fn files_are_sorted_by_name_not_filesystem_order() {
        let dir = tempfile::tempdir().unwrap();
        // Created out of order on purpose.
        for name in ["zeta.toml", "alpha.toml", "middle.toml", "beta.toml"] {
            write(dir.path(), &format!(".cratevista/flows/{name}"), "");
        }
        let found = discover(dir.path());
        let names: Vec<String> = found
            .flows
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            ["alpha.toml", "beta.toml", "middle.toml", "zeta.toml"]
        );
    }

    #[test]
    fn discovery_is_repeatable() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.toml", "a.toml", "c.toml"] {
            write(dir.path(), &format!(".cratevista/flows/{name}"), "");
        }
        let first = discover(dir.path());
        for _ in 0..5 {
            assert_eq!(discover(dir.path()), first);
        }
    }

    #[test]
    fn only_toml_files_are_discovered_and_directories_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".cratevista/flows/real.toml", "");
        write(dir.path(), ".cratevista/flows/notes.md", "");
        write(dir.path(), ".cratevista/flows/data.json", "");
        // A nested directory — including one that looks like a toml file.
        std::fs::create_dir_all(dir.path().join(".cratevista/flows/nested.toml")).unwrap();
        write(dir.path(), ".cratevista/flows/nested.toml/inner.toml", "");

        let found = discover(dir.path());
        let names: Vec<String> = found
            .flows
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["real.toml"], "non-toml and directories are skipped");
    }

    #[test]
    fn a_missing_config_dir_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "cratevista.toml", "");
        let found = discover(dir.path());
        assert!(found.root.is_some());
        assert!(found.flows.is_empty());
        assert!(found.overrides.is_empty());
    }

    #[test]
    fn labels_are_workspace_relative_and_slash_normalized() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".cratevista").join("flows").join("a.toml");
        let label = relative_label(dir.path(), &path);
        assert_eq!(label, ".cratevista/flows/a.toml");
        assert!(
            !label.contains('\\'),
            "no backslashes leak into diagnostics"
        );
        // And no absolute prefix survives.
        assert!(!label.contains(&dir.path().to_string_lossy().to_string()));
    }
}
