//! Raw validated config → [`cratevista_graph::GraphOverlay`].
//!
//! This is the one place that knows both the TOML shape and the graph seam, and
//! the dependency runs **one way**: config → graph. The graph never learns that
//! TOML exists.
//!
//! # What this does not do
//!
//! - **No filesystem reads.** File-backed docs and examples (`[[flow]].docs`,
//!   `[[flow.example]].path`, `[[override]].docs`) are left unresolved for step
//!   4; views come out with `docs: None` / `examples: []`. A manual entity's
//!   `source` *path* is still mapped, because validating a path is pure string
//!   work — it opens nothing.
//! - **No resolution of discovered ids.** A member, relation endpoint, focus or
//!   override target naming a discovered entity is passed through **verbatim**.
//!   PRD 05 checks those against the real entity set (`invalid_view_reference`,
//!   `dangling_relation`, `overlay_target_missing`); re-checking here would make
//!   two sources of truth for one rule.
//!
//! # Determinism
//!
//! Identical input always yields an identical overlay. Files are already visited
//! in sorted order, and within a file everything keeps the author's order —
//! except stages, which are ordered by their explicit `order` field, since that
//! is where a flow's narrative actually lives (declaration position is
//! incidental). Author order is preserved where it *is* the narrative: flow
//! members, relations and examples.

use std::collections::BTreeMap;

use cratevista_graph::{EntityOverride, GraphOverlay};
use cratevista_schema::{
    AttrValue, Entity, EntityId, EntityKind, LocalizedText, Provenance, Relation, RelationId,
    RelationKind, RepoRelativePath, SourceLocation, Stage, StageId, View, ViewId,
};
use serde_spanned::Spanned;

use crate::error::{ConfigDiagnostic, code};
use crate::model::{
    LoadedFile, RawConfig, RawEntity, RawFlow, RawFlowFile, RawLocalized, RawOverride,
    RawOverrideFile, RawRelation, RawValue,
};
use crate::validate::{Validation, manual_entity_id};

/// The default relation kind when a `[[flow.relation]]` names none.
pub const DEFAULT_RELATION_KIND: &str = "manual";

/// The result of converting a configuration.
#[derive(Debug, Default)]
pub struct OverlayOutcome {
    /// The overlay to hand to `cratevista_graph::build_document`.
    pub overlay: GraphOverlay,
    /// Problems found while converting. All non-fatal.
    pub diagnostics: Vec<ConfigDiagnostic>,
}

/// Localized text: a bare string becomes the default translation.
fn localized(raw: &RawLocalized) -> LocalizedText {
    match raw {
        RawLocalized::Plain(text) => LocalizedText::new(text.clone()),
        RawLocalized::Translations(map) => {
            let mut text = LocalizedText::new(map.get("default").cloned().unwrap_or_default());
            for (language, value) in map {
                if language != "default" {
                    text.translations.insert(language.clone(), value.clone());
                }
            }
            text
        }
    }
}

/// TOML value → schema attribute value.
///
/// Written out rather than round-tripped through serde: TOML has a datetime type
/// JSON does not, and an explicit arm makes that lossy step visible instead of
/// letting a serializer decide silently.
fn attr_value(value: &RawValue) -> AttrValue {
    match value {
        RawValue::String(text) => AttrValue::String(text.clone()),
        RawValue::Integer(number) => AttrValue::from(*number),
        RawValue::Float(number) => serde_json::Number::from_f64(*number)
            .map(AttrValue::Number)
            // NaN/inf have no JSON representation; keep the text rather than
            // silently dropping the attribute.
            .unwrap_or_else(|| AttrValue::String(number.to_string())),
        RawValue::Boolean(flag) => AttrValue::Bool(*flag),
        // JSON has no date type: a datetime becomes its TOML spelling.
        RawValue::Datetime(datetime) => AttrValue::String(datetime.to_string()),
        RawValue::Array(items) => AttrValue::Array(items.iter().map(attr_value).collect()),
        RawValue::Table(table) => AttrValue::Object(
            table
                .iter()
                .map(|(key, value)| (key.clone(), attr_value(value)))
                .collect(),
        ),
    }
}

