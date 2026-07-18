//! Application use cases dispatched by the `cargo-cratevista` binary.
//!
//! `init` and `doctor` are fully implemented here; `generate` lives in
//! [`crate::generate`] and `serve`/`open` in [`crate::serve`]/[`crate::open`].
//! Only `build` remains a bootstrap stub that reports "not implemented yet" and
//! exits with [`ExitCode::NOT_IMPLEMENTED`] (issue 10).

use std::path::Path;
use std::process::Command;

use crate::diagnostic::Diagnostic;
use crate::exit::ExitCode;
use crate::paths;

/// A failed command: a diagnostic to render plus the process exit code to use.
#[derive(Debug)]
pub struct CommandFailure {
    /// The diagnostic to present to the user.
    pub diagnostic: Diagnostic,
    /// The exit code to return.
    pub exit: ExitCode,
}

impl CommandFailure {
    /// Builds a failure from a diagnostic and exit code.
    pub fn new(diagnostic: Diagnostic, exit: ExitCode) -> Self {
        CommandFailure { diagnostic, exit }
    }

    /// A runtime error (exit code 1).
    pub fn runtime(diagnostic: Diagnostic) -> Self {
        CommandFailure::new(diagnostic, ExitCode::RUNTIME_ERROR)
    }

    /// A "not implemented yet" stub failure (exit code 4).
    pub fn unimplemented(command: &str) -> Self {
        let diagnostic = Diagnostic::error(
            "unimplemented",
            format!("`cargo cratevista {command}` is not implemented yet"),
        )
        .with_remediation("This command is planned for a future CrateVista release.");
        CommandFailure::new(diagnostic, ExitCode::NOT_IMPLEMENTED)
    }
}

/// The result of running a command: an [`ExitCode`] on success, or a
/// [`CommandFailure`] to render.
pub type CommandOutcome = Result<ExitCode, CommandFailure>;

/// The minimal, commented configuration written by `cargo cratevista init`.
const CRATEVISTA_TOML_TEMPLATE: &str = "\
# CrateVista configuration.
#
# This file is optional. CrateVista works with zero configuration; use it to
# tune tool behavior and to declare manual architecture flows and overrides.
# Full reference: docs/configuration.md
#
# Tool settings (bound in later releases):
#   [metadata]
#   include_external_deps = false
#
#   [rustdoc]
#   document_private_items = false
#
#   [server]
#   port = 7420
#
# Manual flows and presentation overrides live under a sibling directory:
#   .cratevista/flows/*.toml
#   .cratevista/overrides/*.toml
";

