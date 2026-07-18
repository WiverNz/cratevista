//! CrateVista Cargo metadata ingestion.
//!
//! Discovers workspace structure via `cargo metadata --format-version 1` and
//! maps it into deterministically-ordered `cratevista-schema` entities,
//! relations, and diagnostics. This crate does **not** assemble an
//! `ExplorerDocument`, write any artifact, or depend on `cratevista-core`.
//!
//! Entry points:
//!
//! - [`ingest`] — invoke Cargo metadata and normalize it.
//! - [`normalize`] — the pure conversion boundary over a pre-fetched
//!   [`cargo_metadata::Metadata`] (hermetic; used by tests and caching).
//!
//! See `PRD/issue_03_cargo_metadata.md`.
#![forbid(unsafe_code)]

mod diagnostics;
pub mod error;
pub mod ids;
pub mod invoke;
mod normalize;
pub mod options;
pub mod result;
pub mod source;

pub use error::MetadataError;
pub use ids::SourceKind;
pub use normalize::normalize;
pub use options::{
    ExternalDepsMode, FeatureSelection, MetadataOptions, NetworkMode, PackageSelection, TargetKinds,
};
pub use result::{MetadataIngest, MetadataSummary};

/// Invokes `cargo metadata` per `options` and returns the normalized result.
///
/// Validates the options, runs Cargo, then normalizes. Fatal problems (Cargo
/// missing, metadata failure, malformed output, missing selected package,
/// invalid options, internal invariant) return [`MetadataError`]; recoverable
/// problems are `cratevista_schema::DocumentDiagnostic`s in the result.
pub fn ingest(options: &MetadataOptions) -> Result<MetadataIngest, MetadataError> {
    options.validate()?;
    let metadata = invoke::run(options)?;
    normalize::normalize(&metadata, options)
}
