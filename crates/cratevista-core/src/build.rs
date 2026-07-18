//! The `build` use case: generate, then materialize a static site (PRD 10, Phase 2C).
//!
//! `run_build` runs the **existing** generation pipeline once and, when it commits
//! its three-artifact snapshot, hands the committed [`ArtifactPaths`], the embedded
//! frontend assets, the [`SiteOptions`] and the **exact protected input paths** to
//! [`materialize_static_site`]. Everything filesystem-critical — resolving the
//! output, deriving its key, preparing parents, locking, recovery, staging,
//! publication and rollback — is owned by `materialize_static_site` and happens
//! there exactly once. `run_build` performs **none** of it and never constructs a
//! caller-controlled `OutputSafety`.
//!
//! On a fatal generation failure the original generation `CommandOutcome` is
//! returned unchanged and nothing about the requested output is touched.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use cratevista_server::ArtifactPaths;

use crate::clock::Clock;
use crate::exit::ExitCode;
use crate::generate::{GenerateExecution, GenerateOptions, execute_generate};
use crate::static_site::{
    BasePath, BuildError, PublishedSite, SiteOptions, materialize_static_site,
};
use crate::usecase::CommandOutcome;

/// Options for one `build` run: the generation options plus the static-site output
/// location and optional base path.
///
/// Core owns the type; it deliberately has **no `Default`** that binds `output` to
/// the current working directory. The default-output policy belongs to the CLI
/// adapter (Implementation sequence step 3), which has the workspace context to
/// choose `target/cratevista/site`.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// How to generate the artifact snapshot.
    pub generate: GenerateOptions,
    /// Where to materialize the static site (absolute).
    pub output: PathBuf,
    /// The optional `<base href>` base path.
    pub base_path: Option<BasePath>,
}

/// Runs the `build` use case with the real generation, embedded assets and
/// materializer.
pub fn run_build(options: &BuildOptions, clock: &dyn Clock) -> CommandOutcome {
    run_build_with(
        options,
        clock,
        &RealGenerate,
        &EmbeddedAssets,
        &RealMaterialize,
    )
}

// ---------------------------------------------------------------------------
// Injectable seams (so orchestration is testable without cargo/rustdoc)
// ---------------------------------------------------------------------------

/// The generation step.
pub(crate) trait GenerateStep {
    /// Runs the generation pipeline once.
    fn execute(&self, options: &GenerateOptions, clock: &dyn Clock) -> GenerateExecution;
}

