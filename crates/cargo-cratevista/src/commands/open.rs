//! `cargo cratevista open` — generate, serve, and open a browser (issue 06).

use std::sync::Arc;

use cratevista_core::{CommandOutcome, OpenOptions, SystemClock, open};

/// Runs the open use case with the real system clock.
///
/// The clock is shared rather than borrowed because `--watch` regenerates on a
/// blocking pool for as long as the session lives, and each regeneration stamps
/// its own `generated_at`.
pub fn run(options: &OpenOptions) -> CommandOutcome {
    open::run_open(options, Arc::new(SystemClock))
}
