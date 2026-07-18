//! Static-build safety foundations (PRD 10, Phase 2A).
//!
//! These are **application-use-case safety primitives owned specifically by static
//! build** — not general shared-domain utilities. They live in `cratevista-core`
//! because core owns application orchestration (ADR-0001); they must **not** move
//! into `cargo-cratevista` or `cratevista-server`.
//!
//! # What the foundations provide (Phase 2A)
//!
//! - [`base_path`] — `--base-path` parsing/normalization.
//! - [`output_identity`] — the existence-stable `output_key`.
//! - [`safety`] — `OutputSafety` and the directional protected-path check.
//! - [`marker`] — the three-state A/B/C ownership marker and crash-safe I/O.
//! - [`lock`] — the per-output OS advisory lock.
//! - [`error`] — the one authoritative `BuildError` enum + diagnostic mapping.
//!
//! # What materialization adds (Phase 2B)
//!
//! - [`nonce`] — fixed-width 32-hex keyed candidate names.
//! - [`html`] — the two controlled `index.html` head edits.
//! - [`fs_seam`] — the narrow filesystem seam for fault injection.
//! - [`materialize`] — [`materialize_static_site`], key-scoped recovery (cases
//!   A–F), P0/A/B/C classification, and transactional publish-with-rollback.
//!
//! # What Phase 2B does NOT provide (Phase 2C)
//!
//! `run_build` orchestration, `BuildOptions`, generation invocation, the CLI build
//! arguments, and everything downstream. The `run_build` stub is unchanged.

pub mod base_path;
pub mod error;
pub mod fs_seam;
pub mod html;
pub mod lock;
pub mod marker;
pub mod materialize;
pub mod nonce;
pub mod output_identity;
pub mod safety;

pub use base_path::BasePath;
pub use error::BuildError;
pub use fs_seam::{RealSiteFs, SiteFs};
pub use lock::OutputLock;
pub use marker::{MARKER_FILENAME, Marker, MarkerFs, MarkerKind, MarkerRole};
pub use materialize::{PublishedSite, SiteOptions, materialize_static_site};
pub use output_identity::{ResolvedOutput, resolve_output, resolve_output_key};
pub use safety::OutputSafety;
