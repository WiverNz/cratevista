//! Open, string-backed entity and relation kinds.
//!
//! Known kinds are exposed as constants, but any string is valid: unknown kinds
//! deserialize, validate, serialize, and round-trip without loss, and the
//! frontend renders them with a generic fallback (issue 07). There is no strict
//! mode that rejects unknown kinds.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The classification of an [`crate::entity::Entity`]. Open and string-backed.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct EntityKind(String);

/// The classification of a [`crate::relation::Relation`]. Open and string-backed.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct RelationKind(String);

/// The MVP entity kinds emitted by first-party producers.
pub const KNOWN_ENTITY_KINDS: &[&str] = &[
    "workspace",
    "package",
    "target",
    "module",
    "struct",
    "enum",
    "union",
    "trait",
    "function",
    "method",
    "impl",
    "type_alias",
    "constant",
    "static",
    "macro",
    "external_system",
    "infrastructure",
    "stage",
    "manual_block",
];

/// The MVP relation kinds emitted by first-party producers.
pub const KNOWN_RELATION_KINDS: &[&str] = &[
    "contains",
    "depends_on",
    "imports",
    "re_exports",
    "implements",
    "implemented_for",
    "has_field_type",
    "accepts_type",
    "returns_type",
    "error_type",
    "references_type",
    "manual",
];

impl EntityKind {
    /// Builds an entity kind from any string (used for unknown kinds too).
    pub fn new(value: impl Into<String>) -> Self {
        EntityKind(value.into())
    }

    /// The kind as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this kind is one of the MVP [`KNOWN_ENTITY_KINDS`].
    pub fn is_known(&self) -> bool {
        KNOWN_ENTITY_KINDS.contains(&self.0.as_str())
    }

    // Known-kind constants.
    /// `workspace`
    pub const WORKSPACE: &'static str = "workspace";
    /// `package`
    pub const PACKAGE: &'static str = "package";
    /// `target`
    pub const TARGET: &'static str = "target";
    /// `module`
    pub const MODULE: &'static str = "module";
    /// `struct`
    pub const STRUCT: &'static str = "struct";
    /// `enum`
    pub const ENUM: &'static str = "enum";
    /// `union`
    pub const UNION: &'static str = "union";
    /// `trait`
    pub const TRAIT: &'static str = "trait";
    /// `function`
    pub const FUNCTION: &'static str = "function";
    /// `method`
    pub const METHOD: &'static str = "method";
    /// `impl`
    pub const IMPL: &'static str = "impl";
    /// `type_alias`
    pub const TYPE_ALIAS: &'static str = "type_alias";
    /// `constant`
    pub const CONSTANT: &'static str = "constant";
    /// `static`
    pub const STATIC: &'static str = "static";
    /// `macro`
    pub const MACRO: &'static str = "macro";
    /// `external_system`
    pub const EXTERNAL_SYSTEM: &'static str = "external_system";
    /// `infrastructure`
    pub const INFRASTRUCTURE: &'static str = "infrastructure";
    /// `stage`
    pub const STAGE: &'static str = "stage";
    /// `manual_block`
    pub const MANUAL_BLOCK: &'static str = "manual_block";
}

impl RelationKind {
    /// Builds a relation kind from any string (used for unknown kinds too).
    pub fn new(value: impl Into<String>) -> Self {
        RelationKind(value.into())
    }

    /// The kind as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this kind is one of the MVP [`KNOWN_RELATION_KINDS`].
    pub fn is_known(&self) -> bool {
        KNOWN_RELATION_KINDS.contains(&self.0.as_str())
    }

    /// `contains`
    pub const CONTAINS: &'static str = "contains";
    /// `depends_on`
    pub const DEPENDS_ON: &'static str = "depends_on";
    /// `imports`
    pub const IMPORTS: &'static str = "imports";
    /// `re_exports`
    pub const RE_EXPORTS: &'static str = "re_exports";
    /// `implements`
    pub const IMPLEMENTS: &'static str = "implements";
    /// `implemented_for`
    pub const IMPLEMENTED_FOR: &'static str = "implemented_for";
    /// `has_field_type`
    pub const HAS_FIELD_TYPE: &'static str = "has_field_type";
    /// `accepts_type`
    pub const ACCEPTS_TYPE: &'static str = "accepts_type";
    /// `returns_type`
    pub const RETURNS_TYPE: &'static str = "returns_type";
    /// `error_type`
    pub const ERROR_TYPE: &'static str = "error_type";
    /// `references_type`
    pub const REFERENCES_TYPE: &'static str = "references_type";
    /// `manual`
    pub const MANUAL: &'static str = "manual";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_kinds_are_recognized() {
        assert!(EntityKind::new(EntityKind::MODULE).is_known());
        assert!(RelationKind::new(RelationKind::CONTAINS).is_known());
    }

    #[test]
    fn unknown_kinds_are_preserved_and_unknown() {
        let k = EntityKind::new("some_future_kind");
        assert!(!k.is_known());
        assert_eq!(k.as_str(), "some_future_kind");
        // Round-trips as a bare string.
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(json, "\"some_future_kind\"");
        let back: EntityKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }
}
