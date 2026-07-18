//! Structured logging initialization.
//!
//! Logs are written to stderr. Verbosity controls the level; `--quiet` reduces
//! it to errors only. Machine-readable output (`--format json`) is emitted to
//! stdout by the command layer and is independent of these logs.

use std::io::IsTerminal;

use tracing::Level;

/// Whether to colorize terminal output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    /// Colorize when stderr is a terminal.
    Auto,
    /// Always colorize.
    Always,
    /// Never colorize.
    Never,
}

/// Initializes the global tracing subscriber.
///
/// Idempotent: repeated calls are ignored (useful in tests). Level mapping:
/// `quiet` → ERROR, `0` → WARN, `1` → INFO, `2` → DEBUG, `>= 3` → TRACE.
pub fn init(verbosity: u8, quiet: bool, color: ColorChoice) {
    let level = if quiet {
        Level::ERROR
    } else {
        match verbosity {
            0 => Level::WARN,
            1 => Level::INFO,
            2 => Level::DEBUG,
            _ => Level::TRACE,
        }
    };

    let ansi = match color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => std::io::stderr().is_terminal(),
    };

    // `try_init` returns an error if a subscriber is already set; that is fine.
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_ansi(ansi)
        .with_writer(std::io::stderr)
        .try_init();
}
