//! Cross-source linking: join the metadata (`target:`) and rustdoc (`module:`)
//! identity schemes using the stable identities on `CrateSummary` — never
//! crate-name string comparison.

use std::collections::BTreeMap;

use cratevista_rustdoc::CrateSummary;
use cratevista_schema::{DocumentDiagnostic, Entity, EntityId, Provenance, Relation, RelationKind};

use crate::diagnostics::{code, warn};

/// For each documented crate, verify its `target_id`/`root_module_id`, set the
/// root module's parent to its target, and emit a `contains` relation
/// `target_id → root_module_id`.
pub fn link_crates(
    entities: &mut BTreeMap<EntityId, Entity>,
    relations: &mut Vec<Relation>,
    crates: &[CrateSummary],
    diagnostics: &mut Vec<DocumentDiagnostic>,
) {
    for summary in crates {
        if !entities.contains_key(&summary.target_id) {
            diagnostics.push(warn(
                code::RUSTDOC_TARGET_UNLINKED,
                format!(
                    "documented crate `{}` references missing metadata target `{}`",
                    summary.crate_name, summary.target_id
                ),
                Some(summary.target_id.clone()),
            ));
            continue;
        }
        let Some(root_module) = entities.get_mut(&summary.root_module_id) else {
            diagnostics.push(warn(
                code::RUSTDOC_TARGET_UNLINKED,
                format!(
                    "documented crate `{}` has no root module entity `{}`",
                    summary.crate_name, summary.root_module_id
                ),
                Some(summary.target_id.clone()),
            ));
            continue;
        };

        // Set the root module's parent to its Cargo target when currently absent.
        if root_module.parent.is_none() {
            root_module.parent = Some(summary.target_id.clone());
        }

        relations.push(Relation::new(
            RelationKind::new(RelationKind::CONTAINS),
            summary.target_id.clone(),
            summary.root_module_id.clone(),
            Provenance::Discovered,
        ));
    }
}
