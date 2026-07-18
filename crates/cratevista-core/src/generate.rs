//! The `run_generate` use case: metadata → plan → rustdoc → graph → artifacts.
//!
//! `cratevista-core` owns paths, process execution, the clock, and artifact
//! writing; the pure graph builder owns document assembly. No filesystem, clock,
//! or process code lives in `cratevista-graph`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use cratevista_config::{ConfigOutcome, Discovered};
use cratevista_graph::{
    GraphBuildOptions, GraphInput, RustdocPlanOptions, build_document, build_rustdoc_plan,
};
use cratevista_metadata::{
    ExternalDepsMode, FeatureSelection as MetaFeatures, MetadataOptions, PackageSelection,
    TargetKinds,
};
use cratevista_rustdoc::{
    FeatureSelection as RustdocFeatures, NetworkMode as RustdocNetwork, RustdocOptions,
};
use cratevista_schema::{
    Counts, DiagnosticsReport, DocumentDiagnostic, Entity, EntityKind, GenerationReport, Generator,
    Severity, Timestamp,
};
use cratevista_server::ArtifactPaths;

use crate::artifacts::commit_artifacts;
use crate::clock::Clock;
use crate::diagnostic::Diagnostic;
use crate::exit::ExitCode;
use crate::usecase::{CommandFailure, CommandOutcome};

/// The external-dependency selection (tri-valued; a boolean cannot distinguish
/// `direct` from `full`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalDepsChoice {
    /// Members + intra-workspace deps only (default).
    Exclude,
    /// Also include direct external dependencies.
    Direct,
    /// Include the whole resolved graph.
    Full,
}

impl ExternalDepsChoice {
    fn to_mode(self) -> ExternalDepsMode {
        match self {
            ExternalDepsChoice::Exclude => ExternalDepsMode::Exclude,
            ExternalDepsChoice::Direct => ExternalDepsMode::DirectOnly,
            ExternalDepsChoice::Full => ExternalDepsMode::FullGraph,
        }
    }
}

/// Options for one `generate` run, built from the CLI.
#[derive(Debug, Clone)]
pub struct GenerateOptions {
    /// Path to the `Cargo.toml` to analyze.
    pub manifest_path: Option<PathBuf>,
    /// Continue past a failed rustdoc target, marking the result partial.
    pub keep_going: bool,
    /// Named features to enable.
    pub features: Vec<String>,
    /// Enable all features.
    pub all_features: bool,
    /// Disable default features.
    pub no_default_features: bool,
    /// Document private items.
    pub document_private_items: bool,
    /// Override the nightly toolchain.
    pub toolchain: Option<String>,
    /// External-dependency selection.
    pub external_deps: ExternalDepsChoice,
    /// Document `bin` targets (opt-in).
    pub document_bins: bool,
    /// Skip project-local configuration entirely (`--no-config`).
    ///
    /// When set, no configuration is discovered, no file under `.cratevista/` is
    /// read, and the overlay is empty — producing pure discovered output.
    pub no_config: bool,
}

impl Default for GenerateOptions {
    fn default() -> Self {
        GenerateOptions {
            manifest_path: None,
            keep_going: false,
            features: Vec::new(),
            all_features: false,
            no_default_features: false,
            document_private_items: false,
            toolchain: None,
            external_deps: ExternalDepsChoice::Exclude,
            document_bins: false,
            // Configuration is applied by default; zero-config projects simply
            // have none to find.
            no_config: false,
        }
    }
}

/// Runs the generate use case, writing `target/cratevista/{document,generation,diagnostics}.json`.
/// The metadata options one set of generate options implies.
///
/// Shared with `crate::watch`, which must ingest the *same* workspace generation
/// reads — a second, drifting idea of which packages exist would watch the wrong
/// files.
pub(crate) fn metadata_options(options: &GenerateOptions) -> MetadataOptions {
    MetadataOptions {
        manifest_path: options.manifest_path.clone(),
        cwd: None,
        selection: PackageSelection::Default,
        features: MetaFeatures {
            features: options.features.clone(),
            all_features: options.all_features,
            no_default_features: options.no_default_features,
        },
        external_deps: options.external_deps.to_mode(),
        target_kinds: TargetKinds::default(),
        network: cratevista_metadata::NetworkMode::Inherit,
    }
}

