//! The pure conversion boundary: a parsed `rustdoc_types::Crate` + context →
//! deterministically-ordered schema entities/relations/diagnostics.
//!
//! Two passes: the first assigns every reachable local item a stable
//! [`EntityId`] (so forward type references resolve), the second builds entities
//! and only those relations whose target resolves **within this crate's index**.
//! Unresolvable references are preserved, never invented as edges.

use std::collections::{BTreeMap, BTreeSet};

use cratevista_schema::{
    AttrValue, DocBlock, DocumentDiagnostic, Entity, EntityId, EntityKind, LocalizedText,
    Provenance, Relation, RelationId, RelationKind,
};
use rustdoc_types::{
    Crate, Id, Item, ItemEnum, ItemKind, Path, StructKind, Type, VariantKind, Visibility,
};

use crate::diagnostics::{code, warn};
use crate::error::RustdocError;
use crate::ids::{
    assoc_item_kind, impl_for_display, impl_is_synthetic_or_blanket, impl_signature,
    impl_trait_or_inherent, join, raw_id_label,
};
use crate::options::NormalizeContext;
use crate::result::{CrateIngest, CrateSummary, TypeReferenceRole, UnresolvedTypeRef};
use crate::spans::{SpanOmission, map_span};
use crate::types::{display_type, head_id, result_ok_err, type_ref};

/// A planned entity discovered during the id-assignment pass.
struct Node {
    raw_id: Id,
    entity_id: EntityId,
    kind: String,
    qualified_name: String,
    label: String,
    parent: Option<EntityId>,
}

struct Builder<'a> {
    krate: &'a Crate,
    ctx: &'a NormalizeContext,
    crate_name: String,
    /// The rustdoc `crate_id` of the local crate (usually 0); used to name
    /// references back into the local crate.
    local_crate_id: u32,
    entity_ids: BTreeMap<u32, EntityId>,
    nodes: Vec<Node>,
    visited: BTreeSet<u32>,
    reexports: Vec<(EntityId, rustdoc_types::Use)>,
    /// The actual root-module entity id assigned to `krate.root`.
    root_module_id: Option<EntityId>,
    diagnostics: Vec<DocumentDiagnostic>,
}

/// Normalizes a parsed crate into schema entities/relations/diagnostics.
///
/// Fails with [`RustdocError::InternalInvariant`] if a supposedly-successful
/// crate has no trustworthy root-module identity (the root module is always
/// created before the walk, so this is defensive).
pub fn normalize_crate(krate: &Crate, ctx: &NormalizeContext) -> Result<CrateIngest, RustdocError> {
    let local_crate_id = krate
        .index
        .get(&krate.root)
        .map(|item| item.crate_id)
        .unwrap_or(0);
    let mut builder = Builder {
        krate,
        ctx,
        crate_name: ctx.crate_name.clone(),
        local_crate_id,
        entity_ids: BTreeMap::new(),
        nodes: Vec::new(),
        visited: BTreeSet::new(),
        reexports: Vec::new(),
        root_module_id: None,
        diagnostics: Vec::new(),
    };

    builder.assign_ids();
    let (entities, relations, unresolved) = builder.build();
    let root_module_id = builder.root_module_id.take();
    let mut diagnostics = builder.diagnostics;

    // Determinism: stable order everywhere.
    let mut entities = dedup_entities(entities, &mut diagnostics);
    entities.sort_by(|a, b| a.id.cmp(&b.id));
    let mut relations = relations;
    relations.sort_by(|a, b| a.id.cmp(&b.id));
    relations.dedup_by(|a, b| a.id == b.id);
    let mut unresolved = unresolved;
    unresolved.sort();
    unresolved.dedup();
    diagnostics.sort();

    let root_module_id = root_module_id.ok_or_else(|| {
        RustdocError::InternalInvariant(format!(
            "crate `{}` produced no root module entity",
            ctx.crate_name
        ))
    })?;

    let summary = CrateSummary {
        package_id: ctx.package_id.clone(),
        target_id: ctx.target_id.clone(),
        root_module_id,
        crate_name: ctx.crate_name.clone(),
        format_version: krate.format_version,
        toolchain: ctx.toolchain.clone(),
        entity_count: entities.len(),
        relation_count: relations.len(),
        unresolved_refs: unresolved,
    };

    Ok(CrateIngest {
        entities,
        relations,
        diagnostics,
        summary,
    })
}

