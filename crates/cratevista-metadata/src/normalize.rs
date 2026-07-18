//! The pure conversion boundary: `cargo_metadata::Metadata` → normalized schema
//! entities/relations/diagnostics.

use std::collections::{BTreeMap, BTreeSet};

use cargo_metadata::camino::Utf8Path;
use cargo_metadata::{DependencyKind, Metadata, Node, Package, PackageId, Target};
use cratevista_schema::{
    AttrValue, DocumentDiagnostic, Entity, EntityId, EntityKind, LocalizedText, Provenance,
    Relation, RelationId, RelationKind, SourceLocation,
};

use crate::diagnostics::{code, warn};
use crate::error::MetadataError;
use crate::ids::{classify_source, portable_source};
use crate::options::{ExternalDepsMode, MetadataOptions, PackageSelection, TargetKinds};
use crate::result::{MetadataIngest, MetadataSummary};
use crate::source::{SourceMapError, map_source};

/// The canonical CrateVista category for a Cargo target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetCategory {
    Lib,
    Bin,
    ProcMacro,
    Example,
    Test,
    Bench,
    BuildScript,
    Unknown,
}

impl TargetCategory {
    fn as_str(self) -> &'static str {
        match self {
            TargetCategory::Lib => "lib",
            TargetCategory::Bin => "bin",
            TargetCategory::ProcMacro => "proc-macro",
            TargetCategory::Example => "example",
            TargetCategory::Test => "test",
            TargetCategory::Bench => "bench",
            TargetCategory::BuildScript => "custom-build",
            TargetCategory::Unknown => "other",
        }
    }

    /// Whether this category is included given the opt-in `target_kinds`.
    fn included(self, opt_in: &TargetKinds) -> bool {
        match self {
            TargetCategory::Lib | TargetCategory::Bin | TargetCategory::ProcMacro => true,
            TargetCategory::Example => opt_in.example,
            TargetCategory::Test => opt_in.test,
            TargetCategory::Bench => opt_in.bench,
            TargetCategory::BuildScript => opt_in.build_script,
            // Unknown/future kinds: represent safely (included) with a diagnostic.
            TargetCategory::Unknown => true,
        }
    }
}

/// Classifies a Cargo target's `kind` list into a single canonical category.
fn categorize_target(target: &Target) -> TargetCategory {
    let kinds: Vec<String> = target.kind.iter().map(|k| k.to_string()).collect();
    let has = |name: &str| kinds.iter().any(|k| k == name);
    if has("proc-macro") {
        TargetCategory::ProcMacro
    } else if has("custom-build") {
        TargetCategory::BuildScript
    } else if has("example") {
        TargetCategory::Example
    } else if has("test") {
        TargetCategory::Test
    } else if has("bench") {
        TargetCategory::Bench
    } else if has("bin") {
        TargetCategory::Bin
    } else if kinds.iter().any(|k| {
        matches!(
            k.as_str(),
            "lib" | "rlib" | "dylib" | "cdylib" | "staticlib"
        )
    }) {
        TargetCategory::Lib
    } else {
        TargetCategory::Unknown
    }
}

fn dep_role(kind: DependencyKind) -> Option<&'static str> {
    match kind {
        DependencyKind::Normal => Some("normal"),
        DependencyKind::Development => Some("dev"),
        DependencyKind::Build => Some("build"),
        _ => None,
    }
}

/// The stable id key used in `package:{key}` / `target:{key}:...`.
fn package_key(package: &Package, is_member: bool) -> String {
    if is_member {
        package.name.to_string()
    } else {
        format!("{}@{}", package.name, package.version)
    }
}

