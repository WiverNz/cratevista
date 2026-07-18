//! Pure `RustdocPlan` construction from `MetadataIngest`.
//!
//! Planning is **pure domain logic**: it performs no Cargo invocation, no
//! rustdoc invocation, no filesystem reads, and no metadata re-ingestion. The
//! absolute `workspace_root` is runtime orchestration context supplied by
//! `cratevista-core`; the planner joins it with each package's repo-relative
//! manifest source to produce absolute `RustdocTarget` paths.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cratevista_metadata::MetadataIngest;
use cratevista_rustdoc::{RustdocPlan, RustdocTarget, RustdocTargetKind};
use cratevista_schema::{Entity, EntityId};

use crate::error::GraphError;

/// Options controlling target selection.
#[derive(Debug, Clone, Default)]
pub struct RustdocPlanOptions {
    /// Include `bin` targets. Library and proc-macro targets are always included.
    pub include_binaries: bool,
}

/// Builds a concrete, pre-resolved plan from metadata target entities.
///
/// Selects workspace-member **library** and **proc-macro** targets (and `bin`
/// targets only when `include_binaries`). Examples/tests/benches/custom-build and
/// external-dependency targets are never selected. Returns an **empty** plan
/// normally when no documentable target exists.
pub fn build_rustdoc_plan(
    metadata: &MetadataIngest,
    workspace_root: &Path,
    options: &RustdocPlanOptions,
) -> Result<RustdocPlan, GraphError> {
    let packages: BTreeMap<&EntityId, &Entity> = metadata
        .entities
        .iter()
        .filter(|entity| entity.kind.as_str() == "package")
        .map(|entity| (&entity.id, entity))
        .collect();

    let mut targets: Vec<RustdocTarget> = Vec::new();

    for entity in metadata
        .entities
        .iter()
        .filter(|e| e.kind.as_str() == "target")
    {
        let package_id = entity.parent.clone().ok_or_else(|| {
            GraphError::Plan(format!("target `{}` has no parent package", entity.id))
        })?;
        let package = packages.get(&package_id).ok_or_else(|| {
            GraphError::Plan(format!(
                "target `{}` references missing package `{package_id}`",
                entity.id
            ))
        })?;
        let package_name = package.qualified_name.clone();

        let Some((kind_str, target_name)) = parse_target_id(entity.id.as_str(), &package_name)
        else {
            // Not a recognizable `target:{pkg}:{kind}:{name}` id — skip defensively.
            continue;
        };

        let target_kind = match kind_str {
            "lib" => RustdocTargetKind::Library,
            "proc-macro" => RustdocTargetKind::ProcMacro,
            "bin" if options.include_binaries => RustdocTargetKind::Binary,
            // bin (not opted in), example/test/bench/custom-build/other → not documented.
            _ => continue,
        };

        // Absolute manifest path from the package's repo-relative manifest source.
        let manifest_repo_relative = package
            .source
            .as_ref()
            .map(|location| location.path.as_str())
            .ok_or_else(|| {
                GraphError::Plan(format!(
                    "package `{package_name}` has no manifest source location; cannot document `{target_name}`"
                ))
            })?;
        let manifest_path = join_repo_relative(workspace_root, manifest_repo_relative);
        let package_root = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| workspace_root.to_path_buf());

        targets.push(RustdocTarget {
            package_id,
            target_id: entity.id.clone(),
            package_name,
            target_name: target_name.to_string(),
            // The actual crate name: a lib/proc-macro target's Cargo name is
            // already the crate name; a bin's crate name replaces `-` with `_`.
            crate_name: target_name.replace('-', "_"),
            target_kind,
            manifest_path,
            package_root,
        });
    }

    // Deterministic ordering.
    targets.sort_by(|a, b| {
        (&a.package_name, a.target_kind.as_str(), &a.target_name).cmp(&(
            &b.package_name,
            b.target_kind.as_str(),
            &b.target_name,
        ))
    });

    Ok(RustdocPlan {
        workspace_root: workspace_root.to_path_buf(),
        targets,
    })
}