impl<'a> Builder<'a> {
    // ---- Pass 1: id assignment ------------------------------------------

    fn assign_ids(&mut self) {
        let root_id = self.krate.root;
        let crate_name = self.crate_name.clone();
        let root_entity = EntityId::module(&crate_name, &crate_name);
        self.root_module_id = Some(root_entity.clone());
        self.record(Node {
            raw_id: root_id,
            entity_id: root_entity.clone(),
            kind: "module".to_string(),
            qualified_name: crate_name.clone(),
            label: crate_name.clone(),
            parent: None,
        });
        self.visited.insert(root_id.0);
        if let Some(item) = self.krate.index.get(&root_id)
            && let ItemEnum::Module(module) = &item.inner
        {
            let children = module.items.clone();
            for child in children {
                self.walk_item(child, "", &root_entity);
            }
        }

        // Impls for locally-defined traits whose self type is not a local nominal
        // entity (e.g. `impl LocalTrait for u32`) are reachable only via the
        // trait; walk them so they are not silently dropped.
        let trait_impls: Vec<(Id, Vec<Id>)> = self
            .krate
            .index
            .iter()
            .filter_map(|(id, item)| match &item.inner {
                ItemEnum::Trait(t) => Some((*id, t.implementations.clone())),
                _ => None,
            })
            .collect();
        for (trait_id, implementations) in trait_impls {
            if let Some(trait_entity) = self.entity_ids.get(&trait_id.0).cloned() {
                for impl_id in implementations {
                    self.walk_impl(impl_id, &trait_entity);
                }
            }
        }
    }

    fn record(&mut self, node: Node) {
        self.entity_ids
            .insert(node.raw_id.0, node.entity_id.clone());
        self.nodes.push(node);
    }

    fn walk_item(&mut self, id: Id, prefix: &str, parent: &EntityId) {
        if !self.visited.insert(id.0) {
            return;
        }
        let Some(item) = self.krate.index.get(&id) else {
            return; // not a local item (external / stripped): handled elsewhere
        };
        let name = item.name.clone().unwrap_or_default();

        match &item.inner {
            ItemEnum::Module(module) => {
                let canonical = join(prefix, &name);
                let entity_id = EntityId::module(&self.crate_name, &canonical);
                self.record(Node {
                    raw_id: id,
                    entity_id: entity_id.clone(),
                    kind: "module".to_string(),
                    qualified_name: format!("{}::{canonical}", self.crate_name),
                    label: name,
                    parent: Some(parent.clone()),
                });
                let children = module.items.clone();
                for child in children {
                    self.walk_item(child, &canonical, &entity_id);
                }
            }
            ItemEnum::Struct(strukt) => {
                let canonical = join(prefix, &name);
                let entity_id = self.record_item("struct", &canonical, &name, parent, id);
                let field_ids = struct_field_ids(&strukt.kind);
                for field_id in field_ids {
                    self.walk_field(field_id, &canonical, &entity_id);
                }
                let impls = strukt.impls.clone();
                for impl_id in impls {
                    self.walk_impl(impl_id, &entity_id);
                }
            }
            ItemEnum::Enum(enumeration) => {
                let canonical = join(prefix, &name);
                let entity_id = self.record_item("enum", &canonical, &name, parent, id);
                let variants = enumeration.variants.clone();
                for variant_id in variants {
                    self.walk_variant(variant_id, &canonical, &entity_id);
                }
                let impls = enumeration.impls.clone();
                for impl_id in impls {
                    self.walk_impl(impl_id, &entity_id);
                }
            }
            ItemEnum::Union(union) => {
                let canonical = join(prefix, &name);
                let entity_id = self.record_item("union", &canonical, &name, parent, id);
                let fields = union.fields.clone();
                for field_id in fields {
                    self.walk_field(field_id, &canonical, &entity_id);
                }
                let impls = union.impls.clone();
                for impl_id in impls {
                    self.walk_impl(impl_id, &entity_id);
                }
            }
            ItemEnum::Trait(tr) => {
                let canonical = join(prefix, &name);
                let entity_id = self.record_item("trait", &canonical, &name, parent, id);
                let items = tr.items.clone();
                for item_id in items {
                    self.walk_trait_item(item_id, &canonical, &entity_id);
                }
            }
            ItemEnum::Function(_) => {
                let canonical = join(prefix, &name);
                self.record_item("function", &canonical, &name, parent, id);
            }
            ItemEnum::TypeAlias(_) => {
                let canonical = join(prefix, &name);
                self.record_item("type_alias", &canonical, &name, parent, id);
            }
            ItemEnum::Constant { .. } => {
                let canonical = join(prefix, &name);
                self.record_item("constant", &canonical, &name, parent, id);
            }
            ItemEnum::Static(_) => {
                let canonical = join(prefix, &name);
                self.record_item("static", &canonical, &name, parent, id);
            }
            ItemEnum::Macro(_) | ItemEnum::ProcMacro(_) => {
                let canonical = join(prefix, &name);
                self.record_item("macro", &canonical, &name, parent, id);
            }
            ItemEnum::Use(use_) => {
                // Re-exports produce a relation in the build pass, not an entity.
                self.reexports.push((parent.clone(), use_.clone()));
            }
            ItemEnum::ExternCrate { .. } => {
                // `extern crate` declarations are not modeled as entities.
            }
            _ => {
                self.diagnostics.push(warn(
                    code::UNSUPPORTED_RUSTDOC_ITEM,
                    format!("unsupported rustdoc item `{}` ({})", name, raw_id_label(id)),
                    Some(parent.clone()),
                ));
            }
        }
    }

