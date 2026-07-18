//! Invoking `cargo metadata --format-version 1` and mapping failures to
//! [`MetadataError`].

use cargo_metadata::{Metadata, MetadataCommand};

use crate::error::MetadataError;
use crate::options::{MetadataOptions, NetworkMode};

/// Feature + network flags forwarded to `cargo metadata`, in a stable order.
fn feature_and_network_flags(options: &MetadataOptions) -> Vec<String> {
    let mut flags = Vec::new();
    if options.features.all_features {
        flags.push("--all-features".to_string());
    }
    if options.features.no_default_features {
        flags.push("--no-default-features".to_string());
    }
    if !options.features.features.is_empty() {
        flags.push("--features".to_string());
        flags.push(options.features.features.join(","));
    }
    match options.network {
        NetworkMode::Inherit => {}
        NetworkMode::Offline => flags.push("--offline".to_string()),
        NetworkMode::Frozen => flags.push("--frozen".to_string()),
        NetworkMode::Locked => flags.push("--locked".to_string()),
    }
    flags
}

/// Reconstructs the exact effective Cargo argv for [`crate::MetadataSummary`].
pub fn effective_argv(options: &MetadataOptions) -> Vec<String> {
    let mut argv = vec![
        "cargo".to_string(),
        "metadata".to_string(),
        "--format-version".to_string(),
        "1".to_string(),
    ];
    argv.extend(feature_and_network_flags(options));
    if let Some(manifest_path) = &options.manifest_path {
        argv.push("--manifest-path".to_string());
        argv.push(manifest_path.display().to_string());
    }
    argv
}

/// Runs `cargo metadata` and returns the parsed [`Metadata`].
pub fn run(options: &MetadataOptions) -> Result<Metadata, MetadataError> {
    let mut command = MetadataCommand::new();
    if let Some(manifest_path) = &options.manifest_path {
        command.manifest_path(manifest_path.clone());
    }
    if let Some(cwd) = &options.cwd {
        command.current_dir(cwd.clone());
    }
    command.other_options(feature_and_network_flags(options));

    command.exec().map_err(|error| map_error(error, options))
}

fn map_error(error: cargo_metadata::Error, options: &MetadataOptions) -> MetadataError {
    match error {
        cargo_metadata::Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
            MetadataError::CargoNotFound(io.to_string())
        }
        cargo_metadata::Error::Io(io) => MetadataError::CargoMetadataFailed {
            argv: effective_argv(options),
            stderr: io.to_string(),
        },
        cargo_metadata::Error::CargoMetadata { stderr } => MetadataError::CargoMetadataFailed {
            argv: effective_argv(options),
            stderr,
        },
        cargo_metadata::Error::Json(error) => MetadataError::MalformedMetadata(error.to_string()),
        cargo_metadata::Error::NoJson => {
            MetadataError::MalformedMetadata("no JSON in `cargo metadata` output".to_string())
        }
        cargo_metadata::Error::Utf8(error) => MetadataError::MalformedMetadata(error.to_string()),
        cargo_metadata::Error::ErrUtf8(error) => {
            MetadataError::MalformedMetadata(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_pins_format_version() {
        let argv = effective_argv(&MetadataOptions::default());
        assert_eq!(argv[0], "cargo");
        assert_eq!(argv[1], "metadata");
        let joined = argv.join(" ");
        assert!(joined.contains("--format-version 1"), "{joined}");
    }

    #[test]
    fn argv_includes_selected_flags() {
        let mut options = MetadataOptions::default();
        options.features.no_default_features = true;
        options.features.features = vec!["a".into(), "b".into()];
        options.network = NetworkMode::Offline;
        let joined = effective_argv(&options).join(" ");
        assert!(joined.contains("--no-default-features"));
        assert!(joined.contains("--features a,b"));
        assert!(joined.contains("--offline"));
    }

    #[test]
    fn maps_cargo_errors_to_stable_codes() {
        let options = MetadataOptions::default();

        let not_found =
            cargo_metadata::Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "cargo"));
        assert_eq!(map_error(not_found, &options).code(), "cargo_not_found");

        let failed = cargo_metadata::Error::CargoMetadata {
            stderr: "boom".to_string(),
        };
        assert_eq!(map_error(failed, &options).code(), "cargo_metadata_failed");

        assert_eq!(
            map_error(cargo_metadata::Error::NoJson, &options).code(),
            "malformed_metadata"
        );
    }
}