/// The embedded-asset source.
pub(crate) trait AssetSource {
    /// The frontend assets to materialize, as `(path, bytes)`.
    fn assets(&self) -> Vec<(String, Cow<'static, [u8]>)>;
}

/// The materialization step (the single owner of output filesystem work).
pub(crate) trait MaterializeStep {
    /// Materializes the static site exactly once.
    fn materialize(
        &self,
        artifacts: &ArtifactPaths,
        assets: Vec<(String, Cow<'static, [u8]>)>,
        options: &SiteOptions,
        protected_paths: &[PathBuf],
    ) -> Result<PublishedSite, BuildError>;
}

struct RealGenerate;
impl GenerateStep for RealGenerate {
    fn execute(&self, options: &GenerateOptions, clock: &dyn Clock) -> GenerateExecution {
        execute_generate(options, clock)
    }
}

struct EmbeddedAssets;
impl AssetSource for EmbeddedAssets {
    fn assets(&self) -> Vec<(String, Cow<'static, [u8]>)> {
        cratevista_server::assets::embedded_assets().collect()
    }
}

struct RealMaterialize;
impl MaterializeStep for RealMaterialize {
    fn materialize(
        &self,
        artifacts: &ArtifactPaths,
        assets: Vec<(String, Cow<'static, [u8]>)>,
        options: &SiteOptions,
        protected_paths: &[PathBuf],
    ) -> Result<PublishedSite, BuildError> {
        materialize_static_site(artifacts, assets.into_iter(), options, protected_paths)
    }
}

/// The seam-parameterized core. Production uses the real steps; tests inject
/// recording doubles to prove the one-call orchestration.
pub(crate) fn run_build_with(
    options: &BuildOptions,
    clock: &dyn Clock,
    generate: &dyn GenerateStep,
    assets: &dyn AssetSource,
    materialize: &dyn MaterializeStep,
) -> CommandOutcome {
    // 1. Run generation once. On a fatal failure, return its outcome unchanged —
    //    without constructing SiteOptions, requesting assets, or touching the
    //    requested output in any way.
    let generated = match generate.execute(&options.generate, clock) {
        GenerateExecution::Failed { outcome } => return outcome,
        GenerateExecution::Committed { generated, .. } => generated,
    };

    // 2. Committed snapshot → resolve the output against the **authoritative
    //    workspace root the generation just used** (never the process CWD), then
    //    materialize exactly once. The materializer alone resolves/locks/recovers/
    //    publishes; `run_build` does none of that.
    let output = anchor_output(&options.output, &generated.workspace_root);
    let site_options = SiteOptions::new(output, options.base_path.clone(), clock);
    let asset_set = assets.assets();
    match materialize.materialize(
        &generated.artifacts,
        asset_set,
        &site_options,
        &generated.protected_paths,
    ) {
        Ok(published) => {
            report_success(&published, generated.partial);
            Ok(ExitCode::SUCCESS)
        }
        // The one authoritative BuildError → diagnostic/exit mapping (runtime/usage).
        Err(error) => Err(error.to_command_failure()),
    }
}

/// Anchors a relative `build` output to the generated workspace root; an absolute
/// output is used unchanged. This is the **only** place the output is turned
/// absolute, and it happens after generation commits — so a relative default like
/// `target/cratevista/site` always lands inside the analyzed workspace, even when
/// `build` is invoked from an unrelated working directory.
fn anchor_output(output: &Path, workspace_root: &Path) -> PathBuf {
    if output.is_absolute() {
        output.to_path_buf()
    } else {
        workspace_root.join(output)
    }
}

/// Prints the success report. Success-only, so a human-readable output path is
/// allowed here; it makes **no** claim about static-mode frontend behavior, E2E
/// verification, releases or publishing (all later phases).
fn report_success(published: &PublishedSite, partial: bool) {
    println!(
        "Materialized static site at {}",
        published.output().display()
    );
    let base = match published.base_path() {
        Some(base) => format!(", base path {base}"),
        None => String::new(),
    };
    println!("  {} assets written{base}", published.asset_count());
    if partial {
        println!("  note: built from a PARTIAL generation snapshot (--keep-going).");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;

    use crate::clock::FixedClock;
    use crate::diagnostic::Diagnostic;
    use crate::generate::GeneratedArtifacts;
    use crate::static_site::marker::MARKER_FILENAME;
    use crate::static_site::{Marker, MarkerKind};
    use crate::usecase::CommandFailure;

    const CLOCK_TIME: &str = "2026-07-18T00:00:00Z";

    fn clock() -> FixedClock {
        FixedClock(CLOCK_TIME.to_string())
    }

    // --- fixtures ----------------------------------------------------------

    /// Writes the three committed artifacts into `root/artifacts` and returns them.
    fn write_committed_artifacts(root: &Path) -> ArtifactPaths {
        let dir = root.join("artifacts");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("document.json"), b"{\"document\":1}").unwrap();
        fs::write(dir.join("generation.json"), b"{\"generation\":1}").unwrap();
        fs::write(dir.join("diagnostics.json"), b"{\"diagnostics\":1}").unwrap();
        ArtifactPaths::in_dir(&dir)
    }

    fn build_options(output: &Path, base: Option<&str>) -> BuildOptions {
        BuildOptions {
            generate: GenerateOptions::default(),
            output: output.to_path_buf(),
            base_path: base.map(|b| BasePath::parse(b).unwrap()),
        }
    }

    // --- generation doubles ------------------------------------------------

    struct FakeGenerate {
        execution: RefCell<Option<GenerateExecution>>,
        calls: Cell<u32>,
    }
    impl FakeGenerate {
        fn committed(generated: GeneratedArtifacts) -> FakeGenerate {
            FakeGenerate {
                execution: RefCell::new(Some(GenerateExecution::Committed {
                    generated,
                    outcome: Ok(ExitCode::SUCCESS),
                })),
                calls: Cell::new(0),
            }
        }
        fn failed(outcome: CommandOutcome) -> FakeGenerate {
            FakeGenerate {
                execution: RefCell::new(Some(GenerateExecution::Failed { outcome })),
                calls: Cell::new(0),
            }
        }
    }
    impl GenerateStep for FakeGenerate {
        fn execute(&self, _options: &GenerateOptions, _clock: &dyn Clock) -> GenerateExecution {
            self.calls.set(self.calls.get() + 1);
            self.execution
                .borrow_mut()
                .take()
                .expect("generation executed more than once")
        }
    }

    fn committed(root: &Path, protected: Vec<PathBuf>, partial: bool) -> GeneratedArtifacts {
        GeneratedArtifacts {
            artifacts: write_committed_artifacts(root),
            workspace_root: root.to_path_buf(),
            protected_paths: protected,
            partial,
        }
    }

    /// A committed result whose `workspace_root` is set explicitly (to exercise
    /// relative-output anchoring).
    fn committed_rooted(root: &Path, workspace_root: &Path) -> GeneratedArtifacts {
        GeneratedArtifacts {
            artifacts: write_committed_artifacts(root),
            workspace_root: workspace_root.to_path_buf(),
            protected_paths: Vec::new(),
            partial: false,
        }
    }

    // --- asset doubles -----------------------------------------------------

    /// A minimal valid asset set (one index.html + one asset).
    fn test_assets() -> Vec<(String, Cow<'static, [u8]>)> {
        vec![
            (
                "index.html".to_string(),
                Cow::Borrowed(b"<html><head></head><body></body></html>" as &[u8]),
            ),
            (
                "assets/app.abcdef12.js".to_string(),
                Cow::Borrowed(b"console.log(1)" as &[u8]),
            ),
        ]
    }

    struct RecordingAssets {
        requested: Cell<bool>,
    }
    impl RecordingAssets {
        fn new() -> RecordingAssets {
            RecordingAssets {
                requested: Cell::new(false),
            }
        }
    }
    impl AssetSource for RecordingAssets {
        fn assets(&self) -> Vec<(String, Cow<'static, [u8]>)> {
            self.requested.set(true);
            test_assets()
        }
    }

    // --- materializer doubles ---------------------------------------------

    #[derive(Default)]
    struct Captured {
        calls: u32,
        artifacts: Option<ArtifactPaths>,
        protected: Option<Vec<PathBuf>>,
        output: Option<PathBuf>,
        base: Option<Option<String>>,
        asset_names: Option<Vec<String>>,
    }

    struct RecordingMaterialize {
        captured: RefCell<Captured>,
        result: RefCell<Option<Result<PublishedSite, BuildError>>>,
    }
    impl RecordingMaterialize {
        fn returning(result: Result<PublishedSite, BuildError>) -> RecordingMaterialize {
            RecordingMaterialize {
                captured: RefCell::new(Captured::default()),
                result: RefCell::new(Some(result)),
            }
        }
    }
    impl MaterializeStep for RecordingMaterialize {
        fn materialize(
            &self,
            artifacts: &ArtifactPaths,
            assets: Vec<(String, Cow<'static, [u8]>)>,
            options: &SiteOptions,
            protected_paths: &[PathBuf],
        ) -> Result<PublishedSite, BuildError> {
            let mut captured = self.captured.borrow_mut();
            captured.calls += 1;
            captured.artifacts = Some(artifacts.clone());
            captured.protected = Some(protected_paths.to_vec());
            captured.output = Some(options.output.clone());
            captured.base = Some(options.base_path.as_ref().map(|b| b.as_str().to_string()));
            captured.asset_names = Some(assets.into_iter().map(|(name, _)| name).collect());
            self.result
                .borrow_mut()
                .take()
                .expect("materialized more than once")
        }
    }

    // === Test 1: generation fatal failure ================================

    #[test]
    fn generation_failure_returns_the_original_outcome_and_skips_materialization() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let failure = CommandFailure::new(
            Diagnostic::error("no_cargo_workspace", "cargo could not locate a workspace"),
            ExitCode::ENVIRONMENT_ERROR,
        );
        let generate = FakeGenerate::failed(Err(failure));
        let assets = RecordingAssets::new();
        let materialize = RecordingMaterialize::returning(Ok(PublishedSite::new_for_test(
            output.clone(),
            0,
            None,
        )));

        let outcome = run_build_with(
            &build_options(&output, None),
            &clock(),
            &generate,
            &assets,
            &materialize,
        );

        // Exact original outcome; nothing else happened.
        let error = outcome.expect_err("must fail");
        assert_eq!(error.exit, ExitCode::ENVIRONMENT_ERROR);
        assert_eq!(error.diagnostic.code, "no_cargo_workspace");
        assert_eq!(materialize.captured.borrow().calls, 0);
        assert!(!assets.requested.get(), "assets must not be requested");
        assert!(!output.exists(), "the output must be untouched");
    }

    // === Test 2: committed success passes exact inputs ===================

    #[test]
    fn committed_generation_materializes_once_with_the_exact_inputs() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let protected = vec![temp.path().join("Cargo.toml"), temp.path().join("src")];
        let generated = committed(temp.path(), protected.clone(), false);
        let expected_artifacts = generated.artifacts.clone();

        let generate = FakeGenerate::committed(generated);
        let assets = RecordingAssets::new();
        let materialize = RecordingMaterialize::returning(Ok(PublishedSite::new_for_test(
            output.clone(),
            2,
            Some("/demo/".to_string()),
        )));

        let outcome = run_build_with(
            &build_options(&output, Some("/demo/")),
            &clock(),
            &generate,
            &assets,
            &materialize,
        );
        assert_eq!(outcome.expect("build succeeds"), ExitCode::SUCCESS);

        let captured = materialize.captured.borrow();
        assert_eq!(captured.calls, 1, "materializer called exactly once");
        assert_eq!(
            captured.artifacts.as_ref().unwrap().document,
            expected_artifacts.document
        );
        assert_eq!(captured.output.as_ref().unwrap(), &output);
        assert_eq!(captured.base.as_ref().unwrap().as_deref(), Some("/demo/"));
        assert_eq!(captured.protected.as_ref().unwrap(), &protected);
        assert!(assets.requested.get(), "the embedded asset source was used");
        assert!(
            captured
                .asset_names
                .as_ref()
                .unwrap()
                .contains(&"index.html".to_string())
        );
    }