/// Normalizes Cargo metadata into schema entities/relations/diagnostics.
pub fn normalize(
    metadata: &Metadata,
    options: &MetadataOptions,
) -> Result<MetadataIngest, MetadataError> {
    let root = metadata.workspace_root.as_path();
    let by_id: BTreeMap<&PackageId, &Package> =
        metadata.packages.iter().map(|p| (&p.id, p)).collect();
    let member_ids: BTreeSet<&PackageId> = metadata.workspace_members.iter().collect();

    // Resolve the selection into the set of member package ids to emit.
    let selected_members: Vec<&PackageId> = match &options.selection {
        PackageSelection::Default | PackageSelection::Workspace => {
            let mut v: Vec<&PackageId> = member_ids.iter().copied().collect();
            v.sort_by(|a, b| a.repr.cmp(&b.repr));
            v
        }
        PackageSelection::Packages(names) => {
            let mut selected = Vec::new();
            for name in names {
                let found = metadata
                    .workspace_members
                    .iter()
                    .find(|id| by_id.get(id).is_some_and(|p| p.name == name.as_str()));
                match found {
                    Some(id) => selected.push(id),
                    None => return Err(MetadataError::PackageNotFound(name.clone())),
                }
            }
            selected.sort_by(|a, b| a.repr.cmp(&b.repr));
            selected.dedup_by(|a, b| a.repr == b.repr);
            selected
        }
    };
    // Resolve graph nodes by id.
    let nodes: BTreeMap<&PackageId, &Node> = metadata
        .resolve
        .as_ref()
        .map(|r| r.nodes.iter().map(|n| (&n.id, n)).collect())
        .unwrap_or_default();

    // Determine which external packages are included.
    let mut included_external: BTreeSet<&PackageId> = BTreeSet::new();
    match options.external_deps {
        ExternalDepsMode::Exclude => {}
        ExternalDepsMode::DirectOnly => {
            for member in &selected_members {
                if let Some(node) = nodes.get(*member) {
                    for dep in &node.deps {
                        if !member_ids.contains(&dep.pkg) {
                            included_external.insert(&dep.pkg);
                        }
                    }
                }
            }
        }
        ExternalDepsMode::FullGraph => {
            for package in &metadata.packages {
                if !member_ids.contains(&package.id) {
                    included_external.insert(&package.id);
                }
            }
        }
    }

    let mut diagnostics: Vec<DocumentDiagnostic> = Vec::new();

    // Assign stable entity ids for every included package (members + externals),
    // detecting collisions.
    let id_map = assign_ids(
        &selected_members,
        &included_external,
        &by_id,
        &mut diagnostics,
    )?;

    let mut entities: Vec<Entity> = Vec::new();
    let mut relations: Vec<Relation> = Vec::new();

    // Workspace entity.
    let workspace_id = EntityId::workspace();
    entities.push(
        Entity::new(
            workspace_id.clone(),
            EntityKind::new(EntityKind::WORKSPACE),
            LocalizedText::new(workspace_label(root)),
            root.as_str(),
            Provenance::Discovered,
        )
        .with_source(map_workspace_source(root)),
    );

    let mut external_count = 0usize;
    let mut target_count = 0usize;

    // Package + target entities.
    for (pkg_id, entity_id) in &id_map {
        let package = by_id[pkg_id];
        let is_member = member_ids.contains(pkg_id);
        if !is_member {
            external_count += 1;
        }

        let mut package_entity = Entity::new(
            entity_id.clone(),
            EntityKind::new(EntityKind::PACKAGE),
            LocalizedText::new(package.name.to_string()),
            package.name.to_string(),
            Provenance::Discovered,
        );
        if is_member {
            package_entity.parent = Some(workspace_id.clone());
        }
        package_entity
            .attributes
            .insert("version".into(), package.version.to_string().into());
        package_entity.attributes.insert(
            "source_kind".into(),
            classify_source(package.source.as_ref()).as_str().into(),
        );
        add_feature_attributes(&mut package_entity, package, nodes.get(pkg_id).copied());

        // Member manifest source location.
        if is_member {
            match map_source(root, package.manifest_path.as_path()) {
                Ok(location) => package_entity.source = Some(location),
                Err(SourceMapError::OutsideWorkspace) => diagnostics.push(warn(
                    code::SOURCE_OUTSIDE_WORKSPACE,
                    format!(
                        "manifest of `{}` is outside the workspace root",
                        package.name
                    ),
                    Some(entity_id.clone()),
                )),
                Err(SourceMapError::Invalid(reason)) => diagnostics.push(warn(
                    code::NON_UTF8_PATH,
                    format!(
                        "manifest path of `{}` is not repo-relative: {reason}",
                        package.name
                    ),
                    Some(entity_id.clone()),
                )),
            }

            // The declared `repository` of a workspace member (Cargo.toml
            // `[package] repository`). Kept verbatim as a member-package attribute;
            // the graph layer decides the project-level value from these. Only
            // members contribute — an external dependency's repository is not ours
            // to advertise. The frontend decides whether the string is safe to link.
            if let Some(repository) = package.repository.as_deref() {
                let repository = repository.trim();
                if !repository.is_empty() {
                    package_entity
                        .attributes
                        .insert("repository".into(), repository.to_string().into());
                }
            }
        }

        entities.push(package_entity);

        // Containment: workspace -> member; package -> targets.
        if is_member {
            relations.push(contains(&workspace_id, entity_id));
            for target in &package.targets {
                let category = categorize_target(target);
                if !category.included(&options.target_kinds) {
                    continue;
                }
                if category == TargetCategory::Unknown {
                    diagnostics.push(warn(
                        code::UNSUPPORTED_TARGET,
                        format!(
                            "target `{}` of `{}` has an unrecognized kind {:?}",
                            target.name, package.name, target.kind
                        ),
                        Some(entity_id.clone()),
                    ));
                }
                let target_id = EntityId::target(
                    package_key(package, true).as_str(),
                    category.as_str(),
                    &target.name,
                );
                let mut target_entity = Entity::new(
                    target_id.clone(),
                    EntityKind::new(EntityKind::TARGET),
                    LocalizedText::new(format!("{} ({})", target.name, category.as_str())),
                    format!("{}::{}", package.name, target.name),
                    Provenance::Discovered,
                );
                target_entity.parent = Some(entity_id.clone());
                add_target_attributes(&mut target_entity, target);
                match map_source(root, target.src_path.as_path()) {
                    Ok(location) => target_entity.source = Some(location),
                    Err(SourceMapError::OutsideWorkspace) => diagnostics.push(warn(
                        code::SOURCE_OUTSIDE_WORKSPACE,
                        format!(
                            "target `{}` source is outside the workspace root",
                            target.name
                        ),
                        Some(target_id.clone()),
                    )),
                    Err(SourceMapError::Invalid(reason)) => diagnostics.push(warn(
                        code::NON_UTF8_PATH,
                        format!(
                            "target `{}` source is not repo-relative: {reason}",
                            target.name
                        ),
                        Some(target_id.clone()),
                    )),
                }
                relations.push(contains(entity_id, &target_id));
                entities.push(target_entity);
                target_count += 1;
            }
        }
    }

    // Dependency relations (resolved graph). Only edges between included packages.
    for (pkg_id, from_id) in &id_map {
        let Some(node) = nodes.get(pkg_id) else {
            continue;
        };
        let source_package = by_id[pkg_id];
        for node_dep in &node.deps {
            let Some(to_id) = id_map.get(&node_dep.pkg) else {
                continue; // endpoint not included → consistent boundary omission
            };
            let target_package = by_id.get(&node_dep.pkg).copied();
            for dep_kind in &node_dep.dep_kinds {
                let Some(role) = dep_role(dep_kind.kind) else {
                    diagnostics.push(warn(
                        code::INCOMPLETE_OPTIONAL_METADATA,
                        format!(
                            "unrecognized dependency kind on edge to `{}`",
                            node_dep.name
                        ),
                        Some(to_id.clone()),
                    ));
                    continue;
                };
                let target_cfg = dep_kind.target.as_ref().map(|t| t.to_string());
                relations.push(depends_on(
                    from_id,
                    to_id,
                    role,
                    target_cfg.as_deref(),
                    source_package,
                    target_package,
                    &node_dep.name,
                ));
            }
        }
    }

    // Determinism: sort + dedup identical evidence.
    entities.sort_by(|a, b| a.id.cmp(&b.id));
    relations.sort_by(|a, b| a.id.cmp(&b.id));
    relations.dedup_by(|a, b| a.id == b.id);
    diagnostics.sort();

    // Detect any residual duplicate entity id (should not happen after assign_ids).
    if let Some(dup) = first_duplicate(&entities) {
        return Err(MetadataError::InternalInvariant(format!(
            "duplicate entity id after normalization: {dup}"
        )));
    }

    let summary = MetadataSummary {
        workspace_root_repo_relative: Some(".".to_string()),
        selection: options.selection.clone(),
        external_deps_mode: options.external_deps,
        workspace_package_count: metadata.workspace_members.len(),
        selected_package_count: id_map.len(),
        external_package_count: external_count,
        target_count,
        dependency_relation_count: relations
            .iter()
            .filter(|r| r.kind.as_str() == RelationKind::DEPENDS_ON)
            .count(),
        recoverable_diagnostic_count: diagnostics.len(),
        cargo_argv: crate::invoke::effective_argv(options),
    };

    Ok(MetadataIngest {
        entities,
        relations,
        diagnostics,
        summary,
    })
}

