//! Routes parsed CLI commands to `cratevista-core` use cases.

use cratevista_core::{CommandFailure, CommandOutcome, paths};

use crate::cli::{Cli, Command};
use crate::commands;

/// Dispatches a parsed [`Cli`] to the matching command adapter.
pub fn dispatch(cli: Cli) -> CommandOutcome {
    let project_root = match paths::resolve_project_root(cli.manifest_path.as_deref()) {
        Ok(root) => root,
        Err(error) => return Err(CommandFailure::runtime(error.to_diagnostic())),
    };

    match cli.command {
        Command::Init { force } => commands::init::run(&project_root, force),
        Command::Doctor => commands::doctor::run(&project_root, cli.manifest_path.as_deref()),
        Command::Generate { generate } => {
            let options = generate.into_options(cli.manifest_path.clone());
            commands::generate::run(&options)
        }
        Command::Serve { server } => {
            let options = cratevista_core::ServeOptions {
                manifest_path: cli.manifest_path.clone(),
                host: server.host(),
                port: server.port,
                port_was_explicit: server.port_was_explicit(),
                source_access: server.source,
            };
            commands::serve::run(&options)
        }
        Command::Open {
            generate,
            server,
            watch,
        } => {
            let options = cratevista_core::OpenOptions {
                host: server.host(),
                port: server.port,
                port_was_explicit: server.port_was_explicit(),
                source_access: server.source,
                generate: generate.into_options(cli.manifest_path.clone()),
                watch,
            };
            commands::open::run(&options)
        }
        Command::Build {
            output,
            base_path,
            generate,
        } => {
            let options = generate.into_options(cli.manifest_path.clone());
            commands::build::run(options, output, base_path)
        }
    }
}
