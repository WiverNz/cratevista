//! Pre-validation sanitizing (drop known-dangling endpoints so a schema
//! validation failure represents a real invariant bug) and the final schema
//! validation call. No second structural validator is created — the schema's
//! `ExplorerDocument::validate()` is authoritative.

use std::collections::BTreeSet;

use cratevista_schema::{DocumentDiagnostic, EntityId, ExplorerDocument, Relation, View};

use crate::diagnostics::{code, warn, warn_relation};
use crate::error::GraphError;

/// Drops relations whose endpoints are not present, emitting `dangling_relation`.
pub fn drop_dangling_relations(
    relations: Vec<Relation>,
    entity_ids: &BTreeSet<EntityId>,
) -> (Vec<Relation>, Vec<DocumentDiagnostic>) {
    let mut kept = Vec::with_capacity(relations.len());
    let mut diagnostics = Vec::new();
    for relation in relations {
        let from_ok = entity_ids.contains(&relation.from);
        let to_ok = entity_ids.contains(&relation.to);
        if from_ok && to_ok {
            kept.push(relation);
        } else {
            let missing = if !from_ok {
                &relation.from
            } else {
                &relation.to
            };
            diagnostics.push(warn_relation(
                code::DANGLING_RELATION,
                format!(
                    "relation `{}` references missing entity `{missing}`; dropped",
                    relation.id
                ),
                relation.id.clone(),
            ));
        }
    }
    (kept, diagnostics)
}

/// Removes dangling explicit view membership / focus (from manual overlay views),
/// emitting `invalid_view_reference`. Filter-based auto views are untouched.
pub fn sanitize_views(
    views: Vec<View>,
    entity_ids: &BTreeSet<EntityId>,
) -> (Vec<View>, Vec<DocumentDiagnostic>) {
    let mut diagnostics = Vec::new();
    let sanitized = views
        .into_iter()
        .map(|mut view| {
            if let Some(members) = view.entity_ids.take() {
                let mut kept = Vec::new();
                for member in members {
                    if entity_ids.contains(&member) {
                        kept.push(member);
                    } else {
                        diagnostics.push(warn(
                            code::INVALID_VIEW_REFERENCE,
                            format!("view `{}` references missing entity `{member}`", view.id),
                            Some(member),
                        ));
                    }
                }
                view.entity_ids = Some(kept);
            }
            if let Some(focus) = &view.default_focus
                && !entity_ids.contains(focus)
            {
                diagnostics.push(warn(
                    code::INVALID_VIEW_REFERENCE,
                    format!("view `{}` default_focus references missing entity", view.id),
                    Some(focus.clone()),
                ));
                view.default_focus = None;
            }
            view
        })
        .collect();
    (sanitized, diagnostics)
}

/// Validates the assembled document; on failure returns a fatal error so the
/// invalid document is never returned or written.
pub fn validate_document(document: &ExplorerDocument) -> Result<(), GraphError> {
    document
        .validate()
        .map_err(GraphError::DocumentValidationFailed)
}