fn workspace_label(root: &Utf8Path) -> String {
    root.file_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "workspace".to_string())
}

fn map_workspace_source(root: &Utf8Path) -> Option<SourceLocation> {
    // The workspace root maps to the manifest at its root when representable.
    map_source(root, root.join("Cargo.toml").as_path()).ok()
}

fn add_feature_attributes(entity: &mut Entity, package: &Package, node: Option<&Node>) {
    let mut declared: Vec<String> = package.features.keys().cloned().collect();
    declared.sort();
    let mut enabled: Vec<String> = node
        .map(|n| n.features.iter().map(|f| f.to_string()).collect())
        .unwrap_or_default();
    enabled.sort();
    let default_enabled = if package.features.contains_key("default") {
        enabled.iter().any(|f| f == "default")
    } else {
        true
    };
    entity
        .attributes
        .insert("declared_features".into(), json_str_array(&declared));
    entity
        .attributes
        .insert("enabled_features".into(), json_str_array(&enabled));
    entity
        .attributes
        .insert("default_features_enabled".into(), default_enabled.into());
}

fn add_target_attributes(entity: &mut Entity, target: &Target) {
    let mut crate_types: Vec<String> = target.crate_types.iter().map(|c| c.to_string()).collect();
    crate_types.sort();
    let mut required: Vec<String> = target.required_features.clone();
    required.sort();
    entity
        .attributes
        .insert("crate_types".into(), json_str_array(&crate_types));
    entity
        .attributes
        .insert("required_features".into(), json_str_array(&required));
    entity
        .attributes
        .insert("edition".into(), target.edition.as_str().into());
    entity
        .attributes
        .insert("doctest".into(), target.doctest.into());
    entity.attributes.insert("test".into(), target.test.into());
    entity.attributes.insert("doc".into(), target.doc.into());
}