    // === Test 3: materialization error mapping ===========================

    #[test]
    fn materialization_error_is_mapped_through_build_error() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let generate = FakeGenerate::committed(committed(temp.path(), vec![], false));
        let assets = RecordingAssets::new();
        let materialize = RecordingMaterialize::returning(Err(BuildError::OutputNotOwned));

        let outcome = run_build_with(
            &build_options(&output, None),
            &clock(),
            &generate,
            &assets,
            &materialize,
        );
        let error = outcome.expect_err("must map the BuildError");
        // Exact BuildError code + exit mapping preserved, not a generic error.
        assert_eq!(error.diagnostic.code, "build_output_not_owned");
        assert_eq!(error.exit, ExitCode::RUNTIME_ERROR);
    }

    #[test]
    fn base_path_usage_error_maps_to_usage_exit() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let generate = FakeGenerate::committed(committed(temp.path(), vec![], false));
        let assets = RecordingAssets::new();
        let materialize = RecordingMaterialize::returning(Err(BuildError::InvalidBasePath {
            reason: "it looks like a URL with a scheme",
        }));

        let outcome = run_build_with(
            &build_options(&output, None),
            &clock(),
            &generate,
            &assets,
            &materialize,
        );
        let error = outcome.expect_err("must map the usage error");
        assert_eq!(error.diagnostic.code, "build_invalid_base_path");
        assert_eq!(error.exit, ExitCode::USAGE_ERROR);
    }