    fn record_item(
        &mut self,
        kind: &str,
        canonical: &str,
        name: &str,
        parent: &EntityId,
        id: Id,
    ) -> EntityId {
        let entity_id = EntityId::item(kind, &self.crate_name, canonical);
        self.record(Node {
            raw_id: id,
            entity_id: entity_id.clone(),
            kind: kind.to_string(),
            qualified_name: format!("{}::{canonical}", self.crate_name),
            label: name.to_string(),
            parent: Some(parent.clone()),
        });
        entity_id
    }

    fn walk_field(&mut self, id: Id, container_canonical: &str, container: &EntityId) {
        if !self.visited.insert(id.0) {
            return;
        }
        let Some(item) = self.krate.index.get(&id) else {
            return;
        };
        let name = item.name.clone().unwrap_or_default();
        let canonical = join(container_canonical, &name);
        self.record_item("field", &canonical, &name, container, id);
    }

    fn walk_variant(&mut self, id: Id, enum_canonical: &str, container: &EntityId) {
        if !self.visited.insert(id.0) {
            return;
        }
        let Some(item) = self.krate.index.get(&id) else {
            return;
        };
        let name = item.name.clone().unwrap_or_default();
        let canonical = join(enum_canonical, &name);
        let variant_entity = self.record_item("variant", &canonical, &name, container, id);
        if let ItemEnum::Variant(variant) = &item.inner {
            for field_id in variant_field_ids(&variant.kind) {
                self.walk_field(field_id, &canonical, &variant_entity);
            }
        }
    }

    fn walk_trait_item(&mut self, id: Id, trait_canonical: &str, container: &EntityId) {
        if !self.visited.insert(id.0) {
            return;
        }
        let Some(item) = self.krate.index.get(&id) else {
            return;
        };
        let name = item.name.clone().unwrap_or_default();
        let kind = assoc_item_kind(&item.inner);
        let canonical = join(trait_canonical, &name);
        self.record_item(kind, &canonical, &name, container, id);
    }

    fn walk_impl(&mut self, id: Id, self_entity: &EntityId) {
        if !self.visited.insert(id.0) {
            return;
        }
        let Some(item) = self.krate.index.get(&id) else {
            return;
        };
        let ItemEnum::Impl(imp) = &item.inner else {
            return;
        };
        let trait_or_inherent = impl_trait_or_inherent(imp);
        let for_display = impl_for_display(imp);
        let signature = impl_signature(self.krate, imp);
        let entity_id = EntityId::impl_block(
            &self.crate_name,
            &trait_or_inherent,
            &for_display,
            &signature,
        );
        let label = if trait_or_inherent == "inherent" {
            format!("impl {for_display}")
        } else {
            format!("impl {trait_or_inherent} for {for_display}")
        };
        self.record(Node {
            raw_id: id,
            entity_id: entity_id.clone(),
            kind: "impl".to_string(),
            qualified_name: label.clone(),
            label,
            parent: Some(self_entity.clone()),
        });
        let items = imp.items.clone();
        for item_id in items {
            self.walk_impl_item(item_id, &entity_id);
        }
    }

