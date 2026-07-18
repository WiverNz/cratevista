//! Normalizing rustdoc `Type`s into stable display text plus the head nominal
//! reference used for **intra-crate** typed relations.
//!
//! This module never invents cross-crate edges: it exposes the head
//! `rustdoc_types::Id` of a nominal type (peeled through references, pointers,
//! slices, and arrays) so the caller can resolve it *within the same crate's
//! index*; anything else is preserved as an unresolved reference by the caller.

use rustdoc_types::{GenericArg, GenericArgs, Id, Path, Type};

/// A normalized reference to a type.
pub struct TypeRef<'a> {
    /// Stable, human-readable display text (deterministic).
    pub display: String,
    /// The head nominal type's rustdoc id, if the type resolves to a path.
    pub head: Option<&'a Path>,
}

/// Normalizes a type into display text and its head nominal path (if any).
pub fn type_ref(ty: &Type) -> TypeRef<'_> {
    TypeRef {
        display: display_type(ty),
        head: head_path(ty),
    }
}

/// If `ty` is (a reference to) a `Result<T, E>`, returns `(ok, err)`.
pub fn result_ok_err(ty: &Type) -> Option<(&Type, &Type)> {
    let path = head_path(ty)?;
    let last = path.path.rsplit("::").next().unwrap_or(&path.path);
    if last != "Result" {
        return None;
    }
    let GenericArgs::AngleBracketed { args, .. } = path.args.as_deref()? else {
        return None;
    };
    let types: Vec<&Type> = args
        .iter()
        .filter_map(|arg| match arg {
            GenericArg::Type(t) => Some(t),
            _ => None,
        })
        .collect();
    match types.as_slice() {
        [ok, err] => Some((ok, err)),
        _ => None,
    }
}

/// Peels references/pointers/slices/arrays and returns the head nominal path.
fn head_path(ty: &Type) -> Option<&Path> {
    match ty {
        Type::ResolvedPath(path) => Some(path),
        Type::BorrowedRef { type_, .. }
        | Type::RawPointer { type_, .. }
        | Type::Slice(type_)
        | Type::Array { type_, .. } => head_path(type_),
        _ => None,
    }
}

/// A stable, readable rendering of a type. Deterministic for equal input.
pub fn display_type(ty: &Type) -> String {
    match ty {
        Type::ResolvedPath(path) => display_path(path),
        Type::DynTrait(dyn_trait) => {
            let traits: Vec<String> = dyn_trait
                .traits
                .iter()
                .map(|poly| display_path(&poly.trait_))
                .collect();
            format!("dyn {}", traits.join(" + "))
        }
        Type::Generic(name) => name.clone(),
        Type::Primitive(name) => name.clone(),
        Type::FunctionPointer(_) => "fn(..)".to_string(),
        Type::Tuple(items) => {
            let parts: Vec<String> = items.iter().map(display_type).collect();
            format!("({})", parts.join(", "))
        }
        Type::Slice(inner) => format!("[{}]", display_type(inner)),
        Type::Array { type_, len } => format!("[{}; {}]", display_type(type_), len),
        Type::Pat { type_, .. } => display_type(type_),
        Type::ImplTrait(_) => "impl Trait".to_string(),
        Type::Infer => "_".to_string(),
        Type::RawPointer { is_mutable, type_ } => {
            let kind = if *is_mutable { "*mut" } else { "*const" };
            format!("{kind} {}", display_type(type_))
        }
        Type::BorrowedRef {
            is_mutable, type_, ..
        } => {
            let kind = if *is_mutable { "&mut " } else { "&" };
            format!("{kind}{}", display_type(type_))
        }
        Type::QualifiedPath {
            name, self_type, ..
        } => format!("{}::{name}", display_type(self_type)),
    }
}

fn display_path(path: &Path) -> String {
    let base = path.path.clone();
    match path.args.as_deref() {
        Some(GenericArgs::AngleBracketed { args, .. }) if !args.is_empty() => {
            let parts: Vec<String> = args
                .iter()
                .map(|arg| match arg {
                    GenericArg::Lifetime(lifetime) => lifetime.clone(),
                    GenericArg::Type(ty) => display_type(ty),
                    GenericArg::Const(constant) => constant.expr.clone(),
                    GenericArg::Infer => "_".to_string(),
                })
                .collect();
            format!("{base}<{}>", parts.join(", "))
        }
        _ => base,
    }
}

/// The head nominal id of a type, peeling references, for intra-crate resolution.
pub fn head_id(ty: &Type) -> Option<Id> {
    head_path(ty).map(|path| path.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str, id: u32, args: Option<GenericArgs>) -> Path {
        Path {
            path: name.to_string(),
            id: Id(id),
            args: args.map(Box::new),
        }
    }

    #[test]
    fn peels_reference_to_head() {
        let ty = Type::BorrowedRef {
            lifetime: None,
            is_mutable: false,
            type_: Box::new(Type::ResolvedPath(path("Greeter", 7, None))),
        };
        assert_eq!(head_id(&ty), Some(Id(7)));
        assert_eq!(display_type(&ty), "&Greeter");
    }

    #[test]
    fn primitive_has_no_head() {
        let ty = Type::Primitive("u32".into());
        assert_eq!(head_id(&ty), None);
        assert_eq!(display_type(&ty), "u32");
    }

    #[test]
    fn result_decomposes() {
        let ty = Type::ResolvedPath(path(
            "std::result::Result",
            1,
            Some(GenericArgs::AngleBracketed {
                args: vec![
                    GenericArg::Type(Type::ResolvedPath(path("Greeter", 2, None))),
                    GenericArg::Type(Type::ResolvedPath(path("MyError", 3, None))),
                ],
                constraints: vec![],
            }),
        ));
        let (ok, err) = result_ok_err(&ty).unwrap();
        assert_eq!(head_id(ok), Some(Id(2)));
        assert_eq!(head_id(err), Some(Id(3)));
    }
}
