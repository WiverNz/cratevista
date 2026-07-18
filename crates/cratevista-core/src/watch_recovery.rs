//! Recovery coverage: what to watch when `cargo metadata` will not run.
//!
//! # The gap this closes
//!
//! [`crate::watch::build_watch_plan`] needs a successful `cargo metadata`. But
//! metadata **fails** in exactly the situation that matters most: a root manifest
//! that declares a member whose `Cargo.toml` is missing or malformed. The full
//! plan cannot be built, so the new member's manifest is never watched, so the
//! user's fix is never observed — they correct the file and nothing happens.
//!
//! Recovery coverage is built **without cargo**, by reading the root manifest's
//! `[workspace] members`/`exclude` directly. It is deliberately coarse: it does
//! not know a target's `src_path` and does not pretend to. It knows enough to see
//! a fix.
//!
//! # Coverage may lead; it may never narrow
//!
//! Recovery coverage is a **superset of the currently active plan**: it adds
//! candidate member manifests and never removes a source root or config input the
//! active plan already had. Extra observation costs a redundant rebuild; removing
//! observation loses an edit invisibly.
//!
//! Plan evolution is therefore `previous → recovery → complete`, while snapshot
//! publication does not move at all until the final commit.

use std::path::{Component, Path};

use cratevista_watch::WatchInput;
use serde::Deserialize;

use crate::watch::{WatchSetupError, code};

/// The only part of a root `Cargo.toml` recovery needs.
///
/// `deny_unknown_fields` is deliberately **not** used: a real `Cargo.toml` is full
/// of keys that are none of our business, and refusing to read one because it has
/// a `[profile]` section would defeat the entire purpose.
#[derive(Debug, Default, Deserialize)]
struct RootManifest {
    workspace: Option<WorkspaceSection>,
}