/// `cargo cratevista init`: create a minimal `cratevista.toml`.
///
/// Idempotent: if the file already exists it is left untouched unless `force`
/// is set. Never overwrites existing configuration without `force`.
pub fn run_init(project_root: &Path, force: bool) -> CommandOutcome {
    let config_path = project_root.join("cratevista.toml");

    if config_path.exists() && !force {
        println!(
            "cratevista.toml already exists at {} (unchanged). Use --force to overwrite.",
            config_path.display()
        );
        return Ok(ExitCode::SUCCESS);
    }

    match std::fs::write(&config_path, CRATEVISTA_TOML_TEMPLATE) {
        Ok(()) => {
            let verb = if force { "Wrote" } else { "Created" };
            println!("{verb} {}", config_path.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(source) => {
            let diagnostic = Diagnostic::error(
                "init_write_failed",
                format!("could not write {}: {source}", config_path.display()),
            )
            .with_remediation("Check that the directory exists and is writable.");
            Err(CommandFailure::runtime(diagnostic))
        }
    }
}

#[derive(Clone, Copy)]
enum CheckStatus {
    Ok,
    Warn,
    Fatal,
}

impl CheckStatus {
    fn tag(self) -> &'static str {
        match self {
            CheckStatus::Ok => "ok",
            CheckStatus::Warn => "warn",
            CheckStatus::Fatal => "FATAL",
        }
    }
}

struct Check {
    label: &'static str,
    status: CheckStatus,
    detail: String,
    help: Option<&'static str>,
}

/// `cargo cratevista doctor`: report toolchain and project prerequisites.
///
/// Read-only: never installs anything or modifies the machine. Returns
/// [`ExitCode::SUCCESS`] when only warnings/info are present, or
/// [`ExitCode::ENVIRONMENT_ERROR`] when any fatal check fails.
pub fn run_doctor(project_root: &Path, manifest_path: Option<&Path>) -> CommandOutcome {
    let mut checks: Vec<Check> = Vec::new();

    // Cargo availability (fatal if missing).
    match command_line("cargo", &["--version"]) {
        Some(version) => checks.push(Check {
            label: "Cargo available",
            status: CheckStatus::Ok,
            detail: version,
            help: None,
        }),
        None => checks.push(Check {
            label: "Cargo available",
            status: CheckStatus::Fatal,
            detail: "the `cargo` command was not found".to_string(),
            help: Some("Install Rust and Cargo from https://rustup.rs/."),
        }),
    }

    // Rust toolchain (informational).
    if let Some(version) = command_line("rustc", &["--version"]) {
        checks.push(Check {
            label: "Rust toolchain",
            status: CheckStatus::Ok,
            detail: version,
            help: None,
        });
    }

    // Cargo project detection (fatal if missing).
    let manifest = match manifest_path {
        Some(path) if path.is_file() => Some(path.to_path_buf()),
        Some(_) => None,
        None => paths::find_cargo_manifest(project_root),
    };
    match manifest {
        Some(path) => checks.push(Check {
            label: "Cargo project detected",
            status: CheckStatus::Ok,
            detail: path.display().to_string(),
            help: None,
        }),
        None => checks.push(Check {
            label: "Cargo project detected",
            status: CheckStatus::Fatal,
            detail: "no Cargo.toml found in this directory or any parent".to_string(),
            help: Some("Run CrateVista from inside a Cargo workspace, or pass --manifest-path."),
        }),
    }

    // Nightly toolchain for rustdoc JSON (warning if unavailable/unverifiable).
    match nightly_available() {
        Some(true) => checks.push(Check {
            label: "Nightly toolchain for rustdoc JSON",
            status: CheckStatus::Ok,
            detail: "a nightly toolchain is installed".to_string(),
            help: None,
        }),
        Some(false) => checks.push(Check {
            label: "Nightly toolchain for rustdoc JSON",
            status: CheckStatus::Warn,
            detail: "no nightly toolchain found".to_string(),
            help: Some("rustdoc JSON generation needs nightly: rustup toolchain install nightly"),
        }),
        None => checks.push(Check {
            label: "Nightly toolchain for rustdoc JSON",
            status: CheckStatus::Warn,
            detail: "could not verify (rustup not found)".to_string(),
            help: Some("Install rustup, or ensure a nightly toolchain is available for later use."),
        }),
    }

    // Output directory (informational; not created here).
    checks.push(Check {
        label: "Generated output directory",
        status: CheckStatus::Ok,
        detail: format!(
            "{} (created on first `generate`)",
            project_root.join("target").join("cratevista").display()
        ),
        help: None,
    });

    // Render the report to stdout.
    println!("CrateVista doctor\n");
    let mut fatal_count = 0usize;
    let mut warn_count = 0usize;
    for check in &checks {
        if matches!(check.status, CheckStatus::Fatal) {
            fatal_count += 1;
        }
        if matches!(check.status, CheckStatus::Warn) {
            warn_count += 1;
        }
        println!(
            "  [{}] {}: {}",
            check.status.tag(),
            check.label,
            check.detail
        );
        if let Some(help) = check.help {
            println!("        help: {help}");
        }
    }

    println!();
    if fatal_count > 0 {
        println!(
            "Result: {fatal_count} fatal problem(s), {warn_count} warning(s). CrateVista prerequisites are not satisfied."
        );
        Ok(ExitCode::ENVIRONMENT_ERROR)
    } else if warn_count > 0 {
        println!("Result: ok, with {warn_count} warning(s).");
        Ok(ExitCode::SUCCESS)
    } else {
        println!("Result: ok.");
        Ok(ExitCode::SUCCESS)
    }
}

/// Runs `program args...` and returns trimmed stdout on success, else `None`.
fn command_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Returns `Some(true)` if a nightly toolchain is installed, `Some(false)` if
/// none is, or `None` if this cannot be determined (e.g. rustup is absent).
fn nightly_available() -> Option<bool> {
    let output = Command::new("rustup")
        .args(["toolchain", "list"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().any(|line| line.contains("nightly")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_then_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        assert!(run_init(root, false).is_ok());
        let config = root.join("cratevista.toml");
        assert!(config.is_file());
        let first = std::fs::read_to_string(&config).unwrap();

        // Second run without --force must not modify the file.
        assert!(run_init(root, false).is_ok());
        let second = std::fs::read_to_string(&config).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn init_does_not_overwrite_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("cratevista.toml");
        std::fs::write(&config, "# user content\n").unwrap();

        assert!(run_init(dir.path(), false).is_ok());
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            "# user content\n"
        );

        // --force overwrites.
        assert!(run_init(dir.path(), true).is_ok());
        assert_ne!(
            std::fs::read_to_string(&config).unwrap(),
            "# user content\n"
        );
    }

    #[test]
    fn doctor_is_fatal_without_a_cargo_project() {
        // A temp dir outside any Cargo workspace: project detection must fail.
        let dir = tempfile::tempdir().unwrap();
        let outcome = run_doctor(dir.path(), None).expect("doctor returns an exit code");
        assert_eq!(outcome, ExitCode::ENVIRONMENT_ERROR);
    }

    #[test]
    fn doctor_succeeds_with_a_cargo_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        let outcome = run_doctor(dir.path(), None).expect("doctor returns an exit code");
        // Cargo is available in the test environment and a project is present, so
        // the only possible finding is a nightly warning, which is not fatal.
        assert_eq!(outcome, ExitCode::SUCCESS);
    }
}
