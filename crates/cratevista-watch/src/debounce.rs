//! Coalescing a burst of filesystem events into one regeneration request.
//!
//! # A state machine, not a timer
//!
//! Nothing here sleeps or reads a clock. Every method takes the caller's
//! **monotonic timestamp**, and [`Debouncer::deadline`] reports when the caller
//! should next look. The runtime that owns a real timer lives outside this crate.
//!
//! That is what makes the behavior testable *exactly*: a test drives time by
//! hand, so "fires at the maximum delay" is an assertion about the state machine
//! rather than a race against a scheduler. There is no sleep to tune and nothing
//! to flake on a loaded CI machine.
//!
//! # The two deadlines
//!
//! A burst has both a **quiet** deadline and a **maximum** deadline, and fires at
//! whichever comes first:
//!
//! - **quiet** (`last event + quiet`) — resets on every event, so a burst of
//!   related writes collapses into one regeneration.
//! - **maximum** (`first event + max_delay`) — anchored to the burst's *first*
//!   event and never moves. Without it, a continuous stream (`cargo fmt` across a
//!   large tree, a rebase, a `git checkout`) would reset the quiet window forever
//!   and starve regeneration entirely.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::classify::WatchSet;

/// The debounce timings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebounceOptions {
    /// How long the burst must be quiet before firing.
    pub quiet: Duration,
    /// The longest a burst may be held, measured from its first event.
    pub max_delay: Duration,
}

/// 300 ms — long enough to swallow an editor's save sequence, short enough to
/// feel immediate.
pub const DEFAULT_QUIET: Duration = Duration::from_millis(300);
/// 2 s — the ceiling on how long a continuous stream can hold regeneration off.
pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(2);

impl Default for DebounceOptions {
    fn default() -> Self {
        DebounceOptions {
            quiet: DEFAULT_QUIET,
            max_delay: DEFAULT_MAX_DELAY,
        }
    }
}

/// The burst currently being collected.
#[derive(Debug, Clone)]
struct Burst {
    /// When the burst started — anchors the maximum deadline.
    first_at: Duration,
    /// When the most recent event arrived — anchors the quiet deadline.
    last_at: Duration,
    /// The paths seen so far. A set, not a queue: create/modify/remove/rename of
    /// one path is one changed path, however many events the OS emitted.
    paths: BTreeSet<PathBuf>,
}

/// Coalesces relevant events into one batch per burst.
///
/// Timestamps must be **monotonic and non-decreasing**; a timestamp older than
/// the burst's last event is clamped forward, so a caller that misorders two
/// events can only ever fire early, never hang.
#[derive(Debug, Clone)]
pub struct Debouncer {
    options: DebounceOptions,
    burst: Option<Burst>,
}

impl Default for Debouncer {
    fn default() -> Self {
        Debouncer::new(DebounceOptions::default())
    }
}

impl Debouncer {
    /// A debouncer with the given timings.
    pub fn new(options: DebounceOptions) -> Self {
        Debouncer {
            options,
            burst: None,
        }
    }

    /// The configured timings.
    pub fn options(&self) -> DebounceOptions {
        self.options
    }

    /// Whether no burst is in progress.
    pub fn is_idle(&self) -> bool {
        self.burst.is_none()
    }

    /// How many distinct paths the current burst holds.
    pub fn pending(&self) -> usize {
        self.burst.as_ref().map_or(0, |burst| burst.paths.len())
    }

    /// Records a relevant event.
    ///
    /// The first event starts **both** deadlines; each later one resets only the
    /// quiet deadline, leaving the maximum anchored to the first.
    pub fn record(&mut self, at: Duration, path: impl Into<PathBuf>) {
        match &mut self.burst {
            None => {
                let mut paths = BTreeSet::new();
                paths.insert(path.into());
                self.burst = Some(Burst {
                    first_at: at,
                    last_at: at,
                    paths,
                });
            }
            Some(burst) => {
                // Clamp: a non-monotonic caller must not be able to drag the
                // quiet deadline backwards and stall the burst.
                burst.last_at = burst.last_at.max(at);
                burst.paths.insert(path.into());
            }
        }
    }

    /// Classifies `path` and records it only if it is relevant.
    ///
    /// Returns whether it was recorded. **An irrelevant event never starts or
    /// extends a burst** — it cannot move either deadline, because it never
    /// reaches [`record`](Debouncer::record).
    pub fn record_if_relevant(&mut self, set: &WatchSet, at: Duration, path: &Path) -> bool {
        if !set.is_relevant(path) {
            return false;
        }
        self.record(at, path.to_path_buf());
        true
    }