fn located<T>(
    file: &LoadedFile<RawFlowFile>,
    spanned: &Spanned<T>,
    code: &'static str,
    message: String,
) -> ConfigDiagnostic {
    ConfigDiagnostic::new(code, message, &file.path).at_position(crate::error::position_of(
        &file.source,
        spanned.span().start,
    ))
}

fn located_override<T>(
    file: &LoadedFile<RawOverrideFile>,
    spanned: &Spanned<T>,
    code: &'static str,
    message: String,
) -> ConfigDiagnostic {
    ConfigDiagnostic::new(code, message, &file.path).at_position(crate::error::position_of(
        &file.source,
        spanned.span().start,
    ))
}

/// Maps one `[[entity]]` onto a schema entity.
///
/// The config-local id becomes `manual:<id>`; the id itself becomes the
/// `qualified_name`, so a manual entity is searchable by the name its author
/// gave it (matching the canonical `manual_flow` fixture).
fn build_entity(
    file: &LoadedFile<RawFlowFile>,
    raw: &RawEntity,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> Entity {
    let config_id = raw.id.get_ref().trim();
    let mut entity = Entity::new(
        EntityId::from_raw(manual_entity_id(config_id)),
        EntityKind::new(raw.kind.get_ref().trim()),
        localized(&raw.label),
        config_id,
        Provenance::Manual,
    );
    entity.description = raw.description.as_ref().map(localized);
    entity.tags = {
        let mut tags = raw.tags.clone();
        tags.sort();
        tags.dedup();
        tags
    };
    entity.attributes = raw
        .attributes
        .iter()
        .map(|(key, value)| (key.clone(), attr_value(value)))
        .collect();

    // A path, not a file read: `RepoRelativePath` is pure string validation, so
    // this stays inside step 3's no-IO rule while still refusing traversal.
    if let Some(path) = &raw.source {
        match RepoRelativePath::new(path) {
            Ok(repo_relative) => entity.source = Some(SourceLocation::new(repo_relative, None)),
            Err(error) => diagnostics.push(located(
                file,
                &raw.id,
                code::INVALID_SOURCE_PATH,
                format!("entity `{config_id}` has an invalid `source` path: {error}"),
            )),
        }
    }
    entity
}

/// Maps `[[flow.stage]]`s, ordered by their explicit `order`.
fn build_stages(flow: &RawFlow) -> Vec<Stage> {
    let mut stages: Vec<(u32, Stage)> = flow
        .stages
        .iter()
        .map(|raw| {
            let order = *raw.order.get_ref();
            (
                order,
                Stage {
                    id: StageId::from_raw(format!("stage:{}", raw.id.get_ref().trim())),
                    title: localized(&raw.title),
                    order,
                },
            )
        })
        .collect();
    // `validate` guarantees unique orders, so this is a total order and the sort
    // is deterministic regardless of declaration position.
    stages.sort_by_key(|(order, _)| *order);
    stages.into_iter().map(|(_, stage)| stage).collect()
}

/// Maps one `[[flow.relation]]`.
///
/// `role` is part of the id (`RelationId::with_role`), which is what lets two
/// edges between the same pair coexist. Endpoints are passed through verbatim:
/// whether they name real entities is PRD 05's question.
fn build_relation(raw: &RawRelation) -> Relation {
    let kind = RelationKind::new(raw.kind.as_deref().unwrap_or(DEFAULT_RELATION_KIND));
    let from = EntityId::from_raw(raw.from.get_ref().trim());
    let to = EntityId::from_raw(raw.to.get_ref().trim());
    let mut relation = Relation::new(kind.clone(), from.clone(), to.clone(), Provenance::Manual);
    if let Some(role) = &raw.role {
        let role = role.get_ref().trim();
        relation.id = RelationId::with_role(&kind, &from, &to, role);
        relation.role = Some(role.to_string());
    }
    relation.label = raw.label.as_ref().map(localized);
    relation.attributes = raw
        .attributes
        .iter()
        .map(|(key, value)| (key.clone(), attr_value(value)))
        .collect();
    relation
}

/// Maps one `[[flow]]` onto a schema view.
///
/// Membership is **explicit** (`entity_ids`), so a flow shows exactly what its
/// author listed rather than a kind filter. `docs`/`examples` stay empty: they
/// are file-backed and belong to step 4.
fn build_view(flow: &RawFlow) -> View {
    View {
        id: ViewId::view(flow.id.get_ref().trim()),
        title: localized(&flow.title),
        description: flow.description.as_ref().map(localized),
        // A flow selects its members by id, so the kind filters stay empty.
        entity_kinds: Vec::new(),
        relation_kinds: Vec::new(),
        entity_ids: Some(
            flow.members
                .iter()
                .map(|member| EntityId::from_raw(member.get_ref().trim()))
                .collect(),
        ),
        stages: build_stages(flow),
        default_focus: flow
            .default_focus
            .as_ref()
            .map(|focus| EntityId::from_raw(focus.get_ref().trim())),
        presentation: BTreeMap::new(),
        // Step 4 resolves and embeds these; step 3 opens no files.
        docs: None,
        examples: Vec::new(),
    }
}

/// Maps one `[[override]]` onto the real `EntityOverride` fields.
///
/// `category`/`stage`/`promoted`/`presentation` have no dedicated field, so they
/// become attributes — `stage` genuinely *is* one (the UI reads
/// `attributes["stage"]`). `docs` is file-backed and left to step 4.
fn build_override(raw: &RawOverride) -> EntityOverride {
    let mut set_attributes: BTreeMap<String, AttrValue> = BTreeMap::new();
    // Explicit keys first, then the free-form table, so `presentation` can
    // override a shorthand rather than being silently outranked by key order.
    if let Some(category) = &raw.category {
        set_attributes.insert("category".into(), AttrValue::String(category.clone()));
    }
    if let Some(stage) = &raw.stage {
        set_attributes.insert("stage".into(), AttrValue::String(stage.clone()));
    }
    if let Some(promoted) = raw.promoted {
        set_attributes.insert("promoted".into(), AttrValue::Bool(promoted));
    }
    for (key, value) in &raw.presentation {
        set_attributes.insert(key.clone(), attr_value(value));
    }

    EntityOverride {
        label: raw.label.as_ref().map(localized),
        description: raw.description.as_ref().map(localized),
        add_tags: {
            let mut tags = raw.tags.clone();
            tags.sort();
            tags.dedup();
            tags
        },
        set_attributes,
        hidden: raw.hidden,
        // Step 4 reads and appends these.
        docs: None,
    }
}

/// Merges `next` onto `previous`, last-loaded-wins per field.
///
/// Field-level rather than whole-value replacement: two overrides that set
/// different fields should compose, and `add_tags` is additive by nature. Only a
/// field both set actually conflicts, and there the later one wins — which is
/// what the diagnostic reports.
fn merge_overrides(previous: EntityOverride, next: EntityOverride) -> EntityOverride {
    let mut merged = previous;
    if next.label.is_some() {
        merged.label = next.label;
    }
    if next.description.is_some() {
        merged.description = next.description;
    }
    if next.hidden.is_some() {
        merged.hidden = next.hidden;
    }
    if next.docs.is_some() {
        merged.docs = next.docs;
    }
    merged.add_tags.extend(next.add_tags);
    merged.add_tags.sort();
    merged.add_tags.dedup();
    // Later keys win; keys only the earlier one set survive.
    merged.set_attributes.extend(next.set_attributes);
    merged
}

/// The flows that actually become views, in deterministic order.
///
/// Skips what `validate` rejected — an empty id, or a duplicate (first wins) —
/// so the same set is used by [`build_overlay`] and by [`crate::docs`]. Shared
/// rather than reimplemented: two copies of "which flows count" would drift, and
/// step 4 correlates its file reads against exactly the views step 3 emitted.
pub(crate) fn accepted_flows<'a>(
    config: &'a RawConfig,
    _validation: &Validation,
) -> Vec<(&'a LoadedFile<RawFlowFile>, &'a RawFlow)> {
    let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
    let mut accepted = Vec::new();
    for file in &config.flow_files {
        for flow in &file.value.flows {
            let flow_id = flow.id.get_ref().trim();
            if flow_id.is_empty() || seen.insert(flow_id, ()).is_some() {
                continue;
            }
            accepted.push((file, flow));
        }
    }
    accepted
}