#[derive(Debug, Default, Deserialize)]
struct WorkspaceSection {
    #[serde(default)]
    members: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

/// The workspace-member **patterns** the root manifest declares, as first-class
/// inputs.
///
/// Used by **both** plan builders. That is the point: `cargo metadata` only knows
/// the members that exist *right now*, so a complete plan built from metadata
/// alone would stop covering `crates/*` the moment it succeeded — and creating
/// `crates/new/Cargo.toml` afterwards would trigger nothing.
///
/// Only entries containing pattern characters become pattern inputs; an explicit
/// `crates/new` becomes an exact manifest input instead, which is more precise.
pub(crate) fn member_pattern_inputs(
    canonical_root: &Path,
) -> Result<Vec<WatchInput>, WatchSetupError> {
    let workspace = read_workspace(canonical_root)?;
    let excludes: Vec<String> = workspace
        .exclude
        .iter()
        .filter_map(|entry| normalize_entry(entry))
        .collect();

    let mut inputs = Vec::new();
    for member in &workspace.members {
        // Absolute, UNC, drive-qualified or traversing entries are skipped so the
        // other members still get coverage. Cargo rejects them loudly on the next
        // real run, which is where that complaint belongs — and skipping
        // guarantees no external location is ever watched.
        let Some(entry) = normalize_entry(member) else {
            continue;
        };
        if !entry.contains(['*', '?', '[']) {
            continue;
        }
        inputs.push(WatchInput::workspace_member_pattern(
            canonical_root.join(&entry),
            excludes.clone(),
        ));
    }
    Ok(inputs)
}

/// Reads and parses the root manifest's `[workspace]` section.
fn read_workspace(canonical_root: &Path) -> Result<WorkspaceSection, WatchSetupError> {
    let text = std::fs::read_to_string(canonical_root.join("Cargo.toml")).map_err(|_| {
        WatchSetupError::new(
            code::ROOT_MANIFEST_UNREADABLE,
            "the workspace manifest could not be read",
        )
    })?;
    let manifest: RootManifest = toml::from_str(&text).map_err(|_| {
        // The parse error names a line and the file; neither is publishable, and
        // the terminal already shows cargo's own version of it.
        WatchSetupError::new(
            code::ROOT_MANIFEST_INVALID,
            "the workspace manifest is not valid TOML",
        )
    })?;
    Ok(manifest.workspace.unwrap_or_default())
}

/// Builds the recovery input set: everything the active plan had, plus the
/// manifests the root declares.
///
/// `active` is core's retained logical representation of the live plan — the
/// watcher is never asked what it is watching, and no registration detail leaks
/// out of `cratevista-watch`.
pub(crate) fn recovery_inputs(
    canonical_root: &Path,
    active: &[WatchInput],
    no_config: bool,
) -> Result<Vec<WatchInput>, WatchSetupError> {
    let workspace = read_workspace(canonical_root)?;

    // Start from the active plan: recovery must be a superset, never a
    // replacement. Losing an existing source root here would trade one blind spot
    // for another.
    let mut inputs: Vec<WatchInput> = active.to_vec();

    inputs.push(WatchInput::file(canonical_root.join("Cargo.toml")));
    inputs.push(WatchInput::file(canonical_root.join("Cargo.lock")));

    if !no_config {
        inputs.push(WatchInput::file(
            canonical_root.join(cratevista_config::discover::ROOT_CONFIG),
        ));
        inputs.push(WatchInput::flows_dir(
            canonical_root.join(".cratevista").join("flows"),
        ));
        inputs.push(WatchInput::overrides_dir(
            canonical_root.join(".cratevista").join("overrides"),
        ));
    }

    let excludes: Vec<String> = workspace
        .exclude
        .iter()
        .filter_map(|entry| normalize_entry(entry))
        .collect();

    for member in &workspace.members {
        for manifest in member_manifests(canonical_root, member, &excludes) {
            inputs.push(WatchInput::file(canonical_root.join(manifest)));
        }
    }

    // The declared patterns themselves, so a member created later is covered
    // without the root manifest changing again.
    inputs.extend(member_pattern_inputs(canonical_root)?);

    Ok(inputs)
}

/// The candidate manifests one `members` entry names, workspace-relative.
///
/// Cargo-compatible enough for recovery, and no more:
/// - `crates/new` → `crates/new/Cargo.toml`;
/// - `crates/new/Cargo.toml` → itself (Cargo accepts this spelling);
/// - `crates/*` → one entry per **existing** subdirectory, excludes applied.
///
/// An entry that is absolute, drive-lettered, UNC or traversing yields **nothing**
/// rather than an error: recovery exists to rescue a broken manifest, and refusing
/// to watch the *other* members because one entry is malformed would be exactly
/// backwards. Cargo will reject it loudly on the next real run, which is where
/// that complaint belongs — and skipping guarantees no external disk location is
/// ever registered.
fn member_manifests(root: &Path, member: &str, excludes: &[String]) -> Vec<String> {
    let Some(entry) = normalize_entry(member) else {
        return Vec::new();
    };

    if !entry.contains('*') {
        let path = if entry.ends_with("/Cargo.toml") || entry == "Cargo.toml" {
            entry
        } else {
            format!("{entry}/Cargo.toml")
        };
        if is_excluded(&path, excludes) {
            return Vec::new();
        }
        return vec![path];
    }

    // A glob. Only the static prefix before the first `*` is read from disk, and
    // only its direct children are considered — nothing is parsed, and no
    // arbitrary descendant `Cargo.toml` anywhere in the tree is treated as a
    // member just because it exists.
    let prefix = entry.split('*').next().unwrap_or("").trim_end_matches('/');
    let directory = if prefix.is_empty() {
        root.to_path_buf()
    } else {
        root.join(prefix)
    };

    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut found: Vec<String> = Vec::new();
    for child in entries.flatten() {
        if !child.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Some(name) = child.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let candidate = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if !matches_pattern(&entry, &candidate) {
            continue;
        }
        let manifest = format!("{candidate}/Cargo.toml");
        if is_excluded(&candidate, excludes) || is_excluded(&manifest, excludes) {
            continue;
        }
        found.push(manifest);
    }
    found
}

/// Whether `candidate` matches a `members` pattern.
///
/// Component-wise, with `*` matching **one** component — Cargo's common
/// `crates/*` shape. Anything more elaborate simply does not match, which costs
/// coverage rather than correctness: the full plan still watches it once metadata
/// succeeds.
fn matches_pattern(pattern: &str, candidate: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let candidate: Vec<&str> = candidate.split('/').collect();
    if pattern.len() != candidate.len() {
        return false;
    }
    pattern
        .iter()
        .zip(&candidate)
        .all(|(pattern, part)| *pattern == "*" || pattern == part)
}

/// Whether a workspace-relative path is excluded.
///
/// Applied **before** an input is added, so an excluded directory never becomes
/// coverage. An exclude matches the path itself or any ancestor of it, which is
/// how Cargo treats `exclude = ["crates/skipped"]`.
fn is_excluded(path: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|exclude| {
        path == exclude
            || path.starts_with(&format!("{exclude}/"))
            || matches_pattern(exclude, path)
    })
}