    fn walk_impl_item(&mut self, id: Id, impl_entity: &EntityId) {
        if !self.visited.insert(id.0) {
            return;
        }
        let Some(item) = self.krate.index.get(&id) else {
            return;
        };
        let name = item.name.clone().unwrap_or_default();
        let kind = assoc_item_kind(&item.inner);
        // Impl blocks have no canonical path; scope the id under the impl entity so
        // same-named methods in different impls never collide.
        let entity_id = EntityId::from_raw(format!("{}::{name}", impl_entity.as_str()));
        self.entity_ids.insert(id.0, entity_id.clone());
        self.nodes.push(Node {
            raw_id: id,
            entity_id,
            kind: kind.to_string(),
            qualified_name: format!("{}::{name}", impl_entity.as_str()),
            label: name,
            parent: Some(impl_entity.clone()),
        });
    }

    // ---- Pass 2: entity + relation construction -------------------------

    #[allow(clippy::type_complexity)]
    fn build(&mut self) -> (Vec<Entity>, Vec<Relation>, Vec<UnresolvedTypeRef>) {
        let mut entities = Vec::new();
        let mut relations = Vec::new();
        let mut unresolved = Vec::new();
        let mut diagnostics = Vec::new();

        // Take the nodes so we can mutably borrow `self` for diagnostics.
        let nodes = std::mem::take(&mut self.nodes);

        for node in &nodes {
            let item = self.krate.index.get(&node.raw_id);
            let mut entity = Entity::new(
                node.entity_id.clone(),
                EntityKind::new(node.kind.clone()),
                LocalizedText::new(node.label.clone()),
                node.qualified_name.clone(),
                Provenance::Discovered,
            );
            entity.parent = node.parent.clone();

            if let Some(item) = item {
                self.attach_common(&mut entity, item, &mut diagnostics);
                self.attach_kind_specific(
                    &mut entity,
                    &node.entity_id,
                    item,
                    &mut relations,
                    &mut unresolved,
                );
            }

            // Containment from parent.
            if let Some(parent) = &node.parent {
                relations.push(contains(parent, &node.entity_id));
            }

            entities.push(entity);
        }

        // Re-exports: one canonical entity already exists; add a `re_exports`
        // edge from the exporting module to it, and collect alias names.
        let mut aliases: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let reexports = std::mem::take(&mut self.reexports);
        for (module, use_) in reexports {
            let Some(target_raw) = use_.id else {
                continue; // primitive re-export (`pub use i32 as _`): no entity
            };
            match self.entity_ids.get(&target_raw.0) {
                Some(target) => {
                    let mut relation =
                        typed(RelationKind::RE_EXPORTS, &module, target, "re_export");
                    let last = use_.source.rsplit("::").next().unwrap_or(&use_.source);
                    if use_.name != last {
                        relation
                            .attributes
                            .insert("alias".into(), use_.name.clone().into());
                    }
                    relations.push(relation);
                    aliases
                        .entry(target.as_str().to_string())
                        .or_default()
                        .insert(use_.name.clone());
                }
                None => diagnostics.push(warn(
                    code::REEXPORT_TARGET_MISSING,
                    format!(
                        "re-export target `{}` is missing from the index",
                        use_.source
                    ),
                    Some(module.clone()),
                )),
            }
        }
        for entity in &mut entities {
            if let Some(names) = aliases.get(entity.id.as_str()) {
                let list: Vec<cratevista_schema::AttrValue> = names
                    .iter()
                    .map(|name| cratevista_schema::AttrValue::String(name.clone()))
                    .collect();
                entity
                    .attributes
                    .insert("aliases".into(), cratevista_schema::AttrValue::Array(list));
            }
        }

        self.diagnostics.append(&mut diagnostics);
        (entities, relations, unresolved)
    }