    // === Test 4: materialization success with the REAL materializer ======

    #[test]
    fn real_materializer_produces_marker_c_and_content_cargo_free() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        // Committed artifacts with known bytes; no output overlap in protected.
        let generated = committed(temp.path(), vec![temp.path().join("Cargo.toml")], false);
        let doc_bytes = fs::read(&generated.artifacts.document).unwrap();

        let generate = FakeGenerate::committed(generated);
        let assets = RecordingAssets::new();
        let outcome = run_build_with(
            &build_options(&output, Some("/demo/")),
            &clock(),
            &generate,
            &assets,
            &RealMaterialize,
        );
        assert_eq!(outcome.expect("build succeeds"), ExitCode::SUCCESS);

        // Marker C (no output_key) and the expected content are present.
        assert!(output.join("index.html").is_file());
        assert!(output.join("assets/app.abcdef12.js").is_file());
        assert_eq!(fs::read(output.join("document.json")).unwrap(), doc_bytes);
        let marker = Marker::parse(&fs::read(output.join(MARKER_FILENAME)).unwrap()).unwrap();
        assert_eq!(marker.kind(), MarkerKind::Site);
        assert_eq!(marker.output_key(), None);
        // The base path was injected exactly once.
        let index = fs::read_to_string(output.join("index.html")).unwrap();
        assert_eq!(index.matches("<base ").count(), 1);
        assert_eq!(index.matches(r#"name="cratevista-mode""#).count(), 1);
    }

    // === Test 5: partial committed generation ============================

    #[test]
    fn a_partial_committed_snapshot_still_materializes_unchanged_bytes() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        // `partial: true`, mirroring a --keep-going committed snapshot.
        let generated = committed(temp.path(), vec![temp.path().join("Cargo.toml")], true);
        let doc = fs::read(&generated.artifacts.document).unwrap();
        let generation = fs::read(&generated.artifacts.generation).unwrap();
        let diag = fs::read(&generated.artifacts.diagnostics).unwrap();

        let generate = FakeGenerate::committed(generated);
        let assets = RecordingAssets::new();
        let outcome = run_build_with(
            &build_options(&output, None),
            &clock(),
            &generate,
            &assets,
            &RealMaterialize,
        );
        assert_eq!(outcome.expect("build succeeds"), ExitCode::SUCCESS);
        // The three artifact bytes are copied unchanged (run_build never rewrites them).
        assert_eq!(fs::read(output.join("document.json")).unwrap(), doc);
        assert_eq!(
            fs::read(output.join("generation.json")).unwrap(),
            generation
        );
        assert_eq!(fs::read(output.join("diagnostics.json")).unwrap(), diag);
    }

