//! Constructing and running the `cargo rustdoc … --output-format json`
//! invocation and discovering its JSON output.
//!
//! The command form is the Cargo-level form verified and recorded in ADR-0004.
//! Structured `std::process::Command` args only — no shell strings — and no
//! silent syntax fallback. Argv (which may contain absolute paths) is used only
//! in error messages and `tracing`, never in the public summary.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::RustdocError;
use crate::options::RustdocTargetKind;
use crate::options::{FeatureSelection, NetworkMode, RustdocOptions, RustdocPlan, RustdocTarget};

/// The crate name for output discovery — the target's **actual** Cargo crate
/// name (set by the planner from target metadata), which is the rustdoc JSON
/// output filename (`<crate_name>.json`).
pub fn crate_name(target: &RustdocTarget) -> String {
    target.crate_name.clone()
}

/// The effective target directory for a run (isolated to enable caching).
pub fn target_dir(plan: &RustdocPlan, options: &RustdocOptions) -> PathBuf {
    options.target_dir.clone().unwrap_or_else(|| {
        plan.workspace_root
            .join("target")
            .join("cratevista")
            .join("rustdoc")
    })
}

/// Feature + network flags forwarded on the **cargo side** (before `--`).
fn feature_and_network_flags(features: &FeatureSelection, network: NetworkMode) -> Vec<String> {
    let mut flags = Vec::new();
    if features.all_features {
        flags.push("--all-features".to_string());
    }
    if features.no_default_features {
        flags.push("--no-default-features".to_string());
    }
    if !features.features.is_empty() {
        flags.push("--features".to_string());
        flags.push(features.features.join(","));
    }
    match network {
        NetworkMode::Inherit => {}
        NetworkMode::Offline => flags.push("--offline".to_string()),
        NetworkMode::Frozen => flags.push("--frozen".to_string()),
        NetworkMode::Locked => flags.push("--locked".to_string()),
    }
    flags
}

/// The target-selection flag(s) for a documentable target, or `None` for a kind
/// that cannot be documented.
fn target_flags(target: &RustdocTarget) -> Option<Vec<String>> {
    match &target.target_kind {
        RustdocTargetKind::Library | RustdocTargetKind::ProcMacro => {
            Some(vec!["--lib".to_string()])
        }
        RustdocTargetKind::Binary => Some(vec!["--bin".to_string(), target.target_name.clone()]),
        RustdocTargetKind::Other(_) => None,
    }
}

/// Builds the exact `cargo rustdoc` argv (ADR-0004 verified form). Deterministic.
pub fn build_argv(
    toolchain: &str,
    target: &RustdocTarget,
    options: &RustdocOptions,
    dir: &Path,
) -> Option<Vec<String>> {
    let selection = target_flags(target)?;
    let mut argv = vec![
        format!("+{toolchain}"),
        "rustdoc".to_string(),
        "-Z".to_string(),
        "unstable-options".to_string(),
        "--output-format".to_string(),
        "json".to_string(),
        "--manifest-path".to_string(),
        target.manifest_path.display().to_string(),
        "-p".to_string(),
        target.package_name.clone(),
    ];
    argv.extend(selection);
    argv.push("--target-dir".to_string());
    argv.push(dir.display().to_string());
    argv.extend(feature_and_network_flags(
        &options.features,
        options.network,
    ));
    argv.push("--".to_string());
    if options.include_private {
        argv.push("--document-private-items".to_string());
    }
    Some(argv)
}

/// The full argv including the `cargo` program, for error/`tracing` messages.
pub fn effective_argv(argv: &[String]) -> Vec<String> {
    let mut full = vec!["cargo".to_string()];
    full.extend(argv.iter().cloned());
    full
}

