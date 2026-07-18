//! The **raw** configuration model: TOML as authored, before any interpretation.
//!
//! This layer is deliberately dumb. It mirrors the file format one-to-one and
//! makes no judgements — `validate` decides what is wrong, and `overlay`
//! (step 3) decides what it means. Keeping them apart is what lets a bad file
//! produce precise diagnostics instead of a parse failure that loses everything
//! after it.
//!
//! Identifiers are wrapped in [`Spanned`] so a diagnostic can point at the exact
//! key rather than the file as a whole. Only the fields a diagnostic might name
//! carry spans: spanning everything would bloat the model for no benefit.

use serde::Deserialize;
use serde_spanned::Spanned;
use std::collections::BTreeMap;

/// Localized text: either a bare string or a table of translations.
///
/// ```toml
/// label = "Redis"
/// # or
/// label = { default = "Redis", de = "Redis" }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum RawLocalized {
    /// `label = "Redis"` — becomes the default translation.
    Plain(String),
    /// `label = { default = "…", de = "…" }`.
    Translations(BTreeMap<String, String>),
}

/// An arbitrary TOML value used for presentation attributes.
pub type RawValue = toml::Value;

/// A manual entity declared inside a flow file via `[[entity]]`.
///
/// Its config-local `id` becomes the entity id `manual:{id}` (see
/// [`crate::manual_entity_id`]); ids are unique across the **whole** config set,
/// and any flow file may reference any of them.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawEntity {
    /// Config-local id, e.g. `redis` → entity id `manual:redis`.
    pub id: Spanned<String>,
    /// Open kind, e.g. `external_system` / `infrastructure` / `manual_block`.
    pub kind: Spanned<String>,
    /// Display label.
    pub label: RawLocalized,
    #[serde(default)]
    pub description: Option<RawLocalized>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub attributes: BTreeMap<String, RawValue>,
    /// Optional repo-relative source location (validated later, in step 3).
    #[serde(default)]
    pub source: Option<String>,
}

/// One lane within a flow.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawStage {
    pub id: Spanned<String>,
    pub title: RawLocalized,
    pub order: Spanned<u32>,
}

/// A manual relation between two flow members.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRelation {
    /// Full entity id: `manual:<id>` or a discovered stable id.
    pub from: Spanned<String>,
    /// Full entity id: `manual:<id>` or a discovered stable id.
    pub to: Spanned<String>,
    /// Open relation kind; defaults to `manual`.
    #[serde(default)]
    pub kind: Option<String>,
    /// A semantic role (`http`, `ws`, `sql`, …).
    ///
    /// **Part of the relation's identity**, not decoration: `RelationId` is
    /// `kind + from + to` plus an optional role, so two edges between the same
    /// pair need distinct roles or they collapse into one. See
    /// [`crate::overlay`].
    #[serde(default)]
    pub role: Option<Spanned<String>>,
    /// Edge label (`HTTP`, `SQL`, …). Becomes `Relation::label`, not an attribute.
    #[serde(default)]
    pub label: Option<RawLocalized>,
    #[serde(default)]
    pub attributes: BTreeMap<String, RawValue>,
}

/// A worked example attached to a flow.
///
/// `path` is read and **embedded** into the document in step 4, so the explorer
/// renders it without `/api/source`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawExample {
    pub id: Spanned<String>,
    pub title: RawLocalized,
    /// Repo-relative path to the example's contents.
    pub path: Spanned<String>,
    /// Display-only syntax hint (`json`, `http`, …).
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub description: Option<RawLocalized>,
}

/// A curated architecture/runtime view.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawFlow {
    /// Config-local id, e.g. `checkout` → view id `view:checkout`.
    pub id: Spanned<String>,
    pub title: RawLocalized,
    #[serde(default)]
    pub description: Option<RawLocalized>,
    /// Full entity ids. Discovered ids are validated downstream by PRD 05, not
    /// here; only `manual:` references are resolvable within the config set.
    #[serde(default)]
    pub members: Vec<Spanned<String>>,
    #[serde(default)]
    pub default_focus: Option<Spanned<String>>,
    #[serde(default, rename = "stage")]
    pub stages: Vec<RawStage>,
    #[serde(default, rename = "relation")]
    pub relations: Vec<RawRelation>,
    #[serde(default, rename = "example")]
    pub examples: Vec<RawExample>,
    /// Repo-relative Markdown files whose contents become the flow's docs.
    #[serde(default)]
    pub docs: Vec<Spanned<String>>,
}

/// One `.cratevista/flows/*.toml` file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawFlowFile {
    #[serde(default, rename = "entity")]
    pub entities: Vec<RawEntity>,
    #[serde(default, rename = "flow")]
    pub flows: Vec<RawFlow>,
}

/// A presentation override of one discovered entity.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawOverride {
    /// The discovered entity's stable id. **Not** resolved here: PRD 05 already
    /// diagnoses an override aimed at a missing entity.
    pub target: Spanned<String>,
    #[serde(default)]
    pub label: Option<RawLocalized>,
    #[serde(default)]
    pub description: Option<RawLocalized>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub promoted: Option<bool>,
    /// Repo-relative Markdown appended to the entity's discovered docs.
    #[serde(default)]
    pub docs: Vec<Spanned<String>>,
    #[serde(default)]
    pub presentation: BTreeMap<String, RawValue>,
}

/// One `.cratevista/overrides/*.toml` file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawOverrideFile {
    #[serde(default, rename = "override")]
    pub overrides: Vec<RawOverride>,
}

/// The root `cratevista.toml`.
///
/// The reserved `[metadata]` / `[rustdoc]` / `[server]` sections are **parsed
/// and ignored** for the MVP — binding them is deferred (it would change
/// implemented PRD-03/04/06 behaviour). Capturing them here means an author who
/// writes one gets no spurious "unknown field" error.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRootConfig {
    /// Optional and unused in the MVP.
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub metadata: Option<toml::Table>,
    #[serde(default)]
    pub rustdoc: Option<toml::Table>,
    #[serde(default)]
    pub server: Option<toml::Table>,
}

/// Everything found on disk, in deterministic order.
#[derive(Debug, Clone, Default)]
pub struct RawConfig {
    /// `cratevista.toml`, when present.
    pub root: Option<RawRootConfig>,
    /// Flow files, sorted by workspace-relative path.
    pub flow_files: Vec<LoadedFile<RawFlowFile>>,
    /// Override files, sorted by workspace-relative path.
    pub override_files: Vec<LoadedFile<RawOverrideFile>>,
    /// Diagnostics from discovery/loading (unreadable or malformed files).
    pub diagnostics: Vec<crate::error::ConfigDiagnostic>,
}

impl RawConfig {
    /// True when no configuration exists at all — the normal case, which must
    /// produce an empty overlay rather than an error.
    pub fn is_empty(&self) -> bool {
        self.root.is_none() && self.flow_files.is_empty() && self.override_files.is_empty()
    }
}

/// A parsed file plus the context a diagnostic needs.
#[derive(Debug, Clone)]
pub struct LoadedFile<T> {
    /// Workspace-relative path, `/`-normalized. Never absolute.
    pub path: String,
    /// The file's exact text, retained so byte spans resolve to line/column.
    pub source: String,
    /// The parsed contents.
    pub value: T,
}

impl RawLocalized {
    /// The default translation, used for validation messages.
    pub fn default_text(&self) -> Option<&str> {
        match self {
            RawLocalized::Plain(text) => Some(text),
            RawLocalized::Translations(map) => map.get("default").map(String::as_str),
        }
    }
}
