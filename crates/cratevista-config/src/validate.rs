//! **Config-internal** validation only.
//!
//! What this checks: structural sanity the parser cannot express (empty ids,
//! empty kinds), duplicate ids across the whole config set, stage sanity, and
//! `manual:` references that name no declared entity.
//!
//! What this deliberately does **not** check — because PRD 05 already does, and
//! duplicating it would mean two sources of truth drifting apart:
//!
//! - **discovered entity ids** are unknown here (the graph has not been built),
//!   so a member/endpoint/override naming a discovered id is passed through
//!   untouched. `sanitize_views` (`invalid_view_reference`),
//!   `drop_dangling_relations` (`dangling_relation`) and `apply_overlay`
//!   (`overlay_target_missing`) diagnose those against the real entity set;
//! - **override targets** are never resolved here for the same reason.
//!
//! This keeps the crate a pure, order-independent transform: files in,
//! diagnostics out, no graph input required.
//!
//! **Reference resolution order matters.** The complete manual-id set is built
//! across *all* flow files before any reference is examined, so a flow may point
//! at an entity declared in another file and the result never depends on which
//! file happened to load first.

use std::collections::{BTreeMap, BTreeSet};

use serde_spanned::Spanned;

use crate::error::{ConfigDiagnostic, Position, code};
use crate::model::{LoadedFile, RawConfig, RawFlowFile};

/// The `manual:` id prefix. A config-local `id = "redis"` becomes `manual:redis`.
pub const MANUAL_PREFIX: &str = "manual:";

/// The entity id a config-local manual id maps to.
pub fn manual_entity_id(config_local_id: &str) -> String {
    format!("{MANUAL_PREFIX}{config_local_id}")
}

/// True when `reference` points at a manual entity rather than a discovered one.
pub fn is_manual_reference(reference: &str) -> bool {
    reference.starts_with(MANUAL_PREFIX)
}

/// Where a declaration came from, for a duplicate's "first declared at" note.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Origin {
    file: String,
    position: Option<Position>,
}

impl Origin {
    fn describe(&self) -> String {
        match self.position {
            Some(Position { line, column }) => format!("{}:{line}:{column}", self.file),
            None => self.file.clone(),
        }
    }
}

