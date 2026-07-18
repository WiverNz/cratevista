//! `cargo cratevista generate` — build the explorer document (issue 05).

use cratevista_core::{CommandOutcome, GenerateOptions, SystemClock, generate};

/// Runs the generate use case with the real system clock.
pub fn run(options: &GenerateOptions) -> CommandOutcome {
    generate::run_generate(options, &SystemClock)
}