/// The committed output of one generation: the artifact triple, the exact
/// protected input paths generation read, and whether the snapshot is partial.
///
/// This is the seam `run_build` consumes so it never re-discovers what generation
/// already knows. The paths are **internal** — never surfaced in a user-facing
/// diagnostic.
#[derive(Debug, Clone)]
pub(crate) struct GeneratedArtifacts {
    /// The three committed artifact paths under `target/cratevista`.
    pub artifacts: ArtifactPaths,
    /// The absolute workspace root this generation resolved and read from. It is
    /// the authoritative anchor for a relative `build` output — never the process
    /// current directory or a bare manifest parent.
    pub workspace_root: PathBuf,
    /// The exact protected generation inputs (deduplicated, deterministic).
    pub protected_paths: Vec<PathBuf>,
    /// Whether the committed snapshot is a `--keep-going` partial.
    pub partial: bool,
}

/// The outcome of running the shared generation pipeline: either a **committed**
/// three-artifact snapshot (plus the same `CommandOutcome` `run_generate` returns),
/// or a **failure** carrying the original outcome unchanged.
///
/// The distinction is structural — a committed snapshot is proven by the presence
/// of [`GeneratedArtifacts`], not by interpreting a numeric exit code.
pub(crate) enum GenerateExecution {
    /// Generation committed its artifacts.
    Committed {
        /// The committed artifacts and their protected inputs.
        generated: GeneratedArtifacts,
        /// The exact success outcome `run_generate` returns.
        outcome: CommandOutcome,
    },
    /// Generation failed before committing; nothing was written.
    Failed {
        /// The exact failure outcome `run_generate` returns.
        outcome: CommandOutcome,
    },
}

/// The public `generate` use case: unchanged behavior, now a thin wrapper over the
/// shared [`execute_generate`] seam.
pub fn run_generate(options: &GenerateOptions, clock: &dyn Clock) -> CommandOutcome {
    match execute_generate(options, clock) {
        GenerateExecution::Committed { outcome, .. } => outcome,
        GenerateExecution::Failed { outcome } => outcome,
    }
}

/// Runs the generation pipeline once, returning both the `CommandOutcome`
/// `run_generate` reports **and** — on a committed snapshot — the artifacts and
/// protected inputs `run_build` needs. Cargo metadata, rustdoc, config and flow
/// discovery each happen exactly once here.
pub(crate) fn execute_generate(options: &GenerateOptions, clock: &dyn Clock) -> GenerateExecution {
    match generate_inner(options, clock) {
        Ok(generated) => GenerateExecution::Committed {
            generated,
            outcome: Ok(ExitCode::SUCCESS),
        },
        Err(failure) => GenerateExecution::Failed {
            outcome: Err(failure),
        },
    }
}

