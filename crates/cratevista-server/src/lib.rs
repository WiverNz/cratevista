//! CrateVista local loopback server and embedded UI.
//!
//! `cratevista-server` serves an **existing** generated artifact snapshot over a
//! loopback HTTP server: `/api/{document,generation,diagnostics,health,source}`
//! plus the prebuilt SPA embedded via `rust-embed`. It loads the three PRD-05
//! artifacts **hash-verified** against the BLAKE3 digests embedded in
//! `generation.json` (see [`load_snapshot`]) so a request never mixes
//! generations, and exposes a replaceable [`AppState`] + a shutdown handle so
//! PRD 09 can add watcher-driven live reload without rewriting handlers.
//!
//! The lifecycle is four unambiguous primitives — [`bind_listener`],
//! [`shutdown_channel`], [`build_router`], [`run`] — so port behavior and
//! handlers are testable without a long-running process. `cratevista-core` owns
//! the browser-opening and Ctrl-C sequence.
//!
//! It depends only on `cratevista-schema` (plus HTTP/embedding crates) — never
//! on core, the CLI, or the analyzer crates. See
//! `PRD/issue_06_server_and_embedded_ui.md`.
#![forbid(unsafe_code)]

pub mod api;
pub mod assets;
pub mod bind;
pub mod error;
pub mod events;
pub mod options;
pub mod router;
pub mod shutdown;
pub mod snapshot;
pub mod source;
pub mod state;

pub use bind::bind_listener;
pub use error::{ServerError, SnapshotError};
pub use events::{EVENT_CHANNEL_CAPACITY, KEEPALIVE_INTERVAL, RETRY_INTERVAL, ServerEvent};
pub use options::{ArtifactPaths, BindOptions, SnapshotLoadOptions, SourceAccessPolicy};
pub use router::{CONTENT_SECURITY_POLICY, build_router, run};
pub use shutdown::{ShutdownHandle, ShutdownSignal, shutdown_channel};
pub use snapshot::{ArtifactSnapshot, SnapshotMarker, load_snapshot};
pub use source::SourceError;
pub use state::AppState;
