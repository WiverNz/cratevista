//! Generation events and the `GET /api/events` SSE stream.
//!
//! # State notifications, not a log
//!
//! An event means "something happened to the current snapshot"; the truth is
//! always whatever `/api/document` returns **now**. That is why this stream
//! **never emits an SSE `id:` field**: with no id, a browser has nothing to put
//! in `Last-Event-ID`, and **replay is impossible by construction** rather than
//! by policy. A replayed "succeeded" from three snapshots ago would be worse than
//! replaying nothing. A `Last-Event-ID` request header is ignored — not an error,
//! just nothing to honor.
//!
//! # What this module does not do
//!
//! It does not infer anything. The server never decides that a generation
//! started, succeeded or failed: it publishes what it is told and renders it.
//! Deciding — and the whole `generate → verify → swap → publish` sequence — is
//! `cratevista-core`'s job in a later phase, as is converting that crate's
//! `EngineEvent` into a [`ServerEvent`]. **This crate does not depend on
//! `cratevista-watch`**, and must not: reusing one enum is not worth an
//! architectural edge from the server to the watcher.
//!
//! # Nothing here carries a path
//!
//! No variant has a field for one. The paths that trigger a regeneration are
//! absolute paths on someone's machine, and this stream is read by a browser, so
//! the type is shaped to make leaking one impossible rather than merely
//! discouraged. `code`/`message` arrive already safe from core and are
//! transported **unchanged** — no debug formatting, no child-process stderr, no
//! internal error text is added here.

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_core::Stream;
use tokio::sync::broadcast;

use crate::state::AppState;

/// How many events the broadcast channel holds.
///
/// **Bounded on purpose.** A client that stops reading must not be able to grow
/// the server's memory: at 16 it lags, its stream ends, and its `EventSource`
/// reconnects and refetches. Unbounded buffering for a client that may never
/// return is how a local dev server leaks.
pub const EVENT_CHANNEL_CAPACITY: usize = 16;

/// How often an idle stream emits a keepalive comment.
///
/// Idle proxies and browsers drop a silent connection; a comment costs two bytes
/// and is not an event.
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// The reconnect hint sent once, before any event.
pub const RETRY_INTERVAL: Duration = Duration::from_millis(1000);

/// Something that happened to the served snapshot.
///
/// Exactly three variants, and **none carries a path**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// A regeneration began.
    GenerationStarted,
    /// A new snapshot **is already live**; refetch now.
    GenerationSucceeded {
        /// `generation.partial` — valid, but a target was skipped.
        partial: bool,
    },
    /// A regeneration failed. The previous snapshot is still live.
    GenerationFailed {
        /// A stable, machine-matchable code, supplied by core.
        code: String,
        /// A message core has already made safe to publish.
        message: String,
    },
}

impl ServerEvent {
    /// The SSE event name. These three strings are the whole vocabulary.
    pub fn name(&self) -> &'static str {
        match self {
            ServerEvent::GenerationStarted => "generation-started",
            ServerEvent::GenerationSucceeded { .. } => "generation-succeeded",
            ServerEvent::GenerationFailed { .. } => "generation-failed",
        }
    }

    /// The compact JSON payload.
    ///
    /// Built explicitly per variant rather than derived, so the wire format is
    /// visible here and cannot drift when the enum gains a field.
    pub fn data(&self) -> String {
        match self {
            ServerEvent::GenerationStarted => "{}".to_string(),
            ServerEvent::GenerationSucceeded { partial } => {
                serde_json::json!({ "partial": partial }).to_string()
            }
            ServerEvent::GenerationFailed { code, message } => {
                serde_json::json!({ "code": code, "message": message }).to_string()
            }
        }
    }

    /// Renders this event as an SSE frame. Never sets `id`.
    fn to_sse(&self) -> Event {
        Event::default().event(self.name()).data(self.data())
    }
}

/// Stream timings. Only the keepalive is adjustable, and only within this crate,
/// so a test need not wait 15 seconds to prove a comment arrives.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SseOptions {
    pub(crate) keepalive: Duration,
}

impl Default for SseOptions {
    fn default() -> Self {
        SseOptions {
            keepalive: KEEPALIVE_INTERVAL,
        }
    }
}

/// `GET /api/events` — the generation event stream.
///
/// **Registered only for a watch-enabled state.** On an ordinary `serve` the
/// route does not exist and the router's fallback answers with the usual
/// unknown-API JSON `404`; `/api/health.watch_enabled` is the capability source a
/// client is expected to consult first.
pub async fn events(State(state): State<Arc<AppState>>) -> Response {
    sse_response(state.subscribe_events(), SseOptions::default())
}