    fn attach_common(
        &self,
        entity: &mut Entity,
        item: &Item,
        diagnostics: &mut Vec<DocumentDiagnostic>,
    ) {
        // Docs.
        if let Some(docs) = &item.docs {
            let documented = !docs.trim().is_empty();
            let summary = first_paragraph(docs);
            entity.docs = Some(DocBlock {
                markdown: docs.clone(),
                summary,
                documented,
            });
        }
        // Visibility.
        entity
            .attributes
            .insert("visibility".into(), visibility_str(&item.visibility).into());
        if item.deprecation.is_some() {
            entity.attributes.insert("deprecated".into(), true.into());
        }
        // Source span.
        if let Some(span) = &item.span {
            match map_span(self.ctx, span) {
                Ok(location) => entity.source = Some(location),
                Err(SpanOmission::OutsideWorkspace) => diagnostics.push(warn(
                    code::SOURCE_OUTSIDE_WORKSPACE,
                    format!(
                        "source of `{}` is outside the workspace root",
                        entity.qualified_name
                    ),
                    Some(entity.id.clone()),
                )),
                Err(SpanOmission::Generated) => diagnostics.push(warn(
                    code::GENERATED_SOURCE_OMITTED,
                    format!(
                        "source of `{}` is generated/synthetic",
                        entity.qualified_name
                    ),
                    Some(entity.id.clone()),
                )),
                Err(SpanOmission::Invalid(reason)) => diagnostics.push(warn(
                    code::MISSING_CANONICAL_PATH,
                    format!(
                        "source of `{}` is not repo-relative: {reason}",
                        entity.qualified_name
                    ),
                    Some(entity.id.clone()),
                )),
            }
        }
    }

    fn attach_kind_specific(
        &self,
        entity: &mut Entity,
        entity_id: &EntityId,
        item: &Item,
        relations: &mut Vec<Relation>,
        unresolved: &mut Vec<UnresolvedTypeRef>,
    ) {
        match &item.inner {
            ItemEnum::Impl(imp) => {
                let for_display = impl_for_display(imp);
                entity
                    .attributes
                    .insert("impl_for".into(), for_display.clone().into());
                entity
                    .attributes
                    .insert("impl_trait".into(), impl_trait_or_inherent(imp).into());
                if impl_is_synthetic_or_blanket(imp) {
                    let kind = if imp.blanket_impl.is_some() {
                        "blanket"
                    } else {
                        "auto"
                    };
                    entity.attributes.insert("synthetic".into(), kind.into());
                }
                // implemented_for: impl -> self type (intra-crate only).
                if let Some(self_id) = head_id(&imp.for_)
                    && let Some(to) = self.entity_ids.get(&self_id.0)
                {
                    relations.push(typed(
                        RelationKind::IMPLEMENTED_FOR,
                        entity_id,
                        to,
                        "impl_for",
                    ));
                }
                // implements: impl -> trait (intra-crate only).
                if let Some(trait_path) = &imp.trait_ {
                    if let Some(to) = self.entity_ids.get(&trait_path.id.0) {
                        relations.push(typed(
                            RelationKind::IMPLEMENTS,
                            entity_id,
                            to,
                            "impl_trait",
                        ));
                    } else {
                        unresolved.push(self.unresolved_ref(
                            entity_id,
                            TypeReferenceRole::ImplTrait,
                            trait_path,
                            trait_path.path.clone(),
                        ));
                    }
                }
            }
            ItemEnum::StructField(ty) => {
                self.type_edge(
                    entity_id,
                    ty,
                    RelationKind::HAS_FIELD_TYPE,
                    TypeReferenceRole::Field,
                    relations,
                    unresolved,
                );
            }
            ItemEnum::Function(function) => {
                self.attach_function(entity, entity_id, function, relations, unresolved);
            }
            _ => {}
        }
    }

    fn attach_function(
        &self,
        entity: &mut Entity,
        entity_id: &EntityId,
        function: &rustdoc_types::Function,
        relations: &mut Vec<Relation>,
        unresolved: &mut Vec<UnresolvedTypeRef>,
    ) {
        // Signature attributes.
        let inputs: Vec<AttrValue> = function
            .sig
            .inputs
            .iter()
            .map(|(name, ty)| AttrValue::String(format!("{name}: {}", display_type(ty))))
            .collect();
        entity
            .attributes
            .insert("inputs".into(), AttrValue::Array(inputs));
        if let Some(output) = &function.sig.output {
            entity
                .attributes
                .insert("output".into(), display_type(output).into());
            entity
                .attributes
                .insert("is_result".into(), result_ok_err(output).is_some().into());
        }

        for (_name, ty) in &function.sig.inputs {
            self.type_edge(
                entity_id,
                ty,
                RelationKind::ACCEPTS_TYPE,
                TypeReferenceRole::Parameter,
                relations,
                unresolved,
            );
        }
        if let Some(output) = &function.sig.output {
            if let Some((_ok, err)) = result_ok_err(output) {
                self.type_edge(
                    entity_id,
                    err,
                    RelationKind::ERROR_TYPE,
                    TypeReferenceRole::Error,
                    relations,
                    unresolved,
                );
            }
            self.type_edge(
                entity_id,
                output,
                RelationKind::RETURNS_TYPE,
                TypeReferenceRole::Return,
                relations,
                unresolved,
            );
        }
    }