/// Normalizes a manifest entry, refusing anything that could leave the workspace.
///
/// Returns `None` for absolute, drive-lettered, UNC, traversing or empty
/// spellings. This is the textual half of containment; the registration builder
/// still canonicalizes every existing target it registers, which is what catches a
/// symlink escape.
fn normalize_entry(entry: &str) -> Option<String> {
    let text = entry.trim().replace('\\', "/");
    if text.is_empty() || text.starts_with('/') || text.starts_with("//") {
        return None;
    }
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && (bytes[0] as char).is_ascii_alphabetic() {
        return None;
    }
    if Path::new(&text)
        .components()
        .any(|part| part == Component::ParentDir)
    {
        return None;
    }
    let cleaned: Vec<&str> = text
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_member_becomes_its_manifest() {
        assert_eq!(
            member_manifests(Path::new("/w"), "crates/new", &[]),
            ["crates/new/Cargo.toml"]
        );
    }

    #[test]
    fn a_member_entry_that_already_names_the_manifest_is_kept() {
        // Cargo accepts this spelling; appending a second `Cargo.toml` would watch
        // a path that can never exist.
        assert_eq!(
            member_manifests(Path::new("/w"), "crates/new/Cargo.toml", &[]),
            ["crates/new/Cargo.toml"]
        );
    }

    #[test]
    fn absolute_traversing_and_unc_entries_yield_nothing() {
        for entry in [
            "/etc/passwd",
            "C:/secrets",
            "//host/share/x",
            "../outside",
            "crates/../../escape",
            "",
            "   ",
        ] {
            assert!(
                member_manifests(Path::new("/w"), entry, &[]).is_empty(),
                "`{entry}` must never become coverage"
            );
        }
    }

    #[test]
    fn an_excluded_explicit_member_is_dropped() {
        assert!(
            member_manifests(
                Path::new("/w"),
                "crates/skipped",
                &["crates/skipped".to_string()]
            )
            .is_empty()
        );
    }

    #[test]
    fn a_pattern_matches_one_component_per_star() {
        assert!(matches_pattern("crates/*", "crates/demo"));
        assert!(!matches_pattern("crates/*", "crates/demo/nested"));
        assert!(!matches_pattern("crates/*", "other/demo"));
        assert!(matches_pattern("crates/demo", "crates/demo"));
    }

    #[test]
    fn an_exclude_covers_a_path_and_its_descendants() {
        let excludes = vec!["crates/skipped".to_string()];
        assert!(is_excluded("crates/skipped", &excludes));
        assert!(is_excluded("crates/skipped/Cargo.toml", &excludes));
        assert!(!is_excluded("crates/kept", &excludes));
        assert!(!is_excluded("crates/skipped-too", &excludes));
    }
}