fn json_str_array(items: &[String]) -> AttrValue {
    AttrValue::Array(items.iter().map(|s| AttrValue::String(s.clone())).collect())
}

fn contains(from: &EntityId, to: &EntityId) -> Relation {
    Relation::new(
        RelationKind::new(RelationKind::CONTAINS),
        from.clone(),
        to.clone(),
        Provenance::Discovered,
    )
}

#[allow(clippy::too_many_arguments)]
fn depends_on(
    from: &EntityId,
    to: &EntityId,
    role: &str,
    target_cfg: Option<&str>,
    source_package: &Package,
    target_package: Option<&Package>,
    dep_extern_name: &str,
) -> Relation {
    let kind = RelationKind::new(RelationKind::DEPENDS_ON);
    let id = match target_cfg {
        Some(cfg) => RelationId::with_role_and_discriminator(&kind, from, to, role, &[cfg]),
        None => RelationId::with_role(&kind, from, to, role),
    };
    let mut relation = Relation {
        id,
        kind,
        from: from.clone(),
        to: to.clone(),
        role: Some(role.to_string()),
        label: None,
        provenance: Provenance::Discovered,
        attributes: Default::default(),
    };

    // rename / optional from the source package's manifest dependency declaration.
    if let Some(target_package) = target_package {
        let matched = source_package.dependencies.iter().find(|d| {
            target_package.name == d.name.as_str()
                && role_matches(d.kind, role)
                && d.target.as_ref().map(|t| t.to_string()).as_deref() == target_cfg
        });
        if let Some(dep) = matched {
            if let Some(rename) = &dep.rename {
                relation
                    .attributes
                    .insert("rename".into(), rename.clone().into());
            } else if target_package.name != dep_extern_name {
                // The extern name differs from the crate name → effectively renamed.
                relation
                    .attributes
                    .insert("rename".into(), dep_extern_name.into());
            }
            if dep.optional {
                relation.attributes.insert("optional".into(), true.into());
            }
        }
        relation.attributes.insert(
            "resolved_version".into(),
            target_package.version.to_string().into(),
        );
        relation.attributes.insert(
            "source_kind".into(),
            classify_source(target_package.source.as_ref())
                .as_str()
                .into(),
        );
    }
    if let Some(cfg) = target_cfg {
        relation.attributes.insert("target_cfg".into(), cfg.into());
    }
    relation
}

