//! What the OS watcher is asked to register, and over which [`WatchSet`].
//!
//! # This is not the security boundary
//!
//! Validation here is **lexical only** — the same rule [`WatchSet::classify`]
//! uses. It rejects a registration whose *text* escapes the workspace or names an
//! always-ignored output directory, and that is all it can do: it never touches
//! the disk, never canonicalizes, and cannot see through a symlink.
//!
//! **`cratevista-core` must supply a security-validated plan.** Before building
//! one it is responsible for:
//!
//! - resolving the **canonical workspace root**, and refusing to watch at all if
//!   it cannot;
//! - **canonicalizing every registration that exists** and refusing any whose
//!   resolved path falls outside that canonical root — `<root>/link/src` is
//!   lexically innocent no matter where `link` points, and only resolving it
//!   reveals an escape;
//! - representing a **missing referenced file by its nearest existing in-workspace
//!   parent**, because a path that does not exist cannot be registered — the
//!   parent directory is what reports its creation.
//!
//! This layer does none of those three. It is the last cheap check, not the first
//! real one.

use std::path::PathBuf;

use crate::classify::{Classification, IgnoreReason, WatchSet};

/// Whether a registration covers a directory's subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegistrationMode {
    /// The path itself and its direct children only.
    ///
    /// Config discovery is non-recursive, so `.cratevista/flows` is registered
    /// this way: a TOML in a subdirectory is never loaded and must not
    /// regenerate.
    NonRecursive,
    /// The path and everything beneath it.
    ///
    /// Rust source roots need this: a new module can appear in a new
    /// subdirectory, and only a recursive watch reports it.
    Recursive,
}

/// One path to hand the OS watcher.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WatchRegistration {
    /// The absolute path to register.
    pub path: PathBuf,
    /// How deep to watch.
    pub mode: RegistrationMode,
}

impl WatchRegistration {
    /// A non-recursive registration.
    pub fn non_recursive(path: impl Into<PathBuf>) -> Self {
        WatchRegistration {
            path: path.into(),
            mode: RegistrationMode::NonRecursive,
        }
    }

    /// A recursive registration.
    pub fn recursive(path: impl Into<PathBuf>) -> Self {
        WatchRegistration {
            path: path.into(),
            mode: RegistrationMode::Recursive,
        }
    }
}

/// Why a plan was refused.
///
/// Carries a workspace-relative label, never an absolute path: a plan error can
/// end up in a log, and the workspace root is a user's home directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// A registration's text falls outside the workspace root.
    OutsideWorkspace {
        /// The offending registration, as given (never resolved).
        label: String,
    },
    /// A registration names a location that is always ignored.
    ///
    /// Registering inside `target/` is the one that matters: it is where our own
    /// output lands, and watching it is how a regeneration loop starts.
    IgnoredLocation {
        /// The offending registration, workspace-relative.
        label: String,
        /// Why it is ignored.
        reason: IgnoreReason,
    },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::OutsideWorkspace { label } => {
                write!(formatter, "`{label}` is outside the workspace")
            }
            PlanError::IgnoredLocation { label, reason } => write!(
                formatter,
                "`{label}` is in an always-ignored location ({reason:?})"
            ),
        }
    }
}

impl std::error::Error for PlanError {}

/// A [`WatchSet`] plus the registrations that will make its inputs observable.
///
/// The two are one unit deliberately: a registration list without the set it was
/// built for would classify events against the wrong rules.
#[derive(Debug, Clone)]
pub struct WatchPlan {
    watch_set: WatchSet,
    registrations: Vec<WatchRegistration>,
}

impl WatchPlan {
    /// Validates and builds a plan.
    ///
    /// Registrations are **sorted and deduplicated**, so the same inputs always
    /// produce the same plan and a path listed twice is registered once.
    pub fn new(
        watch_set: WatchSet,
        registrations: impl IntoIterator<Item = WatchRegistration>,
    ) -> Result<Self, PlanError> {
        let mut checked: Vec<WatchRegistration> = Vec::new();
        for registration in registrations {
            match watch_set.classify(&registration.path) {
                Classification::Outside => {
                    return Err(PlanError::OutsideWorkspace {
                        label: relative_label(&watch_set, &registration.path),
                    });
                }
                Classification::Ignored(reason) => {
                    return Err(PlanError::IgnoredLocation {
                        label: relative_label(&watch_set, &registration.path),
                        reason,
                    });
                }
                // A directory is `NotAnInput` (only files are inputs) and an exact
                // file input is `Relevant`; both are legitimate things to register.
                Classification::NotAnInput | Classification::Relevant(_) => {
                    checked.push(registration)
                }
            }
        }
        checked.sort();
        checked.dedup();
        Ok(WatchPlan {
            watch_set,
            registrations: checked,
        })
    }

    /// The set every event is classified against.
    pub fn watch_set(&self) -> &WatchSet {
        &self.watch_set
    }

    /// The sorted, deduplicated registrations.
    pub fn registrations(&self) -> &[WatchRegistration] {
        &self.registrations
    }

    /// Consumes the plan into its parts.
    pub fn into_parts(self) -> (WatchSet, Vec<WatchRegistration>) {
        (self.watch_set, self.registrations)
    }
}

