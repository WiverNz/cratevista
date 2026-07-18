//! Build-correctness amendment (PRD 06; bundle relocated in PRD 10 Phase 5A).
//!
//! `assets.rs` embeds `embedded/` at compile time via `rust-embed`, but that
//! directory is not otherwise a Cargo build input, so Cargo had no reason to
//! rebuild this crate when the frontend bundle changed. A rebuild after
//! `npm run build` could silently keep serving the previously embedded UI.
//!
//! Declaring the dependency makes Cargo's incremental rebuild correct. The folder
//! is now **crate-local** (`crates/cratevista-server/embedded/`), so the watch path
//! is relative to this crate. This script only prints that declaration: it runs no
//! npm, mutates no files, generates no Rust, adds no build dependency, and changes
//! no runtime behavior.

fn main() {
    println!("cargo::rerun-if-changed=embedded");
}
