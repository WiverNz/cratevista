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

/// The **bare** trait path an impl implements (no generic args), or `"inherent"`.
///
/// This is the human-readable prefix of the impl entity id; the generic arguments
/// are carried by [`impl_trait_display`] (which feeds the discriminator and the
/// label), so the id prefix stays free of `<>` while distinct `Trait<A>`/`Trait<B>`
/// impls still receive distinct discriminators.
pub fn impl_trait_or_inherent(imp: &Impl) -> String {
    match &imp.trait_ {
        Some(path) => path.path.clone(),
        None => "inherent".to_string(),
    }
}

/// The trait an impl implements **including generic arguments** (e.g.
/// `From<std::io::Error>`), or `"inherent"`. This is what distinguishes
/// `impl From<A> for T` from `impl From<B> for T`: it feeds both the impl-block
/// discriminator (so the two never collide) and the human-readable label.
pub fn impl_trait_display(imp: &Impl) -> String {
    match &imp.trait_ {
        Some(path) => crate::types::display_path(path),
        None => "inherent".to_string(),
    }
}

/// A readable display of the type an impl block is for.
pub fn impl_for_display(imp: &Impl) -> String {
    display_type(&imp.for_)
}

/// A deterministic normalized signature for the impl-block discriminator.
///
/// Includes the negativity flag, trait **with generic args**, self type, generic
/// parameters, where-predicate arity, and the sorted names of the impl's items, so
/// that multiple/inherent/blanket impls for one type never collide. The trait's
/// generic arguments are essential: without them `impl From<A> for T` and
/// `impl From<B> for T` produce the same signature and one impl is silently dropped.
pub fn impl_signature(krate: &Crate, imp: &Impl) -> String {
    let mut member_names: Vec<String> = imp
        .items
        .iter()
        .filter_map(|id| krate.index.get(id))
        .filter_map(|item| item.name.clone())
        .collect();
    member_names.sort();

    // The trait and self type use **canonical** identities (full paths from the
    // crate's `paths` map), so two distinct types that share a short display name —
    // `serde_json::Error` and `sqlx_core::Error`, both shown as `Error` — never
    // collapse. `where=` carries the rendered where-clause, so two hand-written
    // blanket impls that differ only in their bounds stay distinct.
    let trait_signature = match &imp.trait_ {
        Some(path) => crate::types::path_identity(krate, path),
        None => "inherent".to_string(),
    };
    format!(
        "neg={};synthetic={};trait={};for={};generics={};where={};members=[{}]",
        imp.is_negative,
        imp.is_synthetic,
        trait_signature,
        crate::types::type_identity(krate, &imp.for_),
        generics_signature(&imp.generics),
        crate::types::where_identity(krate, &imp.generics),
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
