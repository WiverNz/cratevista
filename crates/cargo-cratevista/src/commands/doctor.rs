//! `cargo cratevista doctor`.

use std::path::Path;

use cratevista_core::{CommandOutcome, usecase};

/// Reports toolchain and project prerequisites.
pub fn run(project_root: &Path, manifest_path: Option<&Path>) -> CommandOutcome {
    usecase::run_doctor(project_root, manifest_path)
}