fn generate_inner(
    options: &GenerateOptions,
    clock: &dyn Clock,
) -> Result<GeneratedArtifacts, CommandFailure> {
    // Mutually-exclusive feature options are a CLI usage error.
    if options.all_features && !options.features.is_empty() {
        return Err(CommandFailure::new(
            Diagnostic::error(
                "invalid_feature_options",
                "`--all-features` cannot be combined with explicit `--features`",
            ),
            ExitCode::USAGE_ERROR,
        ));
    }

    let mut durations: BTreeMap<String, u64> = BTreeMap::new();

    // 1. Resolve the absolute workspace root (orchestration context).
    let workspace_root = resolve_workspace_root(options.manifest_path.as_deref())?;
    let output_dir = workspace_root.join("target").join("cratevista");
    let artifacts = ArtifactPaths::in_dir(&output_dir);

    // 2. Metadata options + 3. metadata ingest.
    let metadata_options = metadata_options(options);
    let started = Instant::now();
    let metadata = cratevista_metadata::ingest(&metadata_options).map_err(map_metadata_error)?;
    durations.insert("metadata_ms".into(), elapsed_ms(started));

    // 4. Build the rustdoc plan (pure).
    let plan = build_rustdoc_plan(
        &metadata,
        &workspace_root,
        &RustdocPlanOptions {
            include_binaries: options.document_bins,
        },
    )
    .map_err(|error| {
        CommandFailure::runtime(Diagnostic::error("plan_failed", error.to_string()))
    })?;

    // 5. Rustdoc ingest — skipped entirely for an empty plan.
    let empty_plan = plan.targets.is_empty();
    let rustdoc = if empty_plan {
        None
    } else {
        let rustdoc_options = RustdocOptions {
            features: RustdocFeatures {
                features: options.features.clone(),
                all_features: options.all_features,
                no_default_features: options.no_default_features,
            },
            include_private: options.document_private_items,
            keep_going: options.keep_going,
            toolchain: options.toolchain.clone(),
            target_dir: None,
            network: RustdocNetwork::Inherit,
        };
        let started = Instant::now();
        let ingest =
            cratevista_rustdoc::ingest(&plan, &rustdoc_options).map_err(map_rustdoc_error)?;
        durations.insert("rustdoc_ms".into(), elapsed_ms(started));
        Some(ingest)
    };

    // Capture runtime info before the inputs are moved into the graph builder.
    let toolchain = rustdoc.as_ref().map(|r| r.summary.compat.nightly.clone());
    let rustdoc_format_version = rustdoc.as_ref().map(|r| r.summary.compat.format_version);

    // 6. Project-local configuration (issue 08) -> the overlay seam.
    //
    // Loaded BEFORE the build, because the overlay is an input to it. Failures
    // here are never fatal: a broken configuration costs its own contents, the
    // discovered document is still built and committed, and the exit code stays
    // 0. `--no-config` skips discovery entirely, so nothing under `.cratevista/`
    // is even opened. Discovery runs **once**: the discovered set is reused for the
    // overlay and for the protected-input collection below.
    let (config, discovered) = if options.no_config {
        (ConfigOutcome::default(), Discovered::default())
    } else {
        let started = Instant::now();
        let discovered = cratevista_config::discover(&workspace_root);
        let outcome = cratevista_config::load_config_with(&workspace_root, &discovered);
        durations.insert("config_ms".into(), elapsed_ms(started));
        (outcome, discovered)
    };
    let config_diagnostics =
        crate::config_diagnostics::to_document_diagnostics(&config.diagnostics);

    // Collect the exact protected inputs from the SAME metadata + config discovery,
    // before `metadata`/`config.overlay` are moved into the graph builder.
    let protected_paths = collect_protected_paths(
        &workspace_root,
        &artifacts,
        &metadata.entities,
        &config.referenced_files,
        &discovered,
    );

    // 7. Build the document (pure).
    let started = Instant::now();
    let result = build_document(
        GraphInput {
            metadata,
            rustdoc,
            overlay: config.overlay,
        },
        &GraphBuildOptions::default(),
    )
    .map_err(|error| {
        CommandFailure::runtime(Diagnostic::error("graph_build_failed", error.to_string()))
    })?;
    durations.insert("graph_ms".into(), elapsed_ms(started));

    // Combine diagnostics: graph (which already carries metadata's and rustdoc's),
    // configuration, and the empty-plan info note. The sort below makes the merge
    // order-independent and therefore deterministic.
    let mut diagnostics = result.diagnostics.clone();
    diagnostics.extend(config_diagnostics);
    if empty_plan {
        diagnostics.push(DocumentDiagnostic::new(
            Severity::Info,
            "no_documentable_rustdoc_targets",
            "no documentable library or proc-macro targets; produced a metadata-only document",
        ));
    }
    diagnostics.sort();
    diagnostics.dedup();

    // 8. GenerationReport (core owns the clock + durations).
    let generation = GenerationReport {
        generator: Generator {
            name: "cargo-cratevista".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        generated_at: Timestamp::new(clock.now_rfc3339()),
        toolchain,
        rustdoc_format_version,
        input_hashes: BTreeMap::new(),
        counts: Counts {
            entities: result.summary.entity_count as u64,
            relations: result.summary.relation_count as u64,
            views: result.summary.view_count as u64,
            diagnostics: diagnostics.len() as u64,
        },
        durations_ms: durations,
        // Populated by `commit_artifacts` from the exact committed bytes.
        artifact_hashes: None,
        partial: result.partial,
    };

    // 9. DiagnosticsReport.
    let diagnostics_report = DiagnosticsReport::new(diagnostics.clone());

    // 10. Commit the three artifacts (the writer computes + embeds artifact_hashes).
    commit_artifacts(
        &output_dir,
        &result.document,
        &diagnostics_report,
        generation,
    )
    .map_err(|error| {
        CommandFailure::runtime(
            Diagnostic::error("artifact_write_failed", error.to_string())
                .with_remediation("Check that target/cratevista/ is writable."),
        )
    })?;

    // 11. Report to the user (identical to the pre-refactor `run_generate`).
    print_summary(
        &output_dir,
        &result,
        &diagnostics,
        result.partial,
        empty_plan,
    );

    Ok(GeneratedArtifacts {
        artifacts,
        workspace_root,
        protected_paths,
        partial: result.partial,
    })
}

/// Collects the exact protected generation inputs — the real discovered paths, not
/// a recursive workspace walk. Deterministically sorted and deduplicated;
/// non-UTF-8 `PathBuf`s are preserved; no symlink is followed here (path joins
/// only). Never surfaced in a user-facing diagnostic.
fn collect_protected_paths(
    workspace_root: &Path,
    artifacts: &ArtifactPaths,
    entities: &[Entity],
    referenced: &[cratevista_config::ReferencedConfigFile],
    discovered: &Discovered,
) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // Workspace root + root manifest + lockfile (when present).
    paths.push(workspace_root.to_path_buf());
    paths.push(workspace_root.join("Cargo.toml"));
    let lockfile = workspace_root.join("Cargo.lock");
    if lockfile.is_file() {
        paths.push(lockfile);
    }

    // Member manifests and member target source roots, from the same metadata the
    // document was built from. Externals (registry sources) are never protected.
    for entity in entities {
        let Some(source) = &entity.source else {
            continue;
        };
        let path = workspace_root.join(source.path.as_str());
        if entity.kind == EntityKind::new(EntityKind::PACKAGE) {
            if is_member_package(entity.id.as_str()) {
                paths.push(path);
            }
        } else if entity.kind == EntityKind::new(EntityKind::TARGET)
            && is_member_target(entity)
            && let Some(source_root) = path.parent()
        {
            // The Rust source root (the directory holding `src/lib.rs`).
            paths.push(source_root.to_path_buf());
        }
    }

    // Configuration: `cratevista.toml`, discovered flow/override files, and every
    // file the configuration explicitly references (docs/examples).
    if let Some(root_config) = &discovered.root {
        paths.push(root_config.clone());
    }
    paths.extend(discovered.flows.iter().cloned());
    paths.extend(discovered.overrides.iter().cloned());
    for reference in referenced {
        paths.push(workspace_root.join(reference.path.as_str()));
    }

    // The artifact directory and the three committed artifacts.
    paths.push(output_root_of(artifacts));
    paths.push(artifacts.document.clone());
    paths.push(artifacts.generation.clone());
    paths.push(artifacts.diagnostics.clone());

    paths.sort();
    paths.dedup();
    paths
}