/// A path rendered relative to the workspace root, for a message that may be
/// logged. Falls back to a placeholder rather than leaking an absolute path.
fn relative_label(watch_set: &WatchSet, path: &std::path::Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    match text.strip_prefix(watch_set.root()) {
        Some(rest) => rest.trim_start_matches('/').to_string(),
        None => "<outside the workspace>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::WatchInput;
    use std::path::Path;

    fn watch_set() -> WatchSet {
        WatchSet::new(
            Path::new("/w"),
            [
                WatchInput::file("/w/Cargo.toml"),
                WatchInput::rust_root("/w/src"),
                WatchInput::flows_dir("/w/.cratevista/flows"),
            ],
        )
    }

    #[test]
    fn a_plan_sorts_and_deduplicates_its_registrations() {
        let plan = WatchPlan::new(
            watch_set(),
            [
                WatchRegistration::recursive("/w/src"),
                WatchRegistration::non_recursive("/w/.cratevista/flows"),
                WatchRegistration::recursive("/w/src"),
                WatchRegistration::non_recursive("/w/.cratevista/flows"),
            ],
        )
        .expect("a valid plan");

        assert_eq!(
            plan.registrations(),
            [
                WatchRegistration::non_recursive("/w/.cratevista/flows"),
                WatchRegistration::recursive("/w/src"),
            ]
        );
    }

    #[test]
    fn one_path_registered_both_ways_keeps_both() {
        // Different modes are different registrations: dedup is on the pair.
        let plan = WatchPlan::new(
            watch_set(),
            [
                WatchRegistration::recursive("/w/src"),
                WatchRegistration::non_recursive("/w/src"),
            ],
        )
        .expect("a valid plan");
        assert_eq!(plan.registrations().len(), 2);
    }

    #[test]
    fn an_exact_file_registration_is_accepted() {
        let plan = WatchPlan::new(
            watch_set(),
            [WatchRegistration::non_recursive("/w/Cargo.toml")],
        )
        .expect("watching a manifest directly is legitimate");
        assert_eq!(plan.registrations().len(), 1);
    }

    #[test]
    fn a_registration_outside_the_workspace_is_refused() {
        let error = WatchPlan::new(
            watch_set(),
            [WatchRegistration::recursive("/elsewhere/src")],
        )
        .expect_err("must be refused");
        assert_eq!(
            error,
            PlanError::OutsideWorkspace {
                label: "<outside the workspace>".into()
            }
        );
    }

    #[test]
    fn a_traversing_registration_is_refused() {
        let error = WatchPlan::new(watch_set(), [WatchRegistration::recursive("/w/../secrets")])
            .expect_err("must be refused");
        assert!(matches!(error, PlanError::OutsideWorkspace { .. }));
    }

    #[test]
    fn registering_inside_our_own_output_is_refused() {
        // The loop guard, enforced before a watcher is ever built: this is the
        // registration that would make our own writes retrigger generation.
        let error = WatchPlan::new(
            watch_set(),
            [WatchRegistration::recursive("/w/target/cratevista")],
        )
        .expect_err("must be refused");
        assert_eq!(
            error,
            PlanError::IgnoredLocation {
                label: "target/cratevista".into(),
                reason: IgnoreReason::GeneratedOutput,
            }
        );
    }

    #[test]
    fn registering_other_always_ignored_locations_is_refused() {
        for path in [
            "/w/target",
            "/w/.git",
            "/w/.git/objects",
            "/w/web/node_modules",
            "/w/web/dist",
            // A hidden directory *with something under it* is refused; a bare
            // trailing one cannot be — see the test below.
            "/w/.idea/caches",
        ] {
            let error = WatchPlan::new(watch_set(), [WatchRegistration::recursive(path)])
                .expect_err("must be refused");
            assert!(
                matches!(error, PlanError::IgnoredLocation { .. }),
                "{path} must be refused"
            );
        }
    }

    #[test]
    fn a_bare_hidden_directory_registration_is_accepted_but_its_contents_stay_ignored() {
        // An honest limitation of a lexical check: `/w/.idea` could equally be a
        // hidden *file*, and nothing here may touch the disk to find out — so the
        // hidden-directory rule (which needs a component *after* the dot) cannot
        // fire on a final component. Registering it is therefore accepted.
        //
        // It is wasteful, not wrong: every event underneath still classifies as
        // ignored, so no regeneration can come of it. Core builds plans from
        // metadata and config and never names `.idea`; if that changed, the cost
        // is a redundant watch, not a stale or looping document.
        let plan = WatchPlan::new(watch_set(), [WatchRegistration::recursive("/w/.idea")])
            .expect("lexically indistinguishable from a hidden file");
        assert_eq!(plan.registrations().len(), 1);

        assert_eq!(
            plan.watch_set()
                .classify(Path::new("/w/.idea/workspace.xml")),
            Classification::Ignored(IgnoreReason::HiddenDirectory),
            "its contents are still ignored, so the watch can produce nothing"
        );
    }

    #[test]
    fn a_plan_error_never_carries_an_absolute_path() {
        let error = WatchPlan::new(watch_set(), [WatchRegistration::recursive("/w/target")])
            .expect_err("must be refused");
        let rendered = error.to_string();
        assert!(!rendered.contains("/w/"), "leaked the root: {rendered}");
        assert_eq!(
            rendered,
            "`target` is in an always-ignored location (GeneratedOutput)"
        );
    }

    #[test]
    fn the_plan_keeps_its_watch_set() {
        let plan = WatchPlan::new(watch_set(), [WatchRegistration::recursive("/w/src")]).unwrap();
        assert!(plan.watch_set().is_relevant(Path::new("/w/src/lib.rs")));
        let (set, registrations) = plan.into_parts();
        assert_eq!(set.root(), "/w");
        assert_eq!(registrations.len(), 1);
    }
}
