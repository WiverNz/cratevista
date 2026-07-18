//! Project-local CrateVista configuration (PRD 08).
//!
//! Loads, validates and (from step 3) converts the TOML under `cratevista.toml`
//! and `.cratevista/` into the `cratevista_graph::GraphOverlay` seam: **manual
//! entities**, **flows** (curated views mixing discovered and manual entities)
//! and **presentation overrides** of discovered entities.
//!
//! # Boundaries
//!
//! - This crate is a **pure transform**: files in, raw model + diagnostics out.
//!   It needs no graph, no analysis order, and no discovered entity ids.
//! - **Configuration errors are never fatal.** A malformed file costs its own
//!   contents and produces a located diagnostic; everything else still loads.
//! - **Validation is config-internal only.** Whether a reference names a real
//!   *discovered* entity is PRD 05's question, and it already answers it
//!   (`invalid_view_reference`, `dangling_relation`, `overlay_target_missing`).
//!   Re-checking here would create a second source of truth.
//! - The dependency direction is **config → graph**, never the reverse:
//!   `cratevista-graph` stays pure and knows nothing about TOML.
//! - Diagnostics carry **workspace-relative** paths and, where a span is
//!   available, a line/column — never an absolute path.
//!
//! # Status
//!
//! Steps 0–2 of the PRD's implementation sequence are implemented: the crate,
//! the raw model, deterministic discovery, span-preserving loading, and
//! config-internal validation. Still to come: `overlay` (step 3, which adds the
//! `cratevista-graph` dependency), `docs` loading + embedding (step 4), and the
//! `cratevista-core` wiring plus `--no-config` (step 6).

#![forbid(unsafe_code)]

use std::path::Path;

use cratevista_graph::GraphOverlay;

pub mod discover;
pub mod docs;
pub mod error;
pub mod load;
pub mod model;
pub mod overlay;
pub mod referenced;
pub mod validate;

pub use discover::{Discovered, discover};
pub use docs::{MAX_EXAMPLE_BYTES, WorkspaceFiles, embed_files};
pub use error::{ConfigDiagnostic, Position, code};
pub use load::{load, load_from};
pub use model::{LoadedFile, RawConfig, RawFlowFile, RawOverrideFile, RawRootConfig};
pub use overlay::{OverlayOutcome, build_overlay};
pub use referenced::{ReferencedConfigFile, ReferencedFileKind};
pub use validate::{
    MANUAL_PREFIX, ManualIds, Validation, is_manual_reference, manual_entity_id, validate,
};

/// Everything a caller needs from a configuration: the overlay to feed the
/// graph, and whatever went wrong producing it.
#[derive(Debug, Default)]
pub struct ConfigOutcome {
    /// The overlay for `cratevista_graph::GraphInput`. Empty when there is no
    /// configuration — which is the normal, zero-config case.
    pub overlay: GraphOverlay,
    /// Every problem found, in pipeline order (load → validate → build → embed).
    /// **All are non-fatal**: the valid parts of a configuration still produce an
    /// overlay, and the caller still builds and commits a document.
    pub diagnostics: Vec<ConfigDiagnostic>,
    /// Every file this configuration explicitly references — flow docs, flow
    /// examples and override docs — sorted and deduplicated by `(path, kind)`.
    ///
    /// Read-only and **derived from declarations, not from disk**: a path appears
    /// here even when the file is missing, oversized, non-UTF-8 or a directory,
    /// because those are exactly the files someone is about to fix. Only illegal
    /// *spellings* (absolute, traversing, malformed) are excluded. See
    /// [`referenced`] for the full rule.
    ///
    /// Empty when there is no configuration.
    pub referenced_files: Vec<ReferencedConfigFile>,
}

impl ConfigOutcome {
    /// True when nothing was configured and nothing went wrong.
    ///
    /// `referenced_files` is not consulted: a reference cannot exist without a
    /// flow or override that declared it, so a non-empty list always implies a
    /// non-empty overlay or a diagnostic.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
            && self.overlay.entities.is_empty()
            && self.overlay.relations.is_empty()
            && self.overlay.views.is_empty()
            && self.overlay.overrides.is_empty()
    }
}

/// Loads the project-local configuration under `workspace_root`.
///
/// The one entry point callers need: it runs the whole pipeline —
/// `discover → load → validate → build_overlay → embed_files` — and returns an
/// overlay plus diagnostics.
///
/// **Absence of configuration is normal**, not an error: with no
/// `cratevista.toml` and no `.cratevista/`, this returns an empty overlay and no
/// diagnostics, which is byte-for-byte equivalent to passing
/// `GraphOverlay::default()`.
///
/// It never fails as a whole and never panics: an unreadable or malformed file
/// costs its own contents and becomes a diagnostic.
pub fn load_config(workspace_root: &Path) -> ConfigOutcome {
    load_config_with(workspace_root, &discover::discover(workspace_root))
}

/// [`load_config`] over an **already-discovered** file set.
///
/// The result is identical to `load_config(workspace_root)` when `discovered ==
/// discover(workspace_root)`; it exists so a caller that already discovered the
/// configuration (e.g. to record the config files as generation inputs) runs
/// discovery **once** instead of scanning `.cratevista/` twice.
pub fn load_config_with(workspace_root: &Path, discovered: &Discovered) -> ConfigOutcome {
    let raw = load::load(workspace_root, discovered);
    // Cheap exit for the overwhelmingly common case: nothing configured, so
    // nothing to validate, convert or read.
    if raw.is_empty() && raw.diagnostics.is_empty() {
        return ConfigOutcome::default();
    }

    let validation = validate::validate(&raw);
    let mut outcome = overlay::build_overlay(&raw, &validation);
    let embed = docs::embed_files(workspace_root, &raw, &validation, &mut outcome.overlay);
    // Pure path validation over the same declarations `embed_files` just read —
    // no filesystem access, and no effect on the overlay or the diagnostics.
    let referenced_files = referenced::collect(&raw);

    // Pipeline order, so a reader follows the same path the data took.
    let mut diagnostics = raw.diagnostics;
    diagnostics.extend(validation.diagnostics);
    diagnostics.extend(outcome.diagnostics);
    diagnostics.extend(embed);

    ConfigOutcome {
        overlay: outcome.overlay,
        diagnostics,
        referenced_files,
    }
}
