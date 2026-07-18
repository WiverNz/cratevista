//! Command-line interface definition and Cargo external-subcommand handling.

use std::ffi::OsString;
use std::net::IpAddr;
use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use cratevista_core::logging::ColorChoice as CoreColorChoice;

/// CrateVista — Interactive Rust Architecture & Documentation Explorer.
#[derive(Parser, Debug)]
#[command(
    name = "cargo-cratevista",
    bin_name = "cargo cratevista",
    version,
    about = "Turn any Rust workspace into an interactive architecture map.",
    long_about = None,
)]
pub struct Cli {
    /// Path to the `Cargo.toml` of the workspace or package to analyze.
    #[arg(long, global = true, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// Increase logging verbosity (repeatable: -v, -vv, -vvv).
    #[arg(short = 'v', long, global = true, action = ArgAction::Count)]
    pub verbose: u8,

    /// Silence all non-error logging.
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,

    /// Control colored output.
    #[arg(long, global = true, value_enum, default_value_t = Color::Auto)]
    pub color: Color,

    /// Output format for diagnostics.
    #[arg(long, global = true, value_enum, default_value_t = Format::Human)]
    pub format: Format,

    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The MVP command set. Non-bootstrap commands are stubs for now.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create a minimal `cratevista.toml` without overwriting existing files.
    Init {
        /// Overwrite an existing `cratevista.toml`.
        #[arg(long)]
        force: bool,
    },
    /// Report toolchain and project prerequisites (read-only).
    Doctor,
    /// Generate a CrateVista explorer document.
    Generate {
        #[command(flatten)]
        generate: GenerateArgs,
    },
    /// Serve an already-generated document and the embedded UI (no regeneration).
    Serve {
        #[command(flatten)]
        server: ServerArgs,
    },
    /// Generate, serve, and open the explorer in a browser.
    Open {
        #[command(flatten)]
        generate: GenerateArgs,
        #[command(flatten)]
        server: ServerArgs,
        /// Watch the workspace and regenerate the document when it changes.
        ///
        /// Declared on this variant rather than on `ServerArgs`: `serve` shares
        /// that group and must keep rejecting `--watch`, because it serves an
        /// existing snapshot and never regenerates.
        #[arg(long)]
        watch: bool,
    },
    /// Build a self-contained, static architecture site (generate, then write a
    /// portable directory you can host anywhere).
    Build {
        /// Output directory for the static site.
        ///
        /// Defaults to `target/cratevista/site`. A relative path is resolved
        /// against the analyzed workspace root (not the current directory), so
        /// `--output dist` writes to `<workspace-root>/dist`; an absolute path is
        /// used unchanged.
        #[arg(long, value_name = "DIR")]
        output: Option<PathBuf>,

        /// Base path for the generated `<base href>` (e.g. `/repo/` for a project
        /// page). Absent means relative hosting with no base element.
        #[arg(long, value_name = "PATH")]
        base_path: Option<String>,

        #[command(flatten)]
        generate: GenerateArgs,
    },
}

/// The `generate` options, shared by `generate` and `open`.
#[derive(Args, Debug, Clone)]
pub struct GenerateArgs {
    /// Continue past a failed rustdoc target, producing a partial document.
    #[arg(long)]
    pub keep_going: bool,
    /// Space/comma-separated list of features to activate.
    #[arg(long, value_name = "FEATURES", value_delimiter = ',')]
    pub features: Vec<String>,
    /// Activate all available features.
    #[arg(long)]
    pub all_features: bool,
    /// Do not activate the `default` feature.
    #[arg(long)]
    pub no_default_features: bool,
    /// Document private items (passes `--document-private-items` to rustdoc).
    #[arg(long)]
    pub document_private_items: bool,
    /// Override the nightly toolchain used for rustdoc JSON.
    #[arg(long, value_name = "TOOLCHAIN")]
    pub toolchain: Option<String>,
    /// External-dependency inclusion mode.
    #[arg(long, value_enum, default_value_t = ExternalDeps::Exclude, value_name = "MODE")]
    pub external_deps: ExternalDeps,
    /// Also document `bin` targets (off by default).
    #[arg(long)]
    pub document_bins: bool,
    /// Ignore project-local configuration (`cratevista.toml`, `.cratevista/`).
    ///
    /// Produces pure discovered output: no manual flows, entities or overrides,
    /// and nothing under `.cratevista/` is read.
    #[arg(long)]
    pub no_config: bool,
}

impl GenerateArgs {
    /// Converts CLI args into core [`cratevista_core::GenerateOptions`].
    pub fn into_options(self, manifest_path: Option<PathBuf>) -> cratevista_core::GenerateOptions {
        cratevista_core::GenerateOptions {
            manifest_path,
            keep_going: self.keep_going,
            features: self.features,
            all_features: self.all_features,
            no_default_features: self.no_default_features,
            document_private_items: self.document_private_items,
            toolchain: self.toolchain,
            external_deps: self.external_deps.into(),
            document_bins: self.document_bins,
            no_config: self.no_config,
        }
    }
}

/// The loopback-server options, shared by `serve` and `open`.
#[derive(Args, Debug, Clone)]
pub struct ServerArgs {
    /// Host/interface to bind. Non-loopback exposes the server on your network.
    #[arg(long, value_name = "HOST")]
    pub host: Option<IpAddr>,
    /// Port to bind. Without this, `7420` is used with increment-on-conflict.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Enable the guarded `/api/source` endpoint (off by default).
    #[arg(long)]
    pub source: bool,
}

impl ServerArgs {
    /// The host, defaulting to loopback.
    pub fn host(&self) -> IpAddr {
        self.host
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
    }

    /// Whether the port was explicitly set.
    pub fn port_was_explicit(&self) -> bool {
        self.port.is_some()
    }
}

/// Colored-output preference.
#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Color {
    /// Colorize when the terminal supports it.
    Auto,
    /// Always colorize.
    Always,
    /// Never colorize.
    Never,
}

impl From<Color> for CoreColorChoice {
    fn from(color: Color) -> Self {
        match color {
            Color::Auto => CoreColorChoice::Auto,
            Color::Always => CoreColorChoice::Always,
            Color::Never => CoreColorChoice::Never,
        }
    }
}

/// Diagnostic output format.
#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Format {
    /// Human-readable text.
    Human,
    /// Machine-readable JSON.
    Json,
}

/// External-dependency inclusion mode for `generate`.
#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum ExternalDeps {
    /// Workspace members + intra-workspace dependencies only.
    Exclude,
    /// Also include direct external dependencies.
    Direct,
    /// Include the entire resolved dependency graph.
    Full,
}

impl From<ExternalDeps> for cratevista_core::ExternalDepsChoice {
    fn from(value: ExternalDeps) -> Self {
        match value {
            ExternalDeps::Exclude => cratevista_core::ExternalDepsChoice::Exclude,
            ExternalDeps::Direct => cratevista_core::ExternalDepsChoice::Direct,
            ExternalDeps::Full => cratevista_core::ExternalDepsChoice::Full,
        }
    }
}

/// Parses the CLI, tolerating the Cargo external-subcommand argv shape.
///
/// When invoked as `cargo cratevista <args>`, Cargo runs this binary as
/// `cargo-cratevista cratevista <args>`. The leading `cratevista` token is
/// stripped so the same parser handles both `cargo cratevista ...` and a
/// direct `cargo-cratevista ...` invocation.
pub fn parse() -> Cli {
    let mut args: Vec<OsString> = std::env::args_os().collect();
    if args.get(1).and_then(|arg| arg.to_str()) == Some("cratevista") {
        args.remove(1);
    }
    Cli::parse_from(args)
}