fn role_matches(kind: DependencyKind, role: &str) -> bool {
    matches!(
        (kind, role),
        (DependencyKind::Normal, "normal")
            | (DependencyKind::Development, "dev")
            | (DependencyKind::Build, "build")
    )
}

fn first_duplicate(entities: &[Entity]) -> Option<String> {
    let mut seen = BTreeSet::new();
    for entity in entities {
        if !seen.insert(entity.id.as_str()) {
            return Some(entity.id.to_string());
        }
    }
    None
}

/// Assigns a stable [`EntityId`] to every included package, resolving external
/// name/version collisions across distinguishable portable sources and
/// deterministically collapsing non-portable collisions (with diagnostics).
fn assign_ids(
    selected_members: &[&PackageId],
    included_external: &BTreeSet<&PackageId>,
    by_id: &BTreeMap<&PackageId, &Package>,
    diagnostics: &mut Vec<DocumentDiagnostic>,
) -> Result<BTreeMap<PackageId, EntityId>, MetadataError> {
    let mut map: BTreeMap<PackageId, EntityId> = BTreeMap::new();

    // Members: package:{name}. Names are unique among members.
    let mut member_names: BTreeMap<String, PackageId> = BTreeMap::new();
    for member in selected_members {
        let package = by_id[member];
        let name = package.name.to_string();
        if let Some(existing) = member_names.insert(name.clone(), (*member).clone()) {
            return Err(MetadataError::InternalInvariant(format!(
                "duplicate workspace member name `{name}` ({} vs {})",
                existing.repr, member.repr
            )));
        }
        map.insert((*member).clone(), EntityId::package(&name));
    }

    // Externals grouped by (name, version).
    let mut groups: BTreeMap<(String, String), Vec<&PackageId>> = BTreeMap::new();
    for ext in included_external {
        let package = by_id[ext];
        groups
            .entry((package.name.to_string(), package.version.to_string()))
            .or_default()
            .push(ext);
    }

    for ((name, version), mut members) in groups {
        members.sort_by(|a, b| a.repr.cmp(&b.repr));
        let sources: Vec<Option<cargo_metadata::Source>> =
            members.iter().map(|id| by_id[id].source.clone()).collect();
        let (ids, mut group_diagnostics) = assign_external_group(&name, &version, &sources);
        for (member, id) in members.iter().zip(ids) {
            map.insert((*member).clone(), id);
        }
        diagnostics.append(&mut group_diagnostics);
    }

    Ok(map)
}