    /// Emits a typed relation to a type's head nominal entity when it resolves
    /// within this crate; otherwise preserves a **structured** unresolved
    /// reference (never re-parsing `display`).
    fn type_edge(
        &self,
        from: &EntityId,
        ty: &Type,
        kind: &str,
        role: TypeReferenceRole,
        relations: &mut Vec<Relation>,
        unresolved: &mut Vec<UnresolvedTypeRef>,
    ) {
        let reference = type_ref(ty);
        let Some(path) = reference.head else {
            return; // primitive/generic/tuple/etc.: no nominal edge
        };
        if let Some(to) = self.entity_ids.get(&path.id.0) {
            relations.push(typed(kind, from, to, role.relation_role()));
        } else {
            unresolved.push(self.unresolved_ref(from, role, path, reference.display.clone()));
        }
    }

    /// Builds a structured [`UnresolvedTypeRef`] from a rustdoc `Path`, using the
    /// crate's `paths` map (`ItemSummary`) and `external_crates` for the crate
    /// name — never re-parsing the display string, never leaking a numeric id or
    /// an absolute path. When rustdoc has no `ItemSummary` for the id, the
    /// structured fields stay `None` and only `role`/`display` are preserved.
    fn unresolved_ref(
        &self,
        from: &EntityId,
        role: TypeReferenceRole,
        path: &Path,
        display: String,
    ) -> UnresolvedTypeRef {
        let (crate_name, canonical_path, item_kind) = match self.krate.paths.get(&path.id) {
            Some(summary) => {
                let crate_name = if summary.crate_id == self.local_crate_id {
                    Some(self.crate_name.clone())
                } else {
                    self.krate
                        .external_crates
                        .get(&summary.crate_id)
                        .map(|external| external.name.clone())
                };
                (
                    crate_name,
                    Some(summary.path.clone()),
                    Some(map_item_kind(summary.kind)),
                )
            }
            None => (None, None, None),
        };
        UnresolvedTypeRef {
            from: from.clone(),
            role,
            crate_name,
            canonical_path,
            item_kind,
            display,
        }
    }
}

/// Maps a rustdoc `ItemKind` to an open schema [`EntityKind`]. Deterministic;
/// carries no numeric id.
fn map_item_kind(kind: ItemKind) -> EntityKind {
    let name = match kind {
        ItemKind::Module => "module",
        ItemKind::ExternCrate => "extern_crate",
        ItemKind::Use => "use",
        ItemKind::Struct => "struct",
        ItemKind::StructField => "field",
        ItemKind::Union => "union",
        ItemKind::Enum => "enum",
        ItemKind::Variant => "variant",
        ItemKind::Function => "function",
        ItemKind::TypeAlias => "type_alias",
        ItemKind::Constant => "constant",
        ItemKind::Trait => "trait",
        ItemKind::TraitAlias => "trait_alias",
        ItemKind::Impl => "impl",
        ItemKind::Static => "static",
        ItemKind::ExternType => "extern_type",
        ItemKind::Macro => "macro",
        ItemKind::ProcAttribute => "macro",
        ItemKind::ProcDerive => "macro",
        ItemKind::AssocConst => "assoc_const",
        ItemKind::AssocType => "assoc_type",
        ItemKind::Primitive => "primitive",
        ItemKind::Keyword => "keyword",
        ItemKind::Attribute => "attribute",
    };
    EntityKind::new(name)
}

// ---- free helpers -------------------------------------------------------

fn contains(from: &EntityId, to: &EntityId) -> Relation {
    Relation::new(
        RelationKind::new(RelationKind::CONTAINS),
        from.clone(),
        to.clone(),
        Provenance::Discovered,
    )
}