fn origin_of<T>(file: &LoadedFile<RawFlowFile>, spanned: &Spanned<T>) -> Origin {
    Origin {
        file: file.path.clone(),
        position: crate::error::position_of(&file.source, spanned.span().start),
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

/// The set of manual entity ids declared anywhere in the configuration.
///
/// Built **before** references are resolved, so cross-file references work and
/// resolution never depends on file order.
#[derive(Debug, Clone, Default)]
pub struct ManualIds {
    /// Full entity ids (`manual:<id>`) → where they were declared.
    ids: BTreeMap<String, Origin>,
}

impl ManualIds {
    /// True when `entity_id` (a full `manual:` id) was declared.
    pub fn contains(&self, entity_id: &str) -> bool {
        self.ids.contains_key(entity_id)
    }

    /// The declared ids, sorted.
    pub fn entity_ids(&self) -> BTreeSet<&str> {
        self.ids.keys().map(String::as_str).collect()
    }

    /// How many entities were declared.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// True when nothing was declared.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// Result of validating a configuration.
#[derive(Debug, Clone, Default)]
pub struct Validation {
    /// The global manual-entity id set (also useful to step 3).
    pub manual_ids: ManualIds,
    /// Everything wrong with the configuration. All non-fatal.
    pub diagnostics: Vec<ConfigDiagnostic>,
}

/// Pass 1: collect every `[[entity]]` id across every flow file, diagnosing
/// duplicates and structurally invalid ids/kinds.
fn collect_manual_ids(config: &RawConfig, diagnostics: &mut Vec<ConfigDiagnostic>) -> ManualIds {
    let mut manual = ManualIds::default();
    for file in &config.flow_files {
        for entity in &file.value.entities {
            let id = entity.id.get_ref().trim();
            if id.is_empty() {
                diagnostics.push(located(
                    file,
                    &entity.id,
                    code::INVALID_ID,
                    "an `[[entity]]` id must not be empty".into(),
                ));
                continue;
            }
            if id.starts_with(MANUAL_PREFIX) {
                diagnostics.push(located(
                    file,
                    &entity.id,
                    code::INVALID_ID,
                    format!(
                        "declare the id as `{}`, not `{id}`: the `{MANUAL_PREFIX}` prefix is added \
                         automatically, and references use the full id",
                        id.trim_start_matches(MANUAL_PREFIX)
                    ),
                ));
                continue;
            }
            if entity.kind.get_ref().trim().is_empty() {
                diagnostics.push(located(
                    file,
                    &entity.kind,
                    code::INVALID_ID,
                    format!("entity `{id}` must declare a non-empty `kind`"),
                ));
                // Not `continue`: the id is still usable for reference checking,
                // so reporting the kind must not also cause phantom
                // "unknown reference" errors elsewhere.
            }

            let entity_id = manual_entity_id(id);
            let origin = origin_of(file, &entity.id);
            if let Some(first) = manual.ids.get(&entity_id) {
                // Ids are global across the config set, so this fires for a
                // duplicate in the same file AND across files.
                diagnostics.push(located(
                    file,
                    &entity.id,
                    code::DUPLICATE_ENTITY_ID,
                    format!(
                        "duplicate entity id `{id}`; already declared at {}. \
                         Manual entity ids are unique across the whole configuration.",
                        first.describe()
                    ),
                ));
                continue;
            }
            manual.ids.insert(entity_id, origin);
        }
    }
    manual
}

/// Validates one flow's stages: non-empty ids, unique ids, unique order.
fn validate_stages(
    file: &LoadedFile<RawFlowFile>,
    flow_id: &str,
    flow: &crate::model::RawFlow,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    let mut seen_ids: BTreeMap<&str, Origin> = BTreeMap::new();
    let mut seen_orders: BTreeMap<u32, Origin> = BTreeMap::new();
    for stage in &flow.stages {
        let stage_id = stage.id.get_ref().trim();
        if stage_id.is_empty() {
            diagnostics.push(located(
                file,
                &stage.id,
                code::INVALID_STAGE,
                format!("a stage of flow `{flow_id}` has an empty id"),
            ));
            continue;
        }
        if let Some(first) = seen_ids.get(stage_id) {
            diagnostics.push(located(
                file,
                &stage.id,
                code::INVALID_STAGE,
                format!(
                    "flow `{flow_id}` declares stage `{stage_id}` twice; first at {}",
                    first.describe()
                ),
            ));
            continue;
        }
        // A duplicate order would make lane placement depend on declaration
        // order rather than on `order`, which is the whole point of the field.
        if let Some(first) = seen_orders.get(stage.order.get_ref()) {
            diagnostics.push(located(
                file,
                &stage.order,
                code::INVALID_STAGE,
                format!(
                    "flow `{flow_id}` reuses stage order {} (stage `{stage_id}`); first at {}. \
                     Stage order must be unique so lanes are deterministic.",
                    stage.order.get_ref(),
                    first.describe()
                ),
            ));
            continue;
        }
        seen_ids.insert(stage_id, origin_of(file, &stage.id));
        seen_orders.insert(*stage.order.get_ref(), origin_of(file, &stage.order));
    }
}

/// Checks one `manual:` reference against the global set. Discovered ids pass
/// through: they are PRD 05's to judge.
fn check_reference(
    file: &LoadedFile<RawFlowFile>,
    reference: &Spanned<String>,
    manual: &ManualIds,
    context: &str,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    let value = reference.get_ref().trim();
    if value.is_empty() {
        diagnostics.push(located(
            file,
            reference,
            code::INVALID_ID,
            format!("{context} is empty"),
        ));
        return;
    }
    if !is_manual_reference(value) {
        // A discovered stable id. Not resolvable here — PRD 05 validates it
        // against the real entity set and diagnoses it if it is stale.
        return;
    }
    if !manual.contains(value) {
        diagnostics.push(located(
            file,
            reference,
            code::UNKNOWN_MANUAL_REFERENCE,
            format!(
                "{context} names `{value}`, but no `[[entity]]` declares it. \
                 Declare it in any flow file, or use a discovered entity id."
            ),
        ));
    }
}

/// Validates a loaded configuration.
///
/// Two passes, and the order is load-bearing: **every** manual id is collected
/// first, so a reference to an entity declared in a different file resolves
/// regardless of which file loaded first.
pub fn validate(config: &RawConfig) -> Validation {
    let mut diagnostics = Vec::new();

    // Pass 1 — the complete global id set.
    let manual_ids = collect_manual_ids(config, &mut diagnostics);

    // Pass 2 — flows: ids, stages, and references resolved against that set.
    let mut seen_flows: BTreeMap<String, Origin> = BTreeMap::new();
    for file in &config.flow_files {
        for flow in &file.value.flows {
            let flow_id = flow.id.get_ref().trim();
            if flow_id.is_empty() {
                diagnostics.push(located(
                    file,
                    &flow.id,
                    code::INVALID_ID,
                    "a `[[flow]]` id must not be empty".into(),
                ));
                continue;
            }
            if let Some(first) = seen_flows.get(flow_id) {
                diagnostics.push(located(
                    file,
                    &flow.id,
                    code::DUPLICATE_FLOW_ID,
                    format!(
                        "duplicate flow id `{flow_id}`; already declared at {}",
                        first.describe()
                    ),
                ));
                continue;
            }
            seen_flows.insert(flow_id.to_string(), origin_of(file, &flow.id));

            validate_stages(file, flow_id, flow, &mut diagnostics);

            for member in &flow.members {
                check_reference(
                    file,
                    member,
                    &manual_ids,
                    &format!("flow `{flow_id}` member"),
                    &mut diagnostics,
                );
            }
            if let Some(focus) = &flow.default_focus {
                check_reference(
                    file,
                    focus,
                    &manual_ids,
                    &format!("flow `{flow_id}` default_focus"),
                    &mut diagnostics,
                );
            }
            for relation in &flow.relations {
                check_reference(
                    file,
                    &relation.from,
                    &manual_ids,
                    &format!("flow `{flow_id}` relation `from`"),
                    &mut diagnostics,
                );
                check_reference(
                    file,
                    &relation.to,
                    &manual_ids,
                    &format!("flow `{flow_id}` relation `to`"),
                    &mut diagnostics,
                );
            }
            // Example ids must be unique within their view (the schema says so).
            let mut seen_examples: BTreeMap<&str, Origin> = BTreeMap::new();
            for example in &flow.examples {
                let example_id = example.id.get_ref().trim();
                if example_id.is_empty() {
                    diagnostics.push(located(
                        file,
                        &example.id,
                        code::INVALID_ID,
                        format!("an example of flow `{flow_id}` has an empty id"),
                    ));
                    continue;
                }
                if let Some(first) = seen_examples.get(example_id) {
                    diagnostics.push(located(
                        file,
                        &example.id,
                        code::INVALID_ID,
                        format!(
                            "flow `{flow_id}` declares example `{example_id}` twice; first at {}",
                            first.describe()
                        ),
                    ));
                    continue;
                }
                seen_examples.insert(example_id, origin_of(file, &example.id));
            }
        }
    }

    Validation {
        manual_ids,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::load_from;
    use std::path::Path;

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// Loads + validates a config set given `(file name, contents)` pairs.
    fn check(files: &[(&str, &str)]) -> (Validation, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        for (name, contents) in files {
            write(dir.path(), &format!(".cratevista/flows/{name}"), contents);
        }
        let config = load_from(dir.path());
        assert!(
            config.diagnostics.is_empty(),
            "load: {:?}",
            config.diagnostics
        );
        (validate(&config), dir)
    }

    fn codes(validation: &Validation) -> Vec<&str> {
        validation.diagnostics.iter().map(|d| d.code).collect()
    }

    const ENTITY: &str = r#"
[[entity]]
id = "redis"
kind = "infrastructure"
label = "Redis"
"#;

    #[test]
    fn a_valid_configuration_produces_no_diagnostics() {
        let (validation, _dir) = check(&[(
            "a.toml",
            &format!(
                r#"{ENTITY}
[[flow]]
id = "checkout"
title = "Checkout"
members = ["manual:redis"]
default_focus = "manual:redis"

  [[flow.stage]]
  id = "s1"
  title = "One"
  order = 1

  [[flow.stage]]
  id = "s2"
  title = "Two"
  order = 2
"#
            ),
        )]);
        assert!(
            validation.diagnostics.is_empty(),
            "{:?}",
            validation.diagnostics
        );
        assert_eq!(validation.manual_ids.len(), 1);
        assert!(validation.manual_ids.contains("manual:redis"));
    }

    #[test]
    fn no_configuration_validates_cleanly() {
        let (validation, _dir) = check(&[]);
        assert!(validation.diagnostics.is_empty());
        assert!(validation.manual_ids.is_empty());
    }

    #[test]
    fn duplicate_entity_ids_are_global_across_files_and_name_both_locations() {
        let (validation, _dir) = check(&[("a.toml", ENTITY), ("b.toml", ENTITY)]);
        assert_eq!(codes(&validation), [code::DUPLICATE_ENTITY_ID]);
        let diagnostic = &validation.diagnostics[0];
        // Reported against the second file…
        assert_eq!(diagnostic.file, ".cratevista/flows/b.toml");
        // …and it names where the first one was.
        assert!(
            diagnostic.message.contains(".cratevista/flows/a.toml"),
            "must name the first declaration: {}",
            diagnostic.message
        );
        assert!(diagnostic.position.is_some());
        // The first declaration still wins, so the id remains usable.
        assert!(validation.manual_ids.contains("manual:redis"));
        assert_eq!(validation.manual_ids.len(), 1);
    }

    #[test]
    fn duplicate_entity_ids_within_one_file_are_caught_too() {
        let (validation, _dir) = check(&[("a.toml", &format!("{ENTITY}{ENTITY}"))]);
        assert_eq!(codes(&validation), [code::DUPLICATE_ENTITY_ID]);
    }

    #[test]
    fn a_manual_entity_is_referenceable_from_another_flow_file() {
        // The decisive cross-file case: `b.toml` references an entity declared in
        // `a.toml`, and must resolve regardless of load order.
        let (validation, _dir) = check(&[
            ("a.toml", ENTITY),
            (
                "b.toml",
                r#"
[[flow]]
id = "other"
title = "Other"
members = ["manual:redis"]
"#,
            ),
        ]);
        assert!(
            validation.diagnostics.is_empty(),
            "{:?}",
            validation.diagnostics
        );
    }

    #[test]
    fn a_reference_resolves_even_when_declared_in_a_later_file() {
        // `a.toml` loads FIRST but references an entity declared in `z.toml`.
        // This only works because the id set is built before resolution.
        let (validation, _dir) = check(&[
            (
                "a.toml",
                r#"
[[flow]]
id = "early"
title = "Early"
members = ["manual:redis"]
"#,
            ),
            ("z.toml", ENTITY),
        ]);
        assert!(
            validation.diagnostics.is_empty(),
            "forward references must resolve: {:?}",
            validation.diagnostics
        );
    }

    #[test]
    fn an_unknown_manual_reference_is_diagnosed_with_a_location() {
        let (validation, _dir) = check(&[(
            "a.toml",
            r#"
[[flow]]
id = "f"
title = "F"
members = ["manual:nope"]
"#,
        )]);
        assert_eq!(codes(&validation), [code::UNKNOWN_MANUAL_REFERENCE]);
        let diagnostic = &validation.diagnostics[0];
        assert!(diagnostic.message.contains("manual:nope"));
        assert!(diagnostic.position.is_some());
    }

    #[test]
    fn unknown_manual_references_are_caught_in_focus_and_relations_too() {
        let (validation, _dir) = check(&[(
            "a.toml",
            &format!(
                r#"{ENTITY}
[[flow]]
id = "f"
title = "F"
members = ["manual:redis"]
default_focus = "manual:ghost"

  [[flow.relation]]
  from = "manual:redis"
  to = "manual:phantom"
"#
            ),
        )]);
        assert_eq!(
            codes(&validation),
            [
                code::UNKNOWN_MANUAL_REFERENCE,
                code::UNKNOWN_MANUAL_REFERENCE
            ]
        );
        assert!(validation.diagnostics[0].message.contains("default_focus"));
        assert!(validation.diagnostics[1].message.contains("relation `to`"));
    }

    /// The boundary with PRD 05: discovered ids are none of this crate's business.
    #[test]
    fn discovered_entity_ids_are_never_validated_here() {
        let (validation, _dir) = check(&[(
            "a.toml",
            r#"
[[flow]]
id = "f"
title = "F"
members = [
  "item:struct:cvcore::model::Widget",
  "package:cvcore",
  "item:struct:totally::made::Up",
]
default_focus = "item:struct:also::Fake"

  [[flow.relation]]
  from = "item:struct:a::A"
  to = "item:struct:b::B"
"#,
        )]);
        // Not one diagnostic: the graph owns these (invalid_view_reference /
        // dangling_relation). Duplicating the check here would mean two sources
        // of truth for the same rule.
        assert!(
            validation.diagnostics.is_empty(),
            "discovered ids must pass through untouched: {:?}",
            validation.diagnostics
        );
    }

    #[test]
    fn override_targets_are_not_resolved_here() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".cratevista/overrides/o.toml",
            "[[override]]\ntarget = \"item:struct:not::Real\"\nlabel = \"X\"\n",
        );
        let config = load_from(dir.path());
        let validation = validate(&config);
        // PRD 05's `apply_overlay` emits `overlay_target_missing` for this.
        assert!(validation.diagnostics.is_empty());
    }

    #[test]
    fn duplicate_flow_ids_are_diagnosed_across_files() {
        let flow = "[[flow]]\nid = \"same\"\ntitle = \"S\"\n";
        let (validation, _dir) = check(&[("a.toml", flow), ("b.toml", flow)]);
        assert_eq!(codes(&validation), [code::DUPLICATE_FLOW_ID]);
        assert!(validation.diagnostics[0].message.contains("a.toml"));
    }

    #[test]
    fn duplicate_stage_ids_and_orders_are_diagnosed() {
        let (validation, _dir) = check(&[(
            "a.toml",
            r#"
[[flow]]
id = "f"
title = "F"

  [[flow.stage]]
  id = "s"
  title = "S"
  order = 1

  [[flow.stage]]
  id = "s"
  title = "Dup id"
  order = 2

  [[flow.stage]]
  id = "t"
  title = "Dup order"
  order = 1
"#,
        )]);
        assert_eq!(
            codes(&validation),
            [code::INVALID_STAGE, code::INVALID_STAGE]
        );
        assert!(validation.diagnostics[0].message.contains("twice"));
        assert!(
            validation.diagnostics[1]
                .message
                .contains("reuses stage order")
        );
    }

    #[test]
    fn empty_ids_and_kinds_are_diagnosed() {
        let (validation, _dir) = check(&[(
            "a.toml",
            r#"
[[entity]]
id = ""
kind = "infrastructure"
label = "X"

[[entity]]
id = "ok"
kind = "  "
label = "Y"

[[flow]]
id = ""
title = "F"
"#,
        )]);
        assert_eq!(
            codes(&validation),
            [code::INVALID_ID, code::INVALID_ID, code::INVALID_ID]
        );
    }

    #[test]
    fn an_entity_with_an_invalid_kind_is_still_referenceable() {
        // Reporting the kind must not ALSO produce a phantom
        // "unknown reference" for every flow that names the entity.
        let (validation, _dir) = check(&[(
            "a.toml",
            r#"
[[entity]]
id = "redis"
kind = ""
label = "Redis"

[[flow]]
id = "f"
title = "F"
members = ["manual:redis"]
"#,
        )]);
        assert_eq!(
            codes(&validation),
            [code::INVALID_ID],
            "only the kind error"
        );
        assert!(validation.manual_ids.contains("manual:redis"));
    }

    #[test]
    fn declaring_an_id_with_the_manual_prefix_is_rejected_with_guidance() {
        let (validation, _dir) = check(&[(
            "a.toml",
            "[[entity]]\nid = \"manual:redis\"\nkind = \"infra\"\nlabel = \"R\"\n",
        )]);
        assert_eq!(codes(&validation), [code::INVALID_ID]);
        // The message must tell the author exactly what to write instead.
        assert!(validation.diagnostics[0].message.contains("`redis`"));
        assert!(validation.manual_ids.is_empty());
    }

    #[test]
    fn duplicate_example_ids_within_a_flow_are_diagnosed() {
        let (validation, _dir) = check(&[(
            "a.toml",
            r#"
[[flow]]
id = "f"
title = "F"

  [[flow.example]]
  id = "e"
  title = "One"
  path = "a.txt"

  [[flow.example]]
  id = "e"
  title = "Two"
  path = "b.txt"
"#,
        )]);
        assert_eq!(codes(&validation), [code::INVALID_ID]);
        assert!(validation.diagnostics[0].message.contains("twice"));
    }

    #[test]
    fn validation_is_deterministic() {
        let files: &[(&str, &str)] = &[
            ("b.toml", ENTITY),
            ("a.toml", ENTITY),
            (
                "c.toml",
                "[[flow]]\nid = \"f\"\ntitle = \"F\"\nmembers = [\"manual:ghost\"]\n",
            ),
        ];
        let (first, _dir) = check(files);
        let rendered: Vec<String> = first.diagnostics.iter().map(ToString::to_string).collect();
        for _ in 0..5 {
            let (again, _dir) = check(files);
            let rendered_again: Vec<String> =
                again.diagnostics.iter().map(ToString::to_string).collect();
            assert_eq!(rendered_again, rendered);
        }
    }

    #[test]
    fn the_manual_id_helpers_agree_with_the_prefix_rule() {
        assert_eq!(manual_entity_id("redis"), "manual:redis");
        assert!(is_manual_reference("manual:redis"));
        assert!(!is_manual_reference("item:struct:a::B"));
        assert!(!is_manual_reference("package:x"));
    }
}