    /// When the current burst is due to fire, if one is in progress.
    ///
    /// The earlier of the quiet and maximum deadlines.
    pub fn deadline(&self) -> Option<Duration> {
        self.burst.as_ref().map(|burst| {
            let quiet = burst.last_at + self.options.quiet;
            let maximum = burst.first_at + self.options.max_delay;
            quiet.min(maximum)
        })
    }

    /// Fires if `now` has reached the deadline, returning the burst's paths —
    /// **sorted and deduplicated** — and resetting to idle.
    ///
    /// The boundary is **inclusive**: `now == deadline` fires. Defining it here
    /// rather than leaving it to a timer's rounding is what makes the behavior
    /// testable at the exact tick instead of "some time around then".
    pub fn poll(&mut self, now: Duration) -> Option<Vec<PathBuf>> {
        let deadline = self.deadline()?;
        if now < deadline {
            return None;
        }
        // Complete reset: the next event starts a genuinely new burst, with both
        // deadlines re-anchored to it.
        let burst = self.burst.take()?;
        Some(burst.paths.into_iter().collect())
    }

    /// Drops the current burst without firing.
    pub fn reset(&mut self) {
        self.burst = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::WatchInput;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn debouncer() -> Debouncer {
        Debouncer::default()
    }

    #[test]
    fn the_defaults_are_300ms_quiet_and_2s_max() {
        let options = DebounceOptions::default();
        assert_eq!(options.quiet, ms(300));
        assert_eq!(options.max_delay, ms(2000));
        assert_eq!(debouncer().options(), options);
    }

    #[test]
    fn an_idle_debouncer_has_no_deadline_and_never_fires() {
        let mut debouncer = debouncer();
        assert!(debouncer.is_idle());
        assert_eq!(debouncer.deadline(), None);
        assert_eq!(debouncer.poll(ms(10_000)), None);
        assert_eq!(debouncer.pending(), 0);
    }

    #[test]
    fn the_first_event_starts_both_deadlines() {
        let mut debouncer = debouncer();
        debouncer.record(ms(1_000), "/w/a.rs");
        // quiet = 1000+300 = 1300; max = 1000+2000 = 3000; earlier wins.
        assert_eq!(debouncer.deadline(), Some(ms(1_300)));
        assert!(!debouncer.is_idle());
    }

    #[test]
    fn a_quiet_window_fires_one_batch() {
        let mut debouncer = debouncer();
        debouncer.record(ms(1_000), "/w/a.rs");
        assert_eq!(debouncer.poll(ms(1_299)), None, "still inside the window");
        assert_eq!(
            debouncer.poll(ms(1_300)),
            Some(vec![PathBuf::from("/w/a.rs")])
        );
        assert!(debouncer.is_idle(), "firing resets completely");
    }

    #[test]
    fn a_later_event_resets_only_the_quiet_deadline() {
        let mut debouncer = debouncer();
        debouncer.record(ms(1_000), "/w/a.rs");
        debouncer.record(ms(1_200), "/w/b.rs");
        // Quiet moved to 1200+300; max is still anchored to the FIRST event.
        assert_eq!(debouncer.deadline(), Some(ms(1_500)));
        assert_eq!(debouncer.poll(ms(1_499)), None);
        assert_eq!(
            debouncer.poll(ms(1_500)),
            Some(vec![PathBuf::from("/w/a.rs"), PathBuf::from("/w/b.rs")])
        );
    }

    #[test]
    fn the_maximum_deadline_stays_anchored_to_the_first_event() {
        let mut debouncer = debouncer();
        debouncer.record(ms(0), "/w/a.rs");
        // A steady stream, each event 100 ms apart: the quiet window never
        // elapses, so only the maximum can end this burst.
        for step in 1..=25 {
            let now = ms(step * 100);
            debouncer.record(now, format!("/w/file{step}.rs"));
            if now < ms(2_000) {
                assert_eq!(
                    debouncer.poll(now),
                    None,
                    "must not fire before the max at {now:?}"
                );
            }
        }
        // Anchored to the first event at 0 → fires at exactly 2000.
        assert_eq!(debouncer.deadline(), Some(ms(2_000)));
        let fired = debouncer.poll(ms(2_000)).expect("the max delay must fire");
        assert_eq!(fired.len(), 26, "every path in the burst, once");
    }

    #[test]
    fn a_continuous_stream_cannot_starve_regeneration() {
        // The same property stated as the risk it exists to prevent. The stream
        // must run *past* the maximum deadline: events 10 ms apart never let the
        // 300 ms quiet window elapse, so without the maximum this burst would
        // never fire at all.
        let mut debouncer = debouncer();
        debouncer.record(ms(0), "/w/a.rs");
        for step in 1..=200 {
            let now = ms(step * 10); // 10 ms … 2000 ms
            debouncer.record(now, "/w/a.rs");
            if now < ms(2_000) {
                assert_eq!(
                    debouncer.poll(now),
                    None,
                    "the quiet window has not elapsed and the max is not reached at {now:?}"
                );
            }
        }
        // Quiet would say 2000+300; the maximum, anchored at 0, binds first.
        assert_eq!(debouncer.deadline(), Some(ms(2_000)));
        assert!(debouncer.poll(ms(2_000)).is_some());
    }

    #[test]
    fn firing_is_inclusive_at_the_exact_quiet_deadline() {
        let mut debouncer = debouncer();
        debouncer.record(ms(500), "/w/a.rs");
        let deadline = debouncer.deadline().unwrap();
        assert_eq!(deadline, ms(800));

        // One nanosecond early: no.
        assert_eq!(debouncer.poll(deadline - Duration::from_nanos(1)), None);
        // Exactly on it: yes.
        assert!(debouncer.poll(deadline).is_some());
    }

    #[test]
    fn firing_is_inclusive_at_the_exact_maximum_deadline() {
        let mut debouncer = debouncer();
        debouncer.record(ms(0), "/w/a.rs");
        // Keep the quiet window alive so the maximum is the binding deadline.
        debouncer.record(ms(1_900), "/w/b.rs");
        let deadline = debouncer.deadline().unwrap();
        assert_eq!(deadline, ms(2_000), "max (0+2000) beats quiet (1900+300)");

        assert_eq!(debouncer.poll(deadline - Duration::from_nanos(1)), None);
        assert!(debouncer.poll(deadline).is_some());
    }

    #[test]
    fn polling_well_past_the_deadline_still_fires_once() {
        let mut debouncer = debouncer();
        debouncer.record(ms(0), "/w/a.rs");
        assert!(
            debouncer.poll(ms(60_000)).is_some(),
            "a late poll still fires"
        );
        assert_eq!(debouncer.poll(ms(60_001)), None, "and only once");
    }

    #[test]
    fn a_burst_coalesces_create_modify_remove_and_rename_into_one_set() {
        let mut debouncer = debouncer();
        // What one editor save actually produces: the same file, several kinds.
        debouncer.record(ms(0), "/w/src/lib.rs"); // create (temp renamed in)
        debouncer.record(ms(10), "/w/src/lib.rs"); // modify
        debouncer.record(ms(20), "/w/src/lib.rs"); // remove
        debouncer.record(ms(30), "/w/src/lib.rs"); // rename back
        debouncer.record(ms(40), "/w/src/other.rs");

        assert_eq!(debouncer.pending(), 2, "one entry per path, not per event");
        let fired = debouncer.poll(ms(340)).unwrap();
        assert_eq!(
            fired,
            [
                PathBuf::from("/w/src/lib.rs"),
                PathBuf::from("/w/src/other.rs")
            ]
        );
    }

    #[test]
    fn fired_paths_are_sorted_and_deduplicated_regardless_of_arrival_order() {
        let mut debouncer = debouncer();
        for (index, path) in ["/w/z.rs", "/w/a.rs", "/w/m.rs", "/w/a.rs", "/w/z.rs"]
            .iter()
            .enumerate()
        {
            debouncer.record(ms(index as u64), *path);
        }
        assert_eq!(
            debouncer.poll(ms(1_000)).unwrap(),
            [
                PathBuf::from("/w/a.rs"),
                PathBuf::from("/w/m.rs"),
                PathBuf::from("/w/z.rs")
            ]
        );
    }

    #[test]
    fn two_bursts_separated_by_a_quiet_interval_fire_independently() {
        let mut debouncer = debouncer();

        debouncer.record(ms(0), "/w/first.rs");
        assert_eq!(
            debouncer.poll(ms(300)).unwrap(),
            [PathBuf::from("/w/first.rs")]
        );
        assert!(debouncer.is_idle());

        // A completely new burst: both deadlines re-anchor to its first event,
        // and nothing from the previous burst leaks in.
        debouncer.record(ms(5_000), "/w/second.rs");
        assert_eq!(debouncer.deadline(), Some(ms(5_300)));
        assert_eq!(
            debouncer.poll(ms(5_300)).unwrap(),
            [PathBuf::from("/w/second.rs")]
        );
        assert!(debouncer.is_idle());
    }

    #[test]
    fn firing_resets_the_maximum_anchor_too() {
        // A second burst must get a full max window, not the remainder of the
        // first one's.
        let mut debouncer = debouncer();
        debouncer.record(ms(0), "/w/a.rs");
        debouncer.poll(ms(2_000)).unwrap();

        debouncer.record(ms(2_100), "/w/b.rs");
        debouncer.record(ms(2_200), "/w/c.rs");
        // max = 2100+2000 = 4100, quiet = 2200+300 = 2500 → quiet binds.
        assert_eq!(debouncer.deadline(), Some(ms(2_500)));
    }

    #[test]
    fn reset_drops_the_burst_without_firing() {
        let mut debouncer = debouncer();
        debouncer.record(ms(0), "/w/a.rs");
        debouncer.reset();
        assert!(debouncer.is_idle());
        assert_eq!(debouncer.deadline(), None);
        assert_eq!(debouncer.poll(ms(10_000)), None);
    }

    #[test]
    fn a_non_monotonic_timestamp_cannot_drag_the_quiet_deadline_backwards() {
        let mut debouncer = debouncer();
        debouncer.record(ms(1_000), "/w/a.rs");
        debouncer.record(ms(900), "/w/b.rs"); // out of order
        // Clamped forward: still 1000+300, never 900+300.
        assert_eq!(debouncer.deadline(), Some(ms(1_300)));
    }

    #[test]
    fn custom_timings_are_honored() {
        let mut debouncer = Debouncer::new(DebounceOptions {
            quiet: ms(50),
            max_delay: ms(120),
        });
        debouncer.record(ms(0), "/w/a.rs");
        assert_eq!(debouncer.deadline(), Some(ms(50)));
        debouncer.record(ms(40), "/w/b.rs");
        debouncer.record(ms(80), "/w/c.rs");
        // quiet = 130, max = 120 → the max binds.
        assert_eq!(debouncer.deadline(), Some(ms(120)));
    }

    // --- classification gating --------------------------------------------

    fn watch_set() -> WatchSet {
        WatchSet::new(
            Path::new("/w"),
            [
                WatchInput::rust_root("/w/src"),
                WatchInput::file("/w/Cargo.toml"),
            ],
        )
    }

    #[test]
    fn an_irrelevant_event_never_starts_a_burst() {
        let mut debouncer = debouncer();
        let set = watch_set();

        for path in [
            "/w/target/cratevista/document.json", // our own output
            "/w/.git/index",
            "/w/src/lib.rs.swp", // editor noise
            "/w/README.md",      // not an input
            "/elsewhere/lib.rs", // outside
        ] {
            assert!(
                !debouncer.record_if_relevant(&set, ms(0), Path::new(path)),
                "{path} must not be recorded"
            );
        }
        assert!(debouncer.is_idle(), "no burst was started");
        assert_eq!(debouncer.deadline(), None);
    }

    #[test]
    fn an_irrelevant_event_never_extends_an_existing_burst() {
        let mut debouncer = debouncer();
        let set = watch_set();

        assert!(debouncer.record_if_relevant(&set, ms(1_000), Path::new("/w/src/lib.rs")));
        let deadline = debouncer.deadline();
        assert_eq!(deadline, Some(ms(1_300)));

        // Our own generated output lands mid-burst — the loop scenario.
        assert!(!debouncer.record_if_relevant(
            &set,
            ms(1_200),
            Path::new("/w/target/cratevista/document.json")
        ));
        assert_eq!(debouncer.deadline(), deadline, "the deadline did not move");
        assert_eq!(debouncer.pending(), 1, "and nothing was added");
    }

    #[test]
    fn a_relevant_event_is_recorded_through_the_classifier() {
        let mut debouncer = debouncer();
        let set = watch_set();
        assert!(debouncer.record_if_relevant(&set, ms(0), Path::new("/w/src/lib.rs")));
        assert!(debouncer.record_if_relevant(&set, ms(10), Path::new("/w/Cargo.toml")));
        assert_eq!(
            debouncer.poll(ms(310)).unwrap(),
            [
                PathBuf::from("/w/Cargo.toml"),
                PathBuf::from("/w/src/lib.rs")
            ]
        );
    }

    #[test]
    fn the_whole_state_machine_is_deterministic_across_repeated_runs() {
        // Same inputs, same timestamps → same output, every time. No clock is
        // read, so there is nothing left to vary.
        let script = [
            (0u64, "/w/src/z.rs"),
            (50, "/w/src/a.rs"),
            (100, "/w/src/z.rs"),
            (150, "/w/Cargo.toml"),
        ];
        let run = || {
            let set = watch_set();
            let mut debouncer = debouncer();
            for (at, path) in script {
                debouncer.record_if_relevant(&set, ms(at), Path::new(path));
            }
            (debouncer.deadline(), debouncer.poll(ms(450)))
        };
        let first = run();
        assert_eq!(first.0, Some(ms(450)));
        for _ in 0..10 {
            assert_eq!(run(), first);
        }
    }
}
