//! What the engine tells the outside world about a regeneration.
//!
//! # Deliberately narrow
//!
//! Three variants, and **none of them carries a changed path**. That is not an
//! oversight: these events are designed to be forwardable to a browser, and the
//! paths that triggered a run are absolute paths on someone's machine. Keeping
//! them out of the type means a later consumer *cannot* leak them by forwarding
//! an event, rather than merely being asked not to.
//!
//! Likewise, the engine knows nothing about HTTP, SSE or JSON. This is a plain
//! Rust enum; whoever serves it decides how it looks on the wire.

/// A step in one regeneration's lifecycle.
///
/// Every run emits exactly one [`GenerationStarted`](EngineEvent::GenerationStarted)
/// followed by exactly one terminal event — either
/// [`GenerationSucceeded`](EngineEvent::GenerationSucceeded) or
/// [`GenerationFailed`](EngineEvent::GenerationFailed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    /// A regeneration has begun. Emitted **before** the operation is entered.
    GenerationStarted,
    /// The regeneration operation returned success.
    ///
    /// In the later core adapter, reaching this means generate → verify → swap →
    /// WatchSet rebuild all completed: the new state is already live by the time
    /// this is observed, so a client refetching on it cannot see the old one.
    GenerationSucceeded {
        /// `generation.partial` — a valid snapshot that skipped a failed target.
        partial: bool,
    },
    /// The regeneration operation returned failure. The previous state stands.
    GenerationFailed {
        /// A stable, machine-matchable code, supplied by the caller.
        code: String,
        /// A human-readable message, supplied by the caller.
        ///
        /// The engine transports this **unchanged**. It performs no sanitization
        /// of any kind — see [`RegenerationFailure`](crate::RegenerationFailure)
        /// for whose job that is.
        message: String,
    },
}

impl EngineEvent {
    /// Whether this ends a run (either outcome).
    ///
    /// Useful to assert the "terminal before the next `Started`" ordering without
    /// caring which outcome occurred.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            EngineEvent::GenerationSucceeded { .. } | EngineEvent::GenerationFailed { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn started_is_not_terminal_and_both_outcomes_are() {
        assert!(!EngineEvent::GenerationStarted.is_terminal());
        assert!(EngineEvent::GenerationSucceeded { partial: false }.is_terminal());
        assert!(EngineEvent::GenerationSucceeded { partial: true }.is_terminal());
        assert!(
            EngineEvent::GenerationFailed {
                code: "x".into(),
                message: "y".into(),
            }
            .is_terminal()
        );
    }
}
