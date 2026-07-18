//! Fixture builders for constructing `MetadataIngest` / `RustdocIngest` values in
//! stable-only unit tests (no nightly, no network).

#![allow(dead_code)]

use cratevista_metadata::{ExternalDepsMode, MetadataIngest, MetadataSummary, PackageSelection};
use cratevista_rustdoc::{
    CompatibilityTuple, CrateSummary, NetworkMode, RustdocIngest, RustdocSummary, TargetOutcome,
    TypeReferenceRole, UnresolvedTypeRef,
};
use cratevista_schema::{
    DocBlock, Entity, EntityId, EntityKind, LocalizedText, Provenance, Relation, RepoRelativePath,
    SourceLocation,
};

/// A workspace entity whose `qualified_name` is an absolute-looking path (to
/// exercise the graph's sanitization).
pub fn workspace_entity() -> Entity {
    Entity::new(
        EntityId::workspace(),
        EntityKind::new("workspace"),
        LocalizedText::new("ws"),
        "/abs/ws",
        Provenance::Discovered,
    )
}

pub fn package_entity(name: &str, manifest_repo_relative: &str) -> Entity {
    let mut entity = Entity::new(
        EntityId::package(name),
        EntityKind::new("package"),
        LocalizedText::new(name),
        name,
        Provenance::Discovered,
    );
    entity.parent = Some(EntityId::workspace());
    entity.source = Some(SourceLocation::new(
        RepoRelativePath::new(manifest_repo_relative).unwrap(),
        None,
    ));
    entity
}

pub fn target_entity(pkg: &str, kind: &str, name: &str, src_repo_relative: &str) -> Entity {
    let mut entity = Entity::new(
        EntityId::target(pkg, kind, name),
        EntityKind::new("target"),
        LocalizedText::new(format!("{name} ({kind})")),
        format!("{pkg}::{name}"),
        Provenance::Discovered,
    );
    entity.parent = Some(EntityId::package(pkg));
    entity.source = Some(SourceLocation::new(
        RepoRelativePath::new(src_repo_relative).unwrap(),
        None,
    ));
    entity
}

/// A minimal entity with an arbitrary id/kind/qualified-name.
pub fn entity_with_kind(id: &str, kind: &str, qualified_name: &str) -> Entity {
    Entity::new(
        EntityId::from_raw(id),
        EntityKind::new(kind),
        LocalizedText::new(id),
        qualified_name,
        Provenance::Discovered,
    )
}

/// A public, documented item entity.
pub fn item_entity(id: &str, kind: &str, qualified_name: &str) -> Entity {
    let mut entity = entity_with_kind(id, kind, qualified_name);
    entity
        .attributes
        .insert("visibility".into(), "public".into());
    entity.docs = Some(DocBlock {
        markdown: "doc".into(),
        summary: Some("doc".into()),
        documented: true,
    });
    entity
}

/// A public but undocumented item entity.
pub fn undocumented_item_entity(id: &str, kind: &str, qualified_name: &str) -> Entity {
    let mut entity = entity_with_kind(id, kind, qualified_name);
    entity
        .attributes
        .insert("visibility".into(), "public".into());
    entity
}

pub fn metadata_ingest(entities: Vec<Entity>) -> MetadataIngest {
    MetadataIngest {
        entities,
        relations: Vec::new(),
        diagnostics: Vec::new(),
        summary: MetadataSummary {
            workspace_root_repo_relative: Some(".".to_string()),
            selection: PackageSelection::Default,
            external_deps_mode: ExternalDepsMode::Exclude,
            workspace_package_count: 0,
            selected_package_count: 0,
            external_package_count: 0,
            target_count: 0,
            dependency_relation_count: 0,
            recoverable_diagnostic_count: 0,
            cargo_argv: vec!["cargo".into(), "metadata".into()],
        },
    }
}

#[allow(clippy::too_many_arguments)]
pub fn unresolved_ref(
    from: &str,
    role: TypeReferenceRole,
    crate_name: Option<&str>,
    canonical_path: Option<Vec<&str>>,
    item_kind: Option<&str>,
    display: &str,
) -> UnresolvedTypeRef {
    UnresolvedTypeRef {
        from: EntityId::from_raw(from),
        role,
        crate_name: crate_name.map(String::from),
        canonical_path: canonical_path.map(|p| p.into_iter().map(String::from).collect()),
        item_kind: item_kind.map(EntityKind::new),
        display: display.to_string(),
    }
}

pub fn crate_summary(
    crate_name: &str,
    package_id: &str,
    target_id: &str,
    root_module_id: &str,
    unresolved_refs: Vec<UnresolvedTypeRef>,
) -> CrateSummary {
    CrateSummary {
        package_id: EntityId::from_raw(package_id),
        target_id: EntityId::from_raw(target_id),
        root_module_id: EntityId::from_raw(root_module_id),
        crate_name: crate_name.to_string(),
        format_version: 60,
        toolchain: "nightly-2026-07-01".into(),
        entity_count: 0,
        relation_count: 0,
        unresolved_refs,
    }
}

pub fn rustdoc_ingest(
    crates: Vec<CrateSummary>,
    entities: Vec<Entity>,
    relations: Vec<Relation>,
    partial: bool,
) -> RustdocIngest {
    RustdocIngest {
        crates,
        entities,
        relations,
        diagnostics: Vec::new(),
        summary: RustdocSummary {
            documented_crate_count: 0,
            entity_count: 0,
            relation_count: 0,
            succeeded_target_count: 0,
            failed_target_count: 0,
            partial,
            include_private: false,
            features: Vec::new(),
            network: NetworkMode::Inherit,
            compat: CompatibilityTuple::current("nightly-2026-07-01"),
            targets: Vec::<TargetOutcome>::new(),
        },
    }
}