/// Builds the SSE response from a subscription.
pub(crate) fn sse_response(
    receiver: broadcast::Receiver<ServerEvent>,
    options: SseOptions,
) -> Response {
    // `Cache-Control: no-store` matches the artifact routes; the security headers
    // (CSP, nosniff, …) are applied globally by the router layer, so a stream is
    // not a hole in them. The CSP already permits this: `connect-src 'self'`.
    let stream = EventStream::new(receiver);
    let mut response = Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(options.keepalive))
        .into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

/// A broadcast receiver rendered as an SSE stream.
///
/// Hand-written because `broadcast::Receiver` has **no poll-based API** — only an
/// `async fn recv` — so the receiver has to live inside a future between polls.
/// Boxing that future is the whole trick; there is no `unsafe` and no
/// self-reference.
struct EventStream {
    /// The in-flight `recv`, carrying the receiver so it survives each poll.
    pending: Pin<Box<dyn Future<Output = ReceiveOutcome> + Send>>,
    /// The one-time `retry:` frame, emitted before anything is received.
    retry: Option<Event>,
    /// Set once the stream has ended; polling a finished broadcast again would
    /// otherwise loop on the same terminal condition.
    finished: bool,
}

/// One `recv`, plus the receiver handed back for the next one.
type ReceiveOutcome = (
    Result<ServerEvent, broadcast::error::RecvError>,
    broadcast::Receiver<ServerEvent>,
);

async fn receive(mut receiver: broadcast::Receiver<ServerEvent>) -> ReceiveOutcome {
    let result = receiver.recv().await;
    (result, receiver)
}

impl EventStream {
    fn new(receiver: broadcast::Receiver<ServerEvent>) -> Self {
        EventStream {
            pending: Box::pin(receive(receiver)),
            // Sent before any event, so a client knows the reconnect delay even if
            // nothing ever happens.
            retry: Some(Event::default().retry(RETRY_INTERVAL)),
            finished: false,
        }
    }
}

impl Stream for EventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(retry) = self.retry.take() {
            return Poll::Ready(Some(Ok(retry)));
        }
        if self.finished {
            return Poll::Ready(None);
        }
        match self.pending.as_mut().poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready((result, receiver)) => match result {
                Ok(event) => {
                    let frame = event.to_sse();
                    self.pending = Box::pin(receive(receiver));
                    Poll::Ready(Some(Ok(frame)))
                }
                // **Lagged ends the stream.** Skipping ahead would hand the client
                // a silently truncated history and let it believe it had seen
                // everything. Ending is honest: the `EventSource` reconnects and
                // refetches, which is the only correct recovery anyway — the
                // events are state notifications, and the state is refetchable.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    self.finished = true;
                    Poll::Ready(None)
                }
                // Every sender is gone: the server is shutting down. End cleanly.
                Err(broadcast::error::RecvError::Closed) => {
                    self.finished = true;
                    Poll::Ready(None)
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_event_names_are_exactly_the_wire_vocabulary() {
        assert_eq!(ServerEvent::GenerationStarted.name(), "generation-started");
        assert_eq!(
            ServerEvent::GenerationSucceeded { partial: false }.name(),
            "generation-succeeded"
        );
        assert_eq!(
            ServerEvent::GenerationFailed {
                code: "x".into(),
                message: "y".into(),
            }
            .name(),
            "generation-failed"
        );
    }

    #[test]
    fn the_payloads_are_exactly_the_documented_compact_json() {
        assert_eq!(ServerEvent::GenerationStarted.data(), "{}");
        assert_eq!(
            ServerEvent::GenerationSucceeded { partial: false }.data(),
            r#"{"partial":false}"#
        );
        assert_eq!(
            ServerEvent::GenerationSucceeded { partial: true }.data(),
            r#"{"partial":true}"#
        );
        assert_eq!(
            ServerEvent::GenerationFailed {
                code: "rustdoc_failed".into(),
                message: "the crate did not compile".into(),
            }
            .data(),
            r#"{"code":"rustdoc_failed","message":"the crate did not compile"}"#
        );
    }

    #[test]
    fn a_message_is_transported_unchanged() {
        // Core owns sanitization; the server must not "help" by rewriting.
        let event = ServerEvent::GenerationFailed {
            code: "code".into(),
            message: "a message with \"quotes\" and a — dash".into(),
        };
        let data: serde_json::Value = serde_json::from_str(&event.data()).unwrap();
        assert_eq!(data["message"], "a message with \"quotes\" and a — dash");
    }

    #[test]
    fn the_production_constants_are_the_documented_ones() {
        assert_eq!(EVENT_CHANNEL_CAPACITY, 16);
        assert_eq!(KEEPALIVE_INTERVAL, Duration::from_secs(15));
        assert_eq!(RETRY_INTERVAL, Duration::from_millis(1000));
        assert_eq!(SseOptions::default().keepalive, KEEPALIVE_INTERVAL);
    }
}