/// The directory holding the artifact triple (their shared parent).
fn output_root_of(artifacts: &ArtifactPaths) -> PathBuf {
    artifacts
        .document
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// Whether a package entity id names a workspace member (`package:{name}`; an
/// external carries `package:{name}@{version}`).
fn is_member_package(id: &str) -> bool {
    id.starts_with("package:") && !id.contains('@')
}

/// Whether a target entity belongs to a workspace member.
fn is_member_target(entity: &Entity) -> bool {
    entity
        .parent
        .as_ref()
        .is_some_and(|parent| is_member_package(parent.as_str()))
}

fn print_summary(
    output_dir: &std::path::Path,
    result: &cratevista_graph::GraphBuildResult,
    diagnostics: &[DocumentDiagnostic],
    partial: bool,
    empty_plan: bool,
) {
    println!("Wrote {}", output_dir.display());
    println!(
        "  entities: {}  relations: {}  views: {}  diagnostics: {}",
        result.summary.entity_count,
        result.summary.relation_count,
        result.summary.view_count,
        diagnostics.len()
    );
    if let Some(percent) = result.summary.coverage_percent {
        println!("  documentation coverage: {percent}%");
    }
    if empty_plan {
        println!("  note: no documentable library/proc-macro targets — metadata-only document.");
    }
    if partial {
        println!(
            "  PARTIAL: some rustdoc targets were skipped (--keep-going); see diagnostics.json."
        );
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

/// Resolves the absolute workspace root via `cargo locate-project --workspace`.
///
/// Shared by `generate`, `serve`, and `open` so they all agree on where
/// `target/cratevista` lives. Uses only stable `cargo` (no nightly).
pub(crate) fn resolve_workspace_root(
    manifest_path: Option<&Path>,
) -> Result<PathBuf, CommandFailure> {
    let mut command = Command::new("cargo");
    command.args(["locate-project", "--workspace", "--message-format", "plain"]);
    if let Some(manifest_path) = manifest_path {
        command.arg("--manifest-path").arg(manifest_path);
    }
    let output = command.output().map_err(|error| {
        CommandFailure::new(
            Diagnostic::error("cargo_not_found", format!("could not run cargo: {error}"))
                .with_remediation("Install Rust and Cargo from https://rustup.rs/."),
            ExitCode::ENVIRONMENT_ERROR,
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CommandFailure::new(
            Diagnostic::error(
                "no_cargo_workspace",
                format!("cargo could not locate a workspace: {}", stderr.trim()),
            )
            .with_remediation("Run inside a Cargo workspace, or pass --manifest-path."),
            ExitCode::ENVIRONMENT_ERROR,
        ));
    }
    let manifest = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let manifest_path = PathBuf::from(manifest);
    manifest_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| {
            CommandFailure::runtime(Diagnostic::error(
                "no_cargo_workspace",
                "cargo returned an unexpected workspace manifest path",
            ))
        })
}

fn map_metadata_error(error: cratevista_metadata::MetadataError) -> CommandFailure {
    let exit = match error.code() {
        "cargo_not_found" => ExitCode::ENVIRONMENT_ERROR,
        _ => ExitCode::RUNTIME_ERROR,
    };
    let mut diagnostic = Diagnostic::error(error.code(), error.to_string());
    if let Some(remediation) = error.remediation() {
        diagnostic = diagnostic.with_remediation(remediation);
    }
    CommandFailure::new(diagnostic, exit)
}

fn map_rustdoc_error(error: cratevista_rustdoc::RustdocError) -> CommandFailure {
    let exit = match error.code() {
        "nightly_missing" | "toolchain_not_found" => ExitCode::ENVIRONMENT_ERROR,
        _ => ExitCode::RUNTIME_ERROR,
    };
    let mut diagnostic = Diagnostic::error(error.code(), error.to_string());
    if let Some(remediation) = error.remediation() {
        diagnostic = diagnostic.with_remediation(remediation);
    }
    CommandFailure::new(diagnostic, exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use cratevista_config::{ReferencedConfigFile, ReferencedFileKind};
    use cratevista_schema::{
        EntityId, LocalizedText, Provenance, RepoRelativePath, SourceLocation,
    };

    fn member_package(name: &str, manifest: &str) -> Entity {
        let mut entity = Entity::new(
            EntityId::package(name),
            EntityKind::new(EntityKind::PACKAGE),
            LocalizedText::new(name),
            name,
            Provenance::Discovered,
        );
        entity.source = Some(SourceLocation::new(
            RepoRelativePath::new(manifest).unwrap(),
            None,
        ));
        entity
    }

    fn member_target(package: &str, src: &str) -> Entity {
        let mut entity = Entity::new(
            EntityId::target(package, "lib", package),
            EntityKind::new(EntityKind::TARGET),
            LocalizedText::new(package),
            package,
            Provenance::Discovered,
        );
        entity.parent = Some(EntityId::package(package));
        entity.source = Some(SourceLocation::new(
            RepoRelativePath::new(src).unwrap(),
            None,
        ));
        entity
    }

    #[test]
    fn protected_paths_cover_every_discovered_input_category() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        // A real lockfile so the present-only inclusion is exercised.
        std::fs::write(root.join("Cargo.lock"), "").unwrap();
        let artifacts = ArtifactPaths::in_dir(&root.join("target").join("cratevista"));

        let entities = vec![
            member_package("mylib", "crates/mylib/Cargo.toml"),
            member_target("mylib", "crates/mylib/src/lib.rs"),
            // An external package with a source must NOT be protected.
            {
                let mut external = Entity::new(
                    EntityId::external_package("serde", "1.0.0"),
                    EntityKind::new(EntityKind::PACKAGE),
                    LocalizedText::new("serde"),
                    "serde",
                    Provenance::Discovered,
                );
                external.source = Some(SourceLocation::new(
                    RepoRelativePath::new("crates/should-not-appear/Cargo.toml").unwrap(),
                    None,
                ));
                external
            },
        ];
        let referenced = vec![ReferencedConfigFile {
            path: RepoRelativePath::new("docs/guide.md").unwrap(),
            kind: ReferencedFileKind::FlowDoc,
        }];
        let discovered = Discovered {
            root: Some(root.join("cratevista.toml")),
            flows: vec![root.join(".cratevista/flows/a.toml")],
            overrides: vec![root.join(".cratevista/overrides/o.toml")],
        };

        let paths = collect_protected_paths(&root, &artifacts, &entities, &referenced, &discovered);
        let has = |p: PathBuf| paths.contains(&p);

        assert!(has(root.clone()), "workspace root");
        assert!(has(root.join("Cargo.toml")), "root manifest");
        assert!(has(root.join("Cargo.lock")), "lockfile (present)");
        assert!(has(root.join("crates/mylib/Cargo.toml")), "member manifest");
        assert!(has(root.join("crates/mylib/src")), "target source root");
        assert!(has(root.join("cratevista.toml")), "config root");
        assert!(has(root.join(".cratevista/flows/a.toml")), "flow file");
        assert!(
            has(root.join(".cratevista/overrides/o.toml")),
            "override file"
        );
        assert!(has(root.join("docs/guide.md")), "referenced doc");
        assert!(has(root.join("target/cratevista")), "artifact root");
        assert!(has(root.join("target/cratevista/document.json")));
        assert!(has(root.join("target/cratevista/generation.json")));
        assert!(has(root.join("target/cratevista/diagnostics.json")));
        assert!(
            !has(root.join("crates/should-not-appear/Cargo.toml")),
            "external sources are not protected"
        );

        // Deterministic: sorted and deduplicated.
        let mut expected = paths.clone();
        expected.sort();
        expected.dedup();
        assert_eq!(paths, expected);
    }

    #[test]
    fn a_missing_lockfile_is_not_protected() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let artifacts = ArtifactPaths::in_dir(&root.join("target").join("cratevista"));
        let paths = collect_protected_paths(&root, &artifacts, &[], &[], &Discovered::default());
        assert!(
            !paths.contains(&root.join("Cargo.lock")),
            "absent lockfile excluded"
        );
    }

    #[test]
    fn external_deps_mapping_is_exact() {
        assert_eq!(
            ExternalDepsChoice::Exclude.to_mode(),
            ExternalDepsMode::Exclude
        );
        assert_eq!(
            ExternalDepsChoice::Direct.to_mode(),
            ExternalDepsMode::DirectOnly
        );
        assert_eq!(
            ExternalDepsChoice::Full.to_mode(),
            ExternalDepsMode::FullGraph
        );
    }

    #[test]
    fn conflicting_feature_options_are_a_usage_error() {
        let options = GenerateOptions {
            all_features: true,
            features: vec!["x".into()],
            ..Default::default()
        };
        // Validated before any cargo invocation → deterministic, no environment needs.
        let failure = run_generate(&options, &FixedClock("2026-07-14T00:00:00Z".into()))
            .expect_err("must be a usage error");
        assert_eq!(failure.exit, ExitCode::USAGE_ERROR);
    }
}
