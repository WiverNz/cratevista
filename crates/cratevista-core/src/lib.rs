//! CrateVista application core.
//!
//! `cratevista-core` is the orchestration / use-case layer for CrateVista. It
//! coordinates generation workflows and defines application-level use cases
//! (`init`, `doctor`, and — in later issues — `generate`, `serve`, `build`),
//! connecting the analyzer, graph, schema, and server crates.
//!
//! It deliberately does **not** own shared domain models (those live in
//! `cratevista-schema`) and is **not** a generic utilities crate. In this
//! bootstrap stage it provides the application runtime scaffolding used by the
//! `cargo-cratevista` binary: a terminal [`diagnostic::Diagnostic`], the
//! [`exit::ExitCode`] policy, [`logging`] initialization, process/OS
//! [`paths`] resolution, the top-level [`error::CoreError`], and the
//! [`usecase`] entry points.
//!
//! See `PRD/issue_01_workspace_and_cli.md` and `docs/adr/0001-crate-boundaries.md`.
#![forbid(unsafe_code)]

pub mod artifacts;
pub mod build;
pub mod clock;
pub mod config_diagnostics;
pub mod diagnostic;
pub mod error;
pub mod exit;
pub mod generate;
pub mod logging;
pub mod open;
pub mod paths;
pub mod serve;
pub mod static_site;
pub mod usecase;
pub mod watch;
mod watch_recovery;
mod watch_runtime;

pub use build::{BuildOptions, run_build};
pub use clock::{Clock, SystemClock};
pub use diagnostic::{Diagnostic, Severity};
pub use error::CoreError;
pub use exit::ExitCode;
pub use generate::{ExternalDepsChoice, GenerateOptions};
pub use open::{OpenOptions, run_open};
pub use serve::{ServeOptions, run_serve};
pub use usecase::{CommandFailure, CommandOutcome};