/// Converts a loaded configuration into a `GraphOverlay`.
///
/// `validation` supplies the authoritative set of accepted manual entity ids, so
/// an entity this crate already rejected (empty id, a `manual:`-prefixed id, a
/// duplicate) never reaches the graph and is not diagnosed twice.
pub fn build_overlay(config: &RawConfig, validation: &Validation) -> OverlayOutcome {
    let mut outcome = OverlayOutcome::default();
    let mut emitted_entities: BTreeMap<String, ()> = BTreeMap::new();
    let mut relation_ids: BTreeMap<RelationId, String> = BTreeMap::new();

    for file in &config.flow_files {
        for raw in &file.value.entities {
            let config_id = raw.id.get_ref().trim();
            let entity_id = manual_entity_id(config_id);
            // Only entities validation accepted, and only their first
            // declaration — the same first-wins rule `validate` reported on.
            if !validation.manual_ids.contains(&entity_id) {
                continue;
            }
            if emitted_entities.insert(entity_id, ()).is_some() {
                continue;
            }
            let entity = build_entity(file, raw, &mut outcome.diagnostics);
            outcome.overlay.entities.push(entity);
        }
    }

    // The single definition of "which flows count" — shared with step 4, which
    // must correlate its file reads against exactly these views.
    for (file, flow) in accepted_flows(config, validation) {
        let flow_id = flow.id.get_ref().trim();
        {
            outcome.overlay.views.push(build_view(flow));

            for raw in &flow.relations {
                let relation = build_relation(raw);
                // Two edges deriving one id would silently collapse in the
                // graph's relation merge (it keeps the first and reports
                // "conflicting evidence", which would not tell the author what
                // to actually do). Catch it here, where we can.
                if let Some(first_flow) = relation_ids.get(&relation.id) {
                    outcome.diagnostics.push(located(
                        file,
                        &raw.from,
                        code::DUPLICATE_RELATION,
                        format!(
                            "flow `{flow_id}` repeats the relation `{}` -> `{}` already declared \
                             in flow `{first_flow}`; give one of them a distinct `role` so both \
                             survive",
                            raw.from.get_ref().trim(),
                            raw.to.get_ref().trim()
                        ),
                    ));
                    continue;
                }
                relation_ids.insert(relation.id.clone(), flow_id.to_string());
                outcome.overlay.relations.push(relation);
            }
        }
    }

    for file in &config.override_files {
        for raw in &file.value.overrides {
            let target = raw.target.get_ref().trim();
            if target.is_empty() {
                outcome.diagnostics.push(located_override(
                    file,
                    &raw.target,
                    code::INVALID_ID,
                    "an `[[override]]` target must not be empty".into(),
                ));
                continue;
            }
            // The target is a DISCOVERED id: passed through untouched. PRD 05
            // reports `overlay_target_missing` if it does not exist.
            let id = EntityId::from_raw(target);
            let next = build_override(raw);
            match outcome.overlay.overrides.remove(&id) {
                None => {
                    outcome.overlay.overrides.insert(id, next);
                }
                Some(previous) => {
                    outcome.diagnostics.push(located_override(
                        file,
                        &raw.target,
                        code::DUPLICATE_OVERRIDE,
                        format!(
                            "`{target}` is overridden more than once; the last one loaded wins \
                             per field (files load in sorted order)"
                        ),
                    ));
                    outcome
                        .overlay
                        .overrides
                        .insert(id, merge_overrides(previous, next));
                }
            }
        }
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::load_from;
    use crate::validate::validate;
    use std::path::Path;

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// Loads, validates and converts `(relative path, contents)` pairs.
    fn convert(files: &[(&str, &str)]) -> (OverlayOutcome, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        for (relative, contents) in files {
            write(dir.path(), relative, contents);
        }
        let config = load_from(dir.path());
        assert!(
            config.diagnostics.is_empty(),
            "load: {:?}",
            config.diagnostics
        );
        let validation = validate(&config);
        (build_overlay(&config, &validation), dir)
    }

    fn flows(contents: &str) -> Vec<(&str, &str)> {
        vec![(".cratevista/flows/a.toml", contents)]
    }

    fn codes(outcome: &OverlayOutcome) -> Vec<&str> {
        outcome.diagnostics.iter().map(|d| d.code).collect()
    }

    const FULL: &str = r#"
[[entity]]
id = "redis"
kind = "infrastructure"
label = { default = "Redis", de = "Redis-Cache" }
description = "The cache"
tags = ["infra", "cache", "infra"]
source = "crates/app/src/cache.rs"

  [entity.attributes]
  region = "eu-west-1"
  replicas = 3
  critical = true

[[entity]]
id = "web"
kind = "external_system"
label = "Web client"

[[flow]]
id = "checkout"
title = "Checkout"
description = "How an order is placed."
members = ["manual:web", "manual:redis", "item:struct:app::Order"]
default_focus = "manual:web"

  [[flow.stage]]
  id = "infra"
  title = "Infrastructure"
  order = 2

  [[flow.stage]]
  id = "clients"
  title = "Clients"
  order = 1

  [[flow.relation]]
  from = "manual:web"
  to = "manual:redis"
  role = "http"
  label = "HTTPS"

  [[flow.relation]]
  from = "manual:web"
  to = "manual:redis"
  role = "ws"
  label = "WebSocket"
"#;

    #[test]
    fn manual_entities_map_with_prefix_provenance_and_presentation() {
        let (outcome, _dir) = convert(&flows(FULL));
        assert!(outcome.diagnostics.is_empty(), "{:?}", outcome.diagnostics);

        let redis = outcome
            .overlay
            .entities
            .iter()
            .find(|e| e.id.as_str() == "manual:redis")
            .expect("manual:redis exists");

        assert_eq!(redis.provenance, Provenance::Manual);
        assert_eq!(redis.kind.as_str(), "infrastructure");
        // The config-local id is the qualified name, so it is searchable.
        assert_eq!(redis.qualified_name, "redis");
        assert_eq!(redis.label.default, "Redis");
        assert_eq!(redis.label.translations.get("de").unwrap(), "Redis-Cache");
        assert_eq!(redis.description.as_ref().unwrap().default, "The cache");
        // Tags sorted + deduped.
        assert_eq!(redis.tags, ["cache", "infra"]);
        assert_eq!(
            redis.attributes["region"],
            AttrValue::String("eu-west-1".into())
        );
        assert_eq!(redis.attributes["replicas"], AttrValue::from(3));
        assert_eq!(redis.attributes["critical"], AttrValue::Bool(true));
        // A path is mapped (pure validation), not read.
        assert_eq!(
            redis.source.as_ref().unwrap().path.as_str(),
            "crates/app/src/cache.rs"
        );
    }

    #[test]
    fn a_traversing_source_path_is_diagnosed_and_omitted() {
        let (outcome, _dir) = convert(&flows(
            r#"
[[entity]]
id = "bad"
kind = "manual_block"
label = "Bad"
source = "../../etc/passwd"
"#,
        ));
        assert_eq!(codes(&outcome), [code::INVALID_SOURCE_PATH]);
        // The entity still exists; only the bad path is dropped.
        let entity = &outcome.overlay.entities[0];
        assert_eq!(entity.id.as_str(), "manual:bad");
        assert!(entity.source.is_none());
    }

    #[test]
    fn a_flow_maps_to_a_view_with_explicit_membership_and_focus() {
        let (outcome, _dir) = convert(&flows(FULL));
        let view = &outcome.overlay.views[0];

        assert_eq!(view.id.as_str(), "view:checkout");
        assert_eq!(view.title.default, "Checkout");
        assert_eq!(
            view.description.as_ref().unwrap().default,
            "How an order is placed."
        );
        // Explicit membership, in the author's narrative order, with the
        // discovered id passed through verbatim.
        let members: Vec<&str> = view
            .entity_ids
            .as_ref()
            .unwrap()
            .iter()
            .map(|id| id.as_str())
            .collect();
        assert_eq!(
            members,
            ["manual:web", "manual:redis", "item:struct:app::Order"]
        );
        assert_eq!(view.default_focus.as_ref().unwrap().as_str(), "manual:web");
        // A flow selects members by id, so kind filters stay empty.
        assert!(view.entity_kinds.is_empty());
        assert!(view.relation_kinds.is_empty());
        // File-backed, so step 4's job.
        assert_eq!(view.docs, None);
        assert!(view.examples.is_empty());
    }

    #[test]
    fn stages_are_ordered_by_their_order_field_not_declaration_position() {
        let (outcome, _dir) = convert(&flows(FULL));
        let stages = &outcome.overlay.views[0].stages;
        // Declared infra(2) then clients(1); `order` is where the narrative
        // lives, so the output follows it.
        assert_eq!(stages[0].id.as_str(), "stage:clients");
        assert_eq!(stages[0].order, 1);
        assert_eq!(stages[1].id.as_str(), "stage:infra");
        assert_eq!(stages[1].order, 2);
        assert_eq!(stages[0].title.default, "Clients");
    }

    #[test]
    fn relations_carry_role_label_and_distinct_ids() {
        let (outcome, _dir) = convert(&flows(FULL));
        let relations = &outcome.overlay.relations;
        assert_eq!(
            relations.len(),
            2,
            "two edges between the same pair coexist"
        );

        // Author order preserved: the narrative is HTTPS then WebSocket.
        assert_eq!(relations[0].label.as_ref().unwrap().default, "HTTPS");
        assert_eq!(relations[1].label.as_ref().unwrap().default, "WebSocket");

        for relation in relations {
            assert_eq!(relation.provenance, Provenance::Manual);
            assert_eq!(relation.kind.as_str(), DEFAULT_RELATION_KIND);
            assert_eq!(relation.from.as_str(), "manual:web");
            assert_eq!(relation.to.as_str(), "manual:redis");
        }
        assert_eq!(relations[0].role.as_deref(), Some("http"));
        assert_eq!(relations[1].role.as_deref(), Some("ws"));
        // The role is what keeps them distinct.
        assert_ne!(relations[0].id, relations[1].id);
        assert!(relations[0].id.as_str().ends_with(":http"));
    }

    #[test]
    fn a_relation_without_a_role_uses_the_basic_id_and_the_default_kind() {
        let (outcome, _dir) = convert(&flows(
            r#"
[[flow]]
id = "f"
title = "F"

  [[flow.relation]]
  from = "package:a"
  to = "package:b"
"#,
        ));
        let relation = &outcome.overlay.relations[0];
        assert_eq!(relation.kind.as_str(), "manual");
        assert_eq!(relation.role, None);
        assert_eq!(relation.id.as_str(), "rel:manual:package:a->package:b");
    }

    #[test]
    fn duplicate_relations_are_diagnosed_rather_than_silently_collapsed() {
        // Without a distinct `role` these derive one id, and the graph's merge
        // would keep the first and report "conflicting evidence" — which would
        // not tell the author what to do.
        let (outcome, _dir) = convert(&flows(
            r#"
[[flow]]
id = "f"
title = "F"

  [[flow.relation]]
  from = "manual:a"
  to = "manual:b"
  label = "First"

  [[flow.relation]]
  from = "manual:a"
  to = "manual:b"
  label = "Second"
"#,
        ));
        assert_eq!(codes(&outcome), [code::DUPLICATE_RELATION]);
        assert!(outcome.diagnostics[0].message.contains("distinct `role`"));
        assert_eq!(outcome.overlay.relations.len(), 1, "the first is kept");
        assert_eq!(
            outcome.overlay.relations[0].label.as_ref().unwrap().default,
            "First"
        );
    }

    #[test]
    fn overrides_map_onto_the_real_entity_override_fields() {
        let (outcome, _dir) = convert(&[(
            ".cratevista/overrides/a.toml",
            r#"
[[override]]
target = "item:struct:app::Order"
label = "Order"
description = "An order"
tags = ["featured", "core", "featured"]
category = "domain"
stage = "stage:clients"
hidden = true
promoted = true

  [override.presentation]
  color = "blue"
"#,
        )]);
        assert!(outcome.diagnostics.is_empty(), "{:?}", outcome.diagnostics);

        let id = EntityId::from_raw("item:struct:app::Order");
        let entry = &outcome.overlay.overrides[&id];
        assert_eq!(entry.label.as_ref().unwrap().default, "Order");
        assert_eq!(entry.description.as_ref().unwrap().default, "An order");
        assert_eq!(entry.add_tags, ["core", "featured"], "sorted + deduped");
        assert_eq!(entry.hidden, Some(true));
        // No dedicated fields exist for these, so they become attributes —
        // `stage` genuinely is one (the UI reads attributes["stage"]).
        assert_eq!(
            entry.set_attributes["category"],
            AttrValue::String("domain".into())
        );
        assert_eq!(
            entry.set_attributes["stage"],
            AttrValue::String("stage:clients".into())
        );
        assert_eq!(entry.set_attributes["promoted"], AttrValue::Bool(true));
        assert_eq!(
            entry.set_attributes["color"],
            AttrValue::String("blue".into())
        );
        // File-backed: step 4.
        assert_eq!(entry.docs, None);
    }

    #[test]
    fn an_override_target_is_passed_through_and_never_resolved() {
        let (outcome, _dir) = convert(&[(
            ".cratevista/overrides/a.toml",
            "[[override]]\ntarget = \"item:struct:totally::made::Up\"\nlabel = \"X\"\n",
        )]);
        // PRD 05 emits `overlay_target_missing`; this crate must not pre-empt it.
        assert!(outcome.diagnostics.is_empty());
        assert!(
            outcome
                .overlay
                .overrides
                .contains_key(&EntityId::from_raw("item:struct:totally::made::Up"))
        );
    }

    #[test]
    fn duplicate_overrides_merge_last_loaded_wins_per_field_with_a_diagnostic() {
        let (outcome, _dir) = convert(&[
            (
                ".cratevista/overrides/a_first.toml",
                r#"
[[override]]
target = "item:struct:app::Order"
label = "First label"
description = "First description"
tags = ["from-first"]
category = "first"
"#,
            ),
            (
                ".cratevista/overrides/z_last.toml",
                r#"
[[override]]
target = "item:struct:app::Order"
label = "Last label"
tags = ["from-last"]
category = "last"
hidden = true
"#,
            ),
        ]);
        assert_eq!(codes(&outcome), [code::DUPLICATE_OVERRIDE]);
        assert_eq!(
            outcome.diagnostics[0].file,
            ".cratevista/overrides/z_last.toml"
        );

        let entry = &outcome.overlay.overrides[&EntityId::from_raw("item:struct:app::Order")];
        // Conflicting fields: the last loaded wins.
        assert_eq!(entry.label.as_ref().unwrap().default, "Last label");
        assert_eq!(
            entry.set_attributes["category"],
            AttrValue::String("last".into())
        );
        // Fields only the first set survive — the merge is per field, not
        // wholesale replacement.
        assert_eq!(
            entry.description.as_ref().unwrap().default,
            "First description"
        );
        // Only the last set `hidden`.
        assert_eq!(entry.hidden, Some(true));
        // Tags are additive by nature.
        assert_eq!(entry.add_tags, ["from-first", "from-last"]);
    }

    #[test]
    fn a_manual_entity_declared_in_another_file_is_usable_by_a_flow() {
        let (outcome, _dir) = convert(&[
            (
                ".cratevista/flows/a_entities.toml",
                "[[entity]]\nid = \"redis\"\nkind = \"infrastructure\"\nlabel = \"Redis\"\n",
            ),
            (
                ".cratevista/flows/z_flow.toml",
                "[[flow]]\nid = \"f\"\ntitle = \"F\"\nmembers = [\"manual:redis\"]\n",
            ),
        ]);
        assert!(outcome.diagnostics.is_empty(), "{:?}", outcome.diagnostics);
        assert_eq!(outcome.overlay.entities.len(), 1);
        assert_eq!(outcome.overlay.entities[0].id.as_str(), "manual:redis");
        assert_eq!(
            outcome.overlay.views[0].entity_ids.as_ref().unwrap()[0].as_str(),
            "manual:redis"
        );
    }

    #[test]
    fn entities_rejected_by_validation_never_reach_the_overlay() {
        let (outcome, _dir) = convert(&flows(
            r#"
[[entity]]
id = ""
kind = "manual_block"
label = "Empty id"

[[entity]]
id = "manual:prefixed"
kind = "manual_block"
label = "Prefixed"

[[entity]]
id = "good"
kind = "manual_block"
label = "Good"
"#,
        ));
        // `validate` reported both; the overlay must not carry them, and must
        // not report them a second time.
        assert!(outcome.diagnostics.is_empty(), "{:?}", outcome.diagnostics);
        let ids: Vec<&str> = outcome
            .overlay
            .entities
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(ids, ["manual:good"]);
    }

    #[test]
    fn a_duplicate_entity_yields_exactly_one_entity_and_the_first_wins() {
        let entity = |label: &str| {
            format!("[[entity]]\nid = \"redis\"\nkind = \"infrastructure\"\nlabel = \"{label}\"\n")
        };
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".cratevista/flows/a.toml", &entity("First"));
        write(dir.path(), ".cratevista/flows/b.toml", &entity("Second"));
        let config = load_from(dir.path());
        let validation = validate(&config);
        let outcome = build_overlay(&config, &validation);

        assert_eq!(outcome.overlay.entities.len(), 1);
        assert_eq!(outcome.overlay.entities[0].label.default, "First");
        // The duplicate was already reported by `validate`; not again here.
        assert!(outcome.diagnostics.is_empty());
    }

    #[test]
    fn a_duplicate_flow_yields_exactly_one_view() {
        let flow = "[[flow]]\nid = \"same\"\ntitle = \"S\"\n";
        let (outcome, _dir) = convert(&[
            (".cratevista/flows/a.toml", flow),
            (".cratevista/flows/b.toml", flow),
        ]);
        assert_eq!(outcome.overlay.views.len(), 1);
    }

    #[test]
    fn no_configuration_yields_an_empty_overlay() {
        let (outcome, _dir) = convert(&[]);
        assert!(outcome.overlay.entities.is_empty());
        assert!(outcome.overlay.relations.is_empty());
        assert!(outcome.overlay.views.is_empty());
        assert!(outcome.overlay.overrides.is_empty());
        assert!(outcome.diagnostics.is_empty());
    }

    #[test]
    fn identical_input_yields_an_identical_overlay() {
        let files = &[
            (".cratevista/flows/b.toml", FULL),
            (
                ".cratevista/overrides/o.toml",
                "[[override]]\ntarget = \"item:struct:app::Order\"\nlabel = \"O\"\ntags = [\"t\"]\n",
            ),
        ];
        let render = |outcome: &OverlayOutcome| {
            format!(
                "{:?}|{:?}|{:?}|{:?}",
                outcome.overlay.entities,
                outcome.overlay.relations,
                outcome.overlay.views,
                outcome.overlay.overrides
            )
        };
        let (first, _dir) = convert(files);
        let baseline = render(&first);
        for _ in 0..5 {
            let (again, _dir) = convert(files);
            assert_eq!(render(&again), baseline, "the overlay must be reproducible");
        }
    }

    #[test]
    fn attribute_values_map_across_toml_types() {
        let (outcome, _dir) = convert(&flows(
            r#"
[[entity]]
id = "e"
kind = "manual_block"
label = "E"

  [entity.attributes]
  text = "s"
  int = 7
  float = 1.5
  flag = false
  list = [1, "two"]

    [entity.attributes.nested]
    inner = "v"
"#,
        ));
        let attributes = &outcome.overlay.entities[0].attributes;
        assert_eq!(attributes["text"], AttrValue::String("s".into()));
        assert_eq!(attributes["int"], AttrValue::from(7));
        assert_eq!(attributes["float"], AttrValue::from(1.5));
        assert_eq!(attributes["flag"], AttrValue::Bool(false));
        assert_eq!(
            attributes["list"],
            AttrValue::Array(vec![AttrValue::from(1), AttrValue::String("two".into())])
        );
        assert_eq!(attributes["nested"]["inner"], AttrValue::String("v".into()));
    }

    #[test]
    fn step_three_never_resolves_file_backed_docs_or_examples() {
        // The paths are authored, but nothing is read and nothing is embedded:
        // that is step 4's job, and this test pins the boundary.
        let (outcome, _dir) = convert(&flows(
            r#"
[[flow]]
id = "f"
title = "F"
docs = ["docs/does-not-exist.md"]

  [[flow.example]]
  id = "e"
  title = "E"
  path = "examples/missing.json"
"#,
        ));
        // A missing file is not even noticed, because nothing opens it.
        assert!(outcome.diagnostics.is_empty(), "{:?}", outcome.diagnostics);
        let view = &outcome.overlay.views[0];
        assert_eq!(view.docs, None);
        assert!(view.examples.is_empty());
    }
}
