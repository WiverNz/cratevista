//! `cargo-cratevista` — the `cargo cratevista` external subcommand.
//!
//! This binary is a thin adapter: it parses arguments, initializes logging, and
//! dispatches to `cratevista-core` use cases, then renders any failure as a
//! diagnostic and returns the policy exit code. See
//! `PRD/issue_01_workspace_and_cli.md`.
#![forbid(unsafe_code)]

mod cli;
mod commands;
mod dispatch;

use std::process::ExitCode;

use cli::Format;

fn main() -> ExitCode {
    let parsed = cli::parse();
    let (verbose, quiet, color, format) =
        (parsed.verbose, parsed.quiet, parsed.color, parsed.format);

    cratevista_core::logging::init(verbose, quiet, color.into());

    match dispatch::dispatch(parsed) {
        Ok(code) => ExitCode::from(code.code() as u8),
        Err(failure) => {
            match format {
                Format::Human => eprintln!("{}", failure.diagnostic),
                Format::Json => println!("{}", failure.diagnostic.to_json()),
            }
            ExitCode::from(failure.exit.code() as u8)
        }
    }
}
