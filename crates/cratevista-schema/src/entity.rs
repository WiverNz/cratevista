//! Entities and their shared building blocks (localized text, attributes,
//! provenance).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::docs::DocBlock;
use crate::ids::EntityId;
use crate::kind::EntityKind;
use crate::source::SourceLocation;

/// A freeform, extensible attribute value (any JSON value).
///
/// Attributes carry presentation/analysis extras without schema churn.
pub type AttrValue = serde_json::Value;

/// The default language key for [`LocalizedText`].
pub const DEFAULT_LANG: &str = "en";

/// Localization-ready text: a default string plus optional per-language
/// translations keyed by language code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LocalizedText {
    /// The default (source-language) text.
    pub default: String,
    /// Optional translations keyed by language code (e.g. `ru`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub translations: BTreeMap<String, String>,
}

impl LocalizedText {
    /// Builds a localized text with only a default value.
    pub fn new(default: impl Into<String>) -> Self {
        LocalizedText {
            default: default.into(),
            translations: BTreeMap::new(),
        }
    }

    /// Resolves the text for a language code, falling back to the default.
    pub fn resolve(&self, lang: &str) -> &str {
        self.translations
            .get(lang)
            .map(String::as_str)
            .unwrap_or(&self.default)
    }
}

/// Whether an entity/relation was discovered automatically or declared manually.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Derived automatically from Cargo/rustdoc data.
    Discovered,
    /// Declared in CrateVista configuration.
    Manual,
}

/// A node in the explorer graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Entity {
    /// Stable identifier.
    pub id: EntityId,
    /// Open, string-backed kind.
    pub kind: EntityKind,
    /// Display label (localization-ready).
    pub label: LocalizedText,
    /// Fully-qualified name (e.g. `crate::module::Type`).
    pub qualified_name: String,
    /// Discovered vs manual provenance.
    pub provenance: Provenance,
    /// Optional containing entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<EntityId>,
    /// Optional repository-relative source location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLocation>,
    /// Optional documentation block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<DocBlock>,
    /// Ordered, deduplicated tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Freeform attributes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, AttrValue>,
    /// Optional longer description (localization-ready).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<LocalizedText>,
}

impl Entity {
    /// Builds a minimal entity; other fields can be set on the returned value.
    pub fn new(
        id: EntityId,
        kind: EntityKind,
        label: LocalizedText,
        qualified_name: impl Into<String>,
        provenance: Provenance,
    ) -> Self {
        Entity {
            id,
            kind,
            label,
            qualified_name: qualified_name.into(),
            provenance,
            parent: None,
            source: None,
            docs: None,
            tags: Vec::new(),
            attributes: BTreeMap::new(),
            description: None,
        }
    }
}