/// Runs `cargo rustdoc` for one target and returns the produced JSON file path.
///
/// The caller is responsible for having resolved the toolchain and rejected
/// undocumentable target kinds. A non-zero exit or a missing output file is a
/// fatal [`RustdocError`].
pub fn document_target(
    toolchain: &str,
    target: &RustdocTarget,
    options: &RustdocOptions,
    dir: &Path,
) -> Result<PathBuf, RustdocError> {
    let argv = build_argv(toolchain, target, options, dir).ok_or_else(|| {
        RustdocError::UnsupportedTargetKind(target.target_kind.as_str().to_string())
    })?;

    let output = Command::new("cargo")
        .args(&argv)
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                RustdocError::NightlyMissing
            } else {
                RustdocError::RustdocInvocationFailed {
                    argv: effective_argv(&argv),
                    stderr: error.to_string(),
                }
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if is_missing_toolchain(&stderr) {
            return Err(if crate::toolchain::is_pinned(toolchain) {
                RustdocError::NightlyMissing
            } else {
                RustdocError::ToolchainNotFound(toolchain.to_string())
            });
        }
        if is_missing_target(&stderr) {
            return Err(RustdocError::TargetNotFound(target.target_name.clone()));
        }
        return Err(RustdocError::RustdocInvocationFailed {
            argv: effective_argv(&argv),
            stderr: stderr_tail(&stderr),
        });
    }

    let output_path = dir.join("doc").join(format!("{}.json", crate_name(target)));
    if output_path.exists() {
        Ok(output_path)
    } else {
        Err(RustdocError::OutputFileMissing(
            output_path.display().to_string(),
        ))
    }
}

fn is_missing_toolchain(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("toolchain")
        && (lower.contains("is not installed") || lower.contains("not found"))
}

fn is_missing_target(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    (lower.contains("no library targets found") || lower.contains("no bin target"))
        || (lower.contains("target") && lower.contains("not found"))
}

fn stderr_tail(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(kind: RustdocTargetKind) -> RustdocTarget {
        RustdocTarget {
            package_id: cratevista_schema::EntityId::package("foo"),
            target_id: cratevista_schema::EntityId::target("foo", kind.as_str(), "foo-bar"),
            package_name: "foo".into(),
            target_name: "foo-bar".into(),
            // The planner supplies the actual crate name (dashes → underscores
            // for a bin like `foo-bar`); output discovery uses it verbatim.
            crate_name: "foo_bar".into(),
            target_kind: kind,
            manifest_path: PathBuf::from("/w/foo/Cargo.toml"),
            package_root: PathBuf::from("/w/foo"),
        }
    }

    #[test]
    fn output_discovery_uses_the_explicit_crate_name() {
        assert_eq!(crate_name(&target(RustdocTargetKind::Library)), "foo_bar");
    }

    #[test]
    fn argv_is_verified_form() {
        let dir = PathBuf::from("/w/target/cratevista/rustdoc");
        let argv = build_argv(
            "nightly-2026-07-01",
            &target(RustdocTargetKind::Library),
            &RustdocOptions::default(),
            &dir,
        )
        .unwrap();
        let joined = argv.join(" ");
        assert!(
            joined.starts_with(
                "+nightly-2026-07-01 rustdoc -Z unstable-options --output-format json"
            )
        );
        assert!(joined.contains("--manifest-path"));
        assert!(joined.contains("-p foo"));
        assert!(joined.contains("--lib"));
        assert!(joined.contains("--target-dir"));
        assert!(joined.trim_end().ends_with("--"));
    }

    #[test]
    fn private_items_flag_after_separator() {
        let dir = PathBuf::from("/w/t");
        let options = RustdocOptions {
            include_private: true,
            ..Default::default()
        };
        let argv = build_argv(
            "nightly-x",
            &target(RustdocTargetKind::Library),
            &options,
            &dir,
        )
        .unwrap();
        let sep = argv.iter().position(|a| a == "--").unwrap();
        let private = argv
            .iter()
            .position(|a| a == "--document-private-items")
            .unwrap();
        assert!(private > sep, "private-items flag must follow `--`");
    }

    #[test]
    fn binary_uses_bin_flag() {
        let dir = PathBuf::from("/w/t");
        let argv = build_argv(
            "nightly-x",
            &target(RustdocTargetKind::Binary),
            &RustdocOptions::default(),
            &dir,
        )
        .unwrap();
        let joined = argv.join(" ");
        assert!(joined.contains("--bin foo-bar"));
    }

    #[test]
    fn unsupported_kind_has_no_argv() {
        let dir = PathBuf::from("/w/t");
        assert!(
            build_argv(
                "nightly-x",
                &target(RustdocTargetKind::Other("cdylib-thing".into())),
                &RustdocOptions::default(),
                &dir,
            )
            .is_none()
        );
    }
}
