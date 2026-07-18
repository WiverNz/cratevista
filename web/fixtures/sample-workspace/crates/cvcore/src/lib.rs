//! `cvcore` — core model types for the CrateVista PRD-07 sample workspace.
//!
//! Deliberately small but structurally rich: public + private modules,
//! documented + undocumented public items, structs/enums, a trait and its
//! implementation, and inherent methods with type references.

pub mod model;
mod internal;

pub use model::{Color, Render, Widget};
