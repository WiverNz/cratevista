//! `cargo cratevista build` — generate, then materialize a static site (issue 10).

use std::path::PathBuf;

use cratevista_core::static_site::BasePath;
use cratevista_core::{BuildOptions, CommandOutcome, GenerateOptions, SystemClock, run_build};

/// The default output, relative to the analyzed workspace root (core anchors it).
const DEFAULT_OUTPUT: &str = "target/cratevista/site";

/// Runs the build use case with the real system clock.
///
/// The adapter only maps CLI values onto [`BuildOptions`]: it parses `--base-path`
/// through the core [`BasePath`] type (preserving `build_invalid_base_path` / exit
/// 2) and picks the default relative output. It never runs cargo, resolves the
/// workspace, enumerates assets, prepares parents, locks, recovers or
/// materializes — `run_build` owns all of that.
pub fn run(
    generate: GenerateOptions,
    output: Option<PathBuf>,
    base_path: Option<String>,
) -> CommandOutcome {
    // Parse the base path through the core contract so the diagnostic code and exit
    // class match everywhere; no validation is duplicated here.
    let base_path = match base_path {
        Some(raw) => Some(BasePath::parse(&raw).map_err(|error| error.to_command_failure())?),
        None => None,
    };

    let options = BuildOptions {
        generate,
        // A relative default; `run_build` anchors it to the generated workspace root.
        output: output.unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT)),
        base_path,
    };
    run_build(&options, &SystemClock)
}
