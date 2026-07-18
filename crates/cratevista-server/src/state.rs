//! The replaceable application state.
//!
//! [`AppState`] holds the whole [`ArtifactSnapshot`] as a **single**
//! `ArcSwap`, so a request always sees a coherent document + generation +
//! diagnostics from one generation — never a mix. PRD 09 replaces the snapshot
//! atomically via [`AppState::replace_snapshot`] with no handler changes; PRD 06
//! itself does not watch the filesystem or reload.

use std::sync::Arc;

use arc_swap::ArcSwap;

use tokio::sync::broadcast;

use crate::events::{EVENT_CHANNEL_CAPACITY, ServerEvent};
use crate::options::SourceAccessPolicy;
use crate::snapshot::ArtifactSnapshot;

/// Shared, replaceable server state.
pub struct AppState {
    snapshot: ArcSwap<ArtifactSnapshot>,
    source: SourceAccessPolicy,
    watch_enabled: bool,
    /// The generation-event fan-out. Bounded; see [`EVENT_CHANNEL_CAPACITY`].
    ///
    /// Held even when watching is off — an unused sender costs a pointer, and a
    /// state that could not publish would make the two constructors differ in
    /// more than the one thing they mean to differ in. The **route** is what is
    /// conditional, not the channel.
    events: broadcast::Sender<ServerEvent>,
}

impl AppState {
    /// Builds shared state from an initial snapshot and a source-access policy.
    ///
    /// Watch mode is **off**: this is what `serve` and a plain `open` use, and a
    /// server that is not watching has no event stream to advertise. Callers that
    /// do watch use [`AppState::new_watching`].
    pub fn new(snapshot: ArtifactSnapshot, source: SourceAccessPolicy) -> Arc<Self> {
        Self::with_watch(snapshot, source, false)
    }

    /// Builds shared state that advertises watch mode via `/api/health`.
    ///
    /// Separate from [`AppState::new`] rather than a parameter on it so every
    /// existing caller keeps compiling and keeps its current meaning — the
    /// non-watching default is the one that must never be set by accident.
    pub fn new_watching(snapshot: ArtifactSnapshot, source: SourceAccessPolicy) -> Arc<Self> {
        Self::with_watch(snapshot, source, true)
    }

    fn with_watch(
        snapshot: ArtifactSnapshot,
        source: SourceAccessPolicy,
        watch_enabled: bool,
    ) -> Arc<Self> {
        // Each state owns its own channel, so callers never pass one in and two
        // servers in one process can never cross-publish.
        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Arc::new(AppState {
            snapshot: ArcSwap::from_pointee(snapshot),
            source,
            watch_enabled,
            events,
        })
    }

    /// Loads the current snapshot. A handler calls this **once** per request and
    /// uses the returned value for everything that request needs, so it never
    /// mixes generations even if a swap happens mid-request.
    pub fn snapshot(&self) -> Arc<ArtifactSnapshot> {
        self.snapshot.load_full()
    }

    /// Atomically replaces the snapshot (the PRD-09 live-reload seam).
    pub fn replace_snapshot(&self, snapshot: ArtifactSnapshot) {
        self.snapshot.store(Arc::new(snapshot));
    }

    /// The source-access policy for `/api/source`.
    pub fn source_policy(&self) -> &SourceAccessPolicy {
        &self.source
    }

    /// Subscribes to generation events.
    ///
    /// Each subscriber gets its own cursor over the last
    /// [`EVENT_CHANNEL_CAPACITY`] events. A subscriber that falls behind lags and
    /// its stream ends; it cannot slow or block any other subscriber, and it
    /// cannot grow the server's memory.
    pub fn subscribe_events(&self) -> broadcast::Receiver<ServerEvent> {
        self.events.subscribe()
    }

    /// Publishes one event to every current subscriber, in publication order.
    ///
    /// **Harmless with no subscribers**: `broadcast::send` returns `Err` when
    /// nobody is listening, and that is not a failure — a server nobody has opened
    /// yet still regenerates. The result is deliberately discarded rather than
    /// surfaced, because there is nothing a caller could sensibly do about it.
    ///
    /// The server never *decides* to publish: core does, and this only fans out.
    pub fn publish_event(&self, event: ServerEvent) {
        let _ = self.events.send(event);
    }

    /// Whether this server is watching and will publish generation events.
    ///
    /// Reported by `/api/health` so a client can decide whether to subscribe.
    /// It is fixed for the process's lifetime: watching is chosen at startup, so
    /// there is nothing to swap and no reason for this to be atomic.
    pub fn watch_enabled(&self) -> bool {
        self.watch_enabled
    }
}
