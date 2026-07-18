//! A signal-agnostic graceful-shutdown trigger/observer pair.
//!
//! `cratevista-server` never installs OS signal handlers (that is a
//! `cratevista-core` concern); instead it exposes a [`ShutdownHandle`] that can
//! be triggered from anywhere — a Ctrl-C handler in core, or a test — and a
//! [`ShutdownSignal`] that [`crate::run`] awaits. This keeps the server testable
//! without sending real signals.

use std::sync::Arc;

use tokio::sync::watch;

/// A cloneable trigger. Calling [`ShutdownHandle::trigger`] requests graceful
/// shutdown of every server awaiting the paired [`ShutdownSignal`].
#[derive(Clone, Debug)]
pub struct ShutdownHandle(Arc<watch::Sender<bool>>);

impl ShutdownHandle {
    /// Requests graceful shutdown. Idempotent; safe to call more than once.
    pub fn trigger(&self) {
        let _ = self.0.send(true);
    }
}

/// The observer awaited by [`crate::run`]; resolves once shutdown is requested.
#[derive(Clone)]
pub struct ShutdownSignal(watch::Receiver<bool>);

impl ShutdownSignal {
    /// Resolves when shutdown has been requested (immediately if it already was).
    pub async fn wait(mut self) {
        if *self.0.borrow() {
            return;
        }
        while self.0.changed().await.is_ok() {
            if *self.0.borrow() {
                return;
            }
        }
        // Sender dropped without triggering: treat as shutdown so the server exits.
    }
}

/// Creates a linked [`ShutdownHandle`] / [`ShutdownSignal`] pair.
pub fn shutdown_channel() -> (ShutdownHandle, ShutdownSignal) {
    let (tx, rx) = watch::channel(false);
    (ShutdownHandle(Arc::new(tx)), ShutdownSignal(rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wait_resolves_after_trigger() {
        let (handle, signal) = shutdown_channel();
        let task = tokio::spawn(signal.wait());
        handle.trigger();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn wait_returns_immediately_if_already_triggered() {
        let (handle, signal) = shutdown_channel();
        handle.trigger();
        // Should not hang.
        signal.wait().await;
    }

    #[tokio::test]
    async fn handle_is_cloneable_and_any_clone_triggers() {
        let (handle, signal) = shutdown_channel();
        let clone = handle.clone();
        drop(handle);
        let task = tokio::spawn(signal.wait());
        clone.trigger();
        task.await.unwrap();
    }
}
