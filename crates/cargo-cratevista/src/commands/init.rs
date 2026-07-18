//! `cargo cratevista init`.

use std::path::Path;

use cratevista_core::{CommandOutcome, usecase};

/// Creates a minimal `cratevista.toml` at `project_root`.
pub fn run(project_root: &Path, force: bool) -> CommandOutcome {
    usecase::run_init(project_root, force)
}
