//! Pure helpers for mapping rustdoc items to schema entity kinds and for the
//! deterministic impl signature that feeds the impl-block discriminator.
//!
//! Raw `rustdoc_types::Id`s are internal lookup keys only; public ids come from
//! the `cratevista_schema` constructors. Nothing here derives an id from
//! rustdoc index/HashMap iteration order or numeric id order.

use rustdoc_types::{Crate, Generics, Id, Impl, ItemEnum};

use crate::types::display_type;

/// Joins a crate-relative `prefix` with a child `name` (`::`-separated).
pub fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}::{name}")
    }
}

/// The schema kind string for an associated (trait/impl) item.
pub fn assoc_item_kind(inner: &ItemEnum) -> &'static str {
    match inner {
        ItemEnum::Function(_) => "method",
        ItemEnum::AssocConst { .. } => "assoc_const",
        ItemEnum::AssocType { .. } => "assoc_type",
        ItemEnum::Constant { .. } => "assoc_const",
        _ => "assoc_item",
    }
}

/// The trait path (with generic args) an impl implements, or `"inherent"`.
pub fn impl_trait_or_inherent(imp: &Impl) -> String {
    match &imp.trait_ {
        Some(path) => path.path.clone(),
        None => "inherent".to_string(),
    }
}

/// A readable display of the type an impl block is for.
pub fn impl_for_display(imp: &Impl) -> String {
    display_type(&imp.for_)
}

/// A deterministic normalized signature for the impl-block discriminator.
///
/// Includes the negativity flag, trait (with args), self type, generic
/// parameters, where-predicate arity, and the sorted names of the impl's items,
/// so that multiple/inherent/blanket impls for one type never collide.
pub fn impl_signature(krate: &Crate, imp: &Impl) -> String {
    let mut member_names: Vec<String> = imp
        .items
        .iter()
        .filter_map(|id| krate.index.get(id))
        .filter_map(|item| item.name.clone())
        .collect();
    member_names.sort();

    format!(
        "neg={};synthetic={};trait={};for={};generics={};where={};members=[{}]",
        imp.is_negative,
        imp.is_synthetic,
        impl_trait_or_inherent(imp),
        impl_for_display(imp),
        generics_signature(&imp.generics),
        imp.generics.where_predicates.len(),
        member_names.join(","),
    )
}

/// A stable signature fragment for an impl's generic parameters.
fn generics_signature(generics: &Generics) -> String {
    let params: Vec<String> = generics
        .params
        .iter()
        .map(|param| param.name.clone())
        .collect();
    params.join(",")
}

/// Whether the impl is a synthetic (auto-trait) or blanket impl.
pub fn impl_is_synthetic_or_blanket(imp: &Impl) -> bool {
    imp.is_synthetic || imp.blanket_impl.is_some()
}

/// A stable label for a raw id used only in diagnostics (never a public id).
pub fn raw_id_label(id: Id) -> String {
    format!("#{}", id.0)
}
