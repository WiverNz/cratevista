//! Filesystem-change classification and debouncing for CrateVista watch mode.
//!
//! This crate answers two questions and nothing else:
//!
//! 1. **Does this changed path matter?** — [`WatchSet::classify`], purely
//!    lexical, no filesystem access.
//! 2. **Have the changes settled enough to act?** — [`Debouncer`], a state
//!    machine driven by caller-supplied timestamps, with no clock and no sleep.
//!
//! # What lives elsewhere
//!
//! - The **real `notify` watcher**, the **single-flight engine**, and the
//!   **event types** arrive in later PRD-09 steps.
//! - **`cratevista-core`** owns orchestration: it builds the [`WatchSet`], runs
//!   generation, and publishes results. This crate never calls it.
//!
//! # Why it depends on nothing
//!
//! Its data model is [`PathBuf`](std::path::PathBuf), [`Duration`](std::time::Duration)
//! and plain Rust values — no schema types, no async runtime, no watcher library.
//! That is not minimalism for its own sake: it is what lets every rule here be
//! tested exactly, with no temporary directories, no timing tolerance and no
//! platform gates. A dependency arrives when code needs it.
//!
//! # The containment boundary
//!
//! [`WatchSet::classify`] rejects paths whose **text** escapes the workspace root.
//! **That is not a symlink-containment check** and must not be mistaken for one:
//! `<root>/link/x.rs` is lexically inside the root wherever `link` points.
//!
//! Core's WatchSet builder is responsible for the other half: it must
//! **canonicalize every registration target that exists and refuse any whose
//! resolved path falls outside the canonical workspace root** before handing it
//! here. See [`classify`] for the full split.

#![forbid(unsafe_code)]

pub mod classify;
pub mod debounce;
pub mod engine;
pub mod event;
pub mod pattern;
pub mod plan;
pub mod watcher;

pub use classify::{
    Classification, IgnoreReason, InputKind, WatchInput, WatchSet, is_lexically_absolute,
};
pub use debounce::{DEFAULT_MAX_DELAY, DEFAULT_QUIET, DebounceOptions, Debouncer};
pub use engine::{
    Engine, EngineClosed, EngineHandle, Regenerate, RegenerationFailure, RegenerationRequest,
    RegenerationResult, RegenerationSuccess, spawn,
};
pub use event::EngineEvent;
pub use plan::{PlanError, RegistrationMode, WatchPlan, WatchRegistration};
pub use watcher::{
    WatchEvent, Watcher, WatcherClosed, WatcherError, spawn_watcher, spawn_watcher_with,
};