    // === Test 7: no double filesystem orchestration ======================

    #[test]
    fn run_build_invokes_the_materializer_exactly_once_and_does_no_preflight() {
        // If run_build did its own lock/recovery/preflight, it would call the
        // materializer more than once or touch the output first. A single-shot
        // recording materializer proves neither happens.
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let generate = FakeGenerate::committed(committed(temp.path(), vec![], false));
        let assets = RecordingAssets::new();
        let materialize = RecordingMaterialize::returning(Ok(PublishedSite::new_for_test(
            output.clone(),
            2,
            None,
        )));

        let outcome = run_build_with(
            &build_options(&output, None),
            &clock(),
            &generate,
            &assets,
            &materialize,
        );
        assert_eq!(outcome.expect("build succeeds"), ExitCode::SUCCESS);
        assert_eq!(materialize.captured.borrow().calls, 1);
        // run_build created nothing at the output itself — the recording materializer
        // never wrote there.
        assert!(!output.exists());
    }

    // === Test 9: existing owned output survives a generation failure =====

    #[test]
    fn an_existing_output_is_byte_identical_when_generation_fails() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("index.html"), b"EXISTING SITE").unwrap();
        let before = fs::read(output.join("index.html")).unwrap();

        let generate = FakeGenerate::failed(Err(CommandFailure::new(
            Diagnostic::error("plan_failed", "the plan failed"),
            ExitCode::RUNTIME_ERROR,
        )));
        let assets = RecordingAssets::new();
        let materialize = RecordingMaterialize::returning(Ok(PublishedSite::new_for_test(
            output.clone(),
            0,
            None,
        )));

        let outcome = run_build_with(
            &build_options(&output, None),
            &clock(),
            &generate,
            &assets,
            &materialize,
        );
        assert!(outcome.is_err());
        // The pre-existing site is untouched; nothing downstream ran.
        assert_eq!(fs::read(output.join("index.html")).unwrap(), before);
        assert_eq!(materialize.captured.borrow().calls, 0);
        assert!(!assets.requested.get());
    }

    // === Output resolution: relative anchors to the generated workspace root =

    fn resolved_output(build_output: &Path, workspace_root: &Path) -> PathBuf {
        let temp = TempDir::new().unwrap();
        let generate = FakeGenerate::committed(committed_rooted(temp.path(), workspace_root));
        let assets = RecordingAssets::new();
        let materialize = RecordingMaterialize::returning(Ok(PublishedSite::new_for_test(
            build_output.to_path_buf(),
            2,
            None,
        )));
        run_build_with(
            &build_options(build_output, None),
            &clock(),
            &generate,
            &assets,
            &materialize,
        )
        .expect("build succeeds");
        materialize.captured.borrow().output.clone().unwrap()
    }

    #[test]
    fn a_relative_output_is_anchored_to_the_workspace_root() {
        let ws = TempDir::new().unwrap();
        // The default relative value the CLI supplies.
        assert_eq!(
            resolved_output(Path::new("target/cratevista/site"), ws.path()),
            ws.path().join("target/cratevista/site")
        );
        // `--output dist` and a deeper relative path.
        assert_eq!(
            resolved_output(Path::new("dist"), ws.path()),
            ws.path().join("dist")
        );
        assert_eq!(
            resolved_output(Path::new("a/b/site"), ws.path()),
            ws.path().join("a/b/site")
        );
    }

    #[test]
    fn an_absolute_output_is_used_unchanged() {
        let ws = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let absolute = elsewhere.path().join("exact/site");
        // The workspace root is a *different* directory; the absolute path wins.
        assert_eq!(resolved_output(&absolute, ws.path()), absolute);
    }

    #[test]
    fn a_relative_output_ignores_any_ambient_current_directory() {
        // `anchor_output` only ever consults the workspace root, never `current_dir`.
        let ws = TempDir::new().unwrap();
        let resolved = resolved_output(Path::new("dist"), ws.path());
        assert!(resolved.starts_with(ws.path()), "{resolved:?}");
        let cwd = std::env::current_dir().unwrap();
        assert!(!resolved.starts_with(&cwd) || ws.path().starts_with(&cwd));
    }

    #[test]
    fn generation_runs_exactly_once_for_a_build() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("site");
        let generate = FakeGenerate::committed(committed(temp.path(), vec![], false));
        let assets = RecordingAssets::new();
        let materialize = RecordingMaterialize::returning(Ok(PublishedSite::new_for_test(
            output.clone(),
            2,
            None,
        )));
        run_build_with(
            &build_options(&output, None),
            &clock(),
            &generate,
            &assets,
            &materialize,
        )
        .expect("build succeeds");
        assert_eq!(generate.calls.get(), 1, "generation must run once");
    }
}