fn typed(kind: &str, from: &EntityId, to: &EntityId, role: &str) -> Relation {
    let kind = RelationKind::new(kind);
    let id = RelationId::with_role(&kind, from, to, role);
    Relation {
        id,
        kind,
        from: from.clone(),
        to: to.clone(),
        role: Some(role.to_string()),
        label: None,
        provenance: Provenance::Discovered,
        attributes: Default::default(),
    }
}

fn visibility_str(visibility: &Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Default => "default",
        Visibility::Crate => "crate",
        Visibility::Restricted { .. } => "restricted",
    }
}

fn first_paragraph(docs: &str) -> Option<String> {
    let trimmed = docs.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let paragraph = trimmed
        .split("\n\n")
        .next()
        .unwrap_or(trimmed)
        .replace('\n', " ")
        .trim()
        .to_string();
    if paragraph.is_empty() {
        None
    } else {
        Some(paragraph)
    }
}

fn struct_field_ids(kind: &StructKind) -> Vec<Id> {
    match kind {
        StructKind::Unit => Vec::new(),
        StructKind::Tuple(fields) => fields.iter().flatten().copied().collect(),
        StructKind::Plain { fields, .. } => fields.clone(),
    }
}

fn variant_field_ids(kind: &VariantKind) -> Vec<Id> {
    match kind {
        VariantKind::Plain => Vec::new(),
        VariantKind::Tuple(fields) => fields.iter().flatten().copied().collect(),
        VariantKind::Struct { fields, .. } => fields.clone(),
    }
}