/// Parses `target:{package}:{kind}:{name}` given the known package name.
fn parse_target_id<'a>(id: &'a str, package_name: &str) -> Option<(&'a str, &'a str)> {
    let rest = id.strip_prefix(&format!("target:{package_name}:"))?;
    rest.split_once(':')
}

/// Joins an absolute workspace root with a validated repo-relative path
/// (forward-slash separators) into an absolute path.
fn join_repo_relative(root: &Path, repo_relative: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for component in repo_relative.split('/').filter(|c| !c.is_empty()) {
        path.push(component);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{metadata_ingest, package_entity, target_entity, workspace_entity};

    fn workspace() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from("C:\\ws")
        } else {
            PathBuf::from("/ws")
        }
    }

    #[test]
    fn selects_lib_and_proc_macro_not_bin_by_default() {
        let metadata = metadata_ingest(vec![
            workspace_entity(),
            package_entity("a", "crates/a/Cargo.toml"),
            target_entity("a", "lib", "a", "crates/a/src/lib.rs"),
            target_entity("a", "bin", "a-cli", "crates/a/src/main.rs"),
            package_entity("m", "crates/m/Cargo.toml"),
            target_entity("m", "proc-macro", "m", "crates/m/src/lib.rs"),
        ]);
        let plan =
            build_rustdoc_plan(&metadata, &workspace(), &RustdocPlanOptions::default()).unwrap();
        let kinds: Vec<&str> = plan
            .targets
            .iter()
            .map(|t| t.target_kind.as_str())
            .collect();
        assert_eq!(kinds, vec!["lib", "proc-macro"]);
        // Absolute manifest path derived from the workspace root.
        let lib = &plan.targets[0];
        assert!(lib.manifest_path.is_absolute() || lib.manifest_path.starts_with(workspace()));
        assert!(lib.manifest_path.ends_with("Cargo.toml"));
        assert_eq!(lib.package_id.as_str(), "package:a");
        assert_eq!(lib.target_id.as_str(), "target:a:lib:a");
        assert_eq!(lib.crate_name, "a");
        plan.validate().expect("plan is valid");
    }

    #[test]
    fn binary_opt_in_and_crate_name_underscored() {
        let metadata = metadata_ingest(vec![
            workspace_entity(),
            package_entity("a", "crates/a/Cargo.toml"),
            target_entity("a", "bin", "a-cli", "crates/a/src/main.rs"),
        ]);
        let options = RustdocPlanOptions {
            include_binaries: true,
        };
        let plan = build_rustdoc_plan(&metadata, &workspace(), &options).unwrap();
        assert_eq!(plan.targets.len(), 1);
        assert_eq!(plan.targets[0].target_name, "a-cli");
        assert_eq!(plan.targets[0].crate_name, "a_cli");
        assert_eq!(plan.targets[0].target_kind.as_str(), "bin");
    }

    #[test]
    fn no_documentable_target_yields_empty_plan() {
        let metadata = metadata_ingest(vec![
            workspace_entity(),
            package_entity("a", "crates/a/Cargo.toml"),
            target_entity("a", "bin", "a", "crates/a/src/main.rs"),
        ]);
        let plan =
            build_rustdoc_plan(&metadata, &workspace(), &RustdocPlanOptions::default()).unwrap();
        assert!(plan.targets.is_empty());
    }

    #[test]
    fn lib_and_bin_same_crate_name_distinct_target_ids() {
        let metadata = metadata_ingest(vec![
            workspace_entity(),
            package_entity("tool", "crates/tool/Cargo.toml"),
            target_entity("tool", "lib", "tool", "crates/tool/src/lib.rs"),
            target_entity("tool", "bin", "tool", "crates/tool/src/main.rs"),
        ]);
        let options = RustdocPlanOptions {
            include_binaries: true,
        };
        let plan = build_rustdoc_plan(&metadata, &workspace(), &options).unwrap();
        assert_eq!(plan.targets.len(), 2);
        let ids: Vec<&str> = plan.targets.iter().map(|t| t.target_id.as_str()).collect();
        assert!(ids.contains(&"target:tool:lib:tool"));
        assert!(ids.contains(&"target:tool:bin:tool"));
        plan.validate().expect("distinct target ids → valid plan");
    }
}