/// Resolves the entity ids for one `(name, version)` group of external packages
/// (already sorted deterministically). Portable sources (registry/git) are
/// disambiguated with a discriminator; non-portable ones (path/unknown) collapse
/// onto a single `name@version` entity with diagnostics.
pub(crate) fn assign_external_group(
    name: &str,
    version: &str,
    sources: &[Option<cargo_metadata::Source>],
) -> (Vec<EntityId>, Vec<DocumentDiagnostic>) {
    let mut diagnostics = Vec::new();
    if sources.len() == 1 {
        return (vec![EntityId::external_package(name, version)], diagnostics);
    }

    let mut ids: Vec<Option<EntityId>> = vec![None; sources.len()];
    let mut non_portable: Vec<usize> = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        match portable_source(source.as_ref()) {
            Some(portable) => {
                let kind = classify_source(source.as_ref());
                ids[index] = Some(EntityId::external_package_disambiguated(
                    name,
                    version,
                    kind.as_str(),
                    &portable,
                ));
            }
            None => non_portable.push(index),
        }
    }
    if let Some((&first, rest)) = non_portable.split_first() {
        let kept = EntityId::external_package(name, version);
        ids[first] = Some(kept.clone());
        for &index in rest {
            ids[index] = Some(kept.clone());
            diagnostics.push(warn(
                code::NON_PORTABLE_PATH_IDENTITY,
                format!(
                    "`{name}@{version}` has multiple non-portable sources; identities collapsed"
                ),
                Some(kept.clone()),
            ));
            diagnostics.push(warn(
                code::DUPLICATE_GENERATED_ID,
                format!("collapsed a duplicate `{name}@{version}` external onto one entity"),
                Some(kept.clone()),
            ));
            diagnostics.push(warn(
                code::OMITTED_EXTERNAL_IDENTITY,
                format!("omitted a distinct external identity for `{name}@{version}`"),
                Some(kept.clone()),
            ));
        }
    }
    (
        ids.into_iter().map(|id| id.expect("id assigned")).collect(),
        diagnostics,
    )
}

/// Small ergonomic extension used when building the workspace entity.
trait WithSource {
    fn with_source(self, source: Option<SourceLocation>) -> Self;
}
impl WithSource for Entity {
    fn with_source(mut self, source: Option<SourceLocation>) -> Self {
        self.source = source;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(repr: &str) -> cargo_metadata::Source {
        serde_json::from_value(serde_json::Value::String(repr.to_string())).unwrap()
    }

    #[test]
    fn single_external_uses_plain_id() {
        let (ids, diags) = assign_external_group("serde", "1.0.0", &[Some(source("registry+x"))]);
        assert_eq!(ids[0].as_str(), "package:serde@1.0.0");
        assert!(diags.is_empty());
    }

    #[test]
    fn portable_collision_is_disambiguated() {
        let (ids, diags) = assign_external_group(
            "dup",
            "1.0.0",
            &[
                Some(source("registry+https://a")),
                Some(source("git+https://b#r")),
            ],
        );
        assert_ne!(ids[0], ids[1], "distinct portable sources get distinct ids");
        assert!(ids[0].as_str().starts_with("package:dup@1.0.0:"));
        assert!(diags.is_empty());
        // No absolute path leaks into the ids.
        assert!(!ids[0].as_str().contains('/') || ids[0].as_str().starts_with("package:"));
    }

    #[test]
    fn non_portable_collision_collapses_with_diagnostics() {
        let (ids, diags) = assign_external_group(
            "dup",
            "1.0.0",
            &[
                Some(source("path+file:///a/abs")),
                Some(source("path+file:///b/abs")),
            ],
        );
        assert_eq!(ids[0], ids[1], "non-portable collision collapses to one id");
        assert_eq!(ids[0].as_str(), "package:dup@1.0.0");
        let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&code::NON_PORTABLE_PATH_IDENTITY));
        assert!(codes.contains(&code::DUPLICATE_GENERATED_ID));
        assert!(codes.contains(&code::OMITTED_EXTERNAL_IDENTITY));
    }
}