/// Drops entities that share an id (keeping the first), emitting a
/// `duplicate_item_identity` diagnostic per drop.
fn dedup_entities(entities: Vec<Entity>, diagnostics: &mut Vec<DocumentDiagnostic>) -> Vec<Entity> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut kept = Vec::with_capacity(entities.len());
    for entity in entities {
        if seen.insert(entity.id.as_str().to_string()) {
            kept.push(entity);
        } else {
            diagnostics.push(warn(
                code::DUPLICATE_ITEM_IDENTITY,
                format!("duplicate item identity `{}`; keeping the first", entity.id),
                Some(entity.id.clone()),
            ));
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use rustdoc_types::{
        ExternalCrate, Generics, ItemSummary, Module, Path as RdPath, Struct, StructKind, Target,
        Type, Visibility,
    };

    fn ctx() -> NormalizeContext {
        NormalizeContext {
            workspace_root: "/w".into(),
            package_root: "/w/tiny".into(),
            package_id: EntityId::package("tiny"),
            target_id: EntityId::target("tiny", "lib", "tiny"),
            package_name: "tiny".into(),
            crate_name: "tiny".into(),
            target_name: "tiny".into(),
            target_kind: crate::options::RustdocTargetKind::Library,
            toolchain: "nightly-test".into(),
        }
    }

    fn item(id: u32, name: &str, inner: ItemEnum) -> Item {
        Item {
            id: Id(id),
            crate_id: 0,
            name: Some(name.to_string()),
            span: None,
            visibility: Visibility::Public,
            docs: None,
            links: HashMap::new(),
            attrs: Vec::new(),
            deprecation: None,
            stability: None,
            const_stability: None,
            inner,
        }
    }

    fn resolved(id: u32, path: &str) -> Type {
        Type::ResolvedPath(RdPath {
            path: path.to_string(),
            id: Id(id),
            args: None,
        })
    }

    fn empty_generics() -> Generics {
        Generics {
            params: Vec::new(),
            where_predicates: Vec::new(),
        }
    }

    /// A minimal crate: a root module containing one struct `Holder` with two
    /// fields — `known` referencing an external item present in `paths`, and
    /// `mystery` referencing an id absent from `paths` (no structured evidence).
    fn crate_with_fields() -> Crate {
        let mut index: HashMap<Id, Item> = HashMap::new();
        index.insert(
            Id(0),
            item(
                0,
                "tiny",
                ItemEnum::Module(Module {
                    is_crate: true,
                    items: vec![Id(1)],
                    is_stripped: false,
                }),
            ),
        );
        index.insert(
            Id(1),
            item(
                1,
                "Holder",
                ItemEnum::Struct(Struct {
                    kind: StructKind::Plain {
                        fields: vec![Id(2), Id(3)],
                        has_stripped_fields: false,
                    },
                    generics: empty_generics(),
                    impls: Vec::new(),
                }),
            ),
        );
        index.insert(
            Id(2),
            item(
                2,
                "known",
                ItemEnum::StructField(resolved(900, "other::Widget")),
            ),
        );
        index.insert(
            Id(3),
            item(
                3,
                "mystery",
                ItemEnum::StructField(resolved(950, "Mystery")),
            ),
        );

        let mut paths: HashMap<Id, ItemSummary> = HashMap::new();
        paths.insert(
            Id(0),
            ItemSummary {
                crate_id: 0,
                path: vec!["tiny".into()],
                kind: ItemKind::Module,
            },
        );
        paths.insert(
            Id(1),
            ItemSummary {
                crate_id: 0,
                path: vec!["tiny".into(), "Holder".into()],
                kind: ItemKind::Struct,
            },
        );
        // `Widget` (id 900) is external and summarized; `mystery` (id 950) is not.
        paths.insert(
            Id(900),
            ItemSummary {
                crate_id: 5,
                path: vec!["other".into(), "Widget".into()],
                kind: ItemKind::Struct,
            },
        );

        let mut external_crates: HashMap<u32, ExternalCrate> = HashMap::new();
        external_crates.insert(
            5,
            ExternalCrate {
                name: "other".into(),
                html_root_url: None,
                path: "/some/abs/other.rmeta".into(),
            },
        );

        Crate {
            root: Id(0),
            crate_version: None,
            includes_private: false,
            index,
            paths,
            external_crates,
            target: Target {
                triple: "x86_64".into(),
                target_features: Vec::new(),
            },
            format_version: rustdoc_types::FORMAT_VERSION,
        }
    }

    #[test]
    fn crate_summary_carries_stable_identities() {
        let ingest = normalize_crate(&crate_with_fields(), &ctx()).unwrap();
        assert_eq!(ingest.summary.package_id.as_str(), "package:tiny");
        assert_eq!(ingest.summary.target_id.as_str(), "target:tiny:lib:tiny");
        assert_eq!(ingest.summary.root_module_id.as_str(), "module:tiny::tiny");
    }

    #[test]
    fn structured_external_reference_has_crate_path_and_kind() {
        let ingest = normalize_crate(&crate_with_fields(), &ctx()).unwrap();
        let known = ingest
            .summary
            .unresolved_refs
            .iter()
            .find(|r| r.display == "other::Widget")
            .expect("the external Widget reference is preserved");
        assert_eq!(known.role, TypeReferenceRole::Field);
        assert_eq!(known.crate_name.as_deref(), Some("other"));
        assert_eq!(
            known.canonical_path.as_deref(),
            Some(&["other".to_string(), "Widget".to_string()][..])
        );
        assert_eq!(known.item_kind.as_ref().map(|k| k.as_str()), Some("struct"));
    }

    #[test]
    fn reference_without_summary_has_empty_structured_fields() {
        let ingest = normalize_crate(&crate_with_fields(), &ctx()).unwrap();
        let mystery = ingest
            .summary
            .unresolved_refs
            .iter()
            .find(|r| r.display == "Mystery")
            .expect("the unsummarized reference is preserved");
        assert_eq!(mystery.role, TypeReferenceRole::Field);
        assert_eq!(mystery.crate_name, None);
        assert_eq!(mystery.canonical_path, None);
        assert_eq!(mystery.item_kind, None);
    }

    #[test]
    fn unresolved_refs_are_deterministically_sorted_and_leak_no_numeric_ids() {
        let ingest = normalize_crate(&crate_with_fields(), &ctx()).unwrap();
        let refs = &ingest.summary.unresolved_refs;
        let mut sorted = refs.clone();
        sorted.sort();
        assert_eq!(*refs, sorted, "unresolved refs are emitted in sorted order");
        // No raw numeric rustdoc id (900/950) leaks into any structured field.
        for reference in refs {
            assert!(!reference.display.contains("900"));
            assert!(!reference.display.contains("950"));
            if let Some(path) = &reference.canonical_path {
                for component in path {
                    assert!(
                        component.parse::<u32>().is_err(),
                        "path component is a name, not an id"
                    );
                }
            }
        }
    }
}
