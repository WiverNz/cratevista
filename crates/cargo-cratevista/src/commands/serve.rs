//! `cargo cratevista serve` — serve an existing generated snapshot (issue 06).

use cratevista_core::{CommandOutcome, ServeOptions, serve};

/// Runs the serve use case.
pub fn run(options: &ServeOptions) -> CommandOutcome {
    serve::run_serve(options)
}
