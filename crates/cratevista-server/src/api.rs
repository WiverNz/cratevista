//! JSON API handlers.
//!
//! `/api/{document,generation,diagnostics}` serve the **exact stored canonical
//! bytes** from the current snapshot (no per-request re-serialization).
//! `/api/health` reports liveness + schema version + partial flag. HEAD is
//! handled automatically by axum for these GET routes. Security headers
//! (`nosniff`, CSP, …) are applied globally by the router layer; these handlers
//! set only `Content-Type` and `Cache-Control`.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::Response;

use crate::state::AppState;

const JSON_CONTENT_TYPE: &str = "application/json; charset=utf-8";

/// The snapshot-identity header carried by the three artifact routes.
///
/// The three artifacts are three requests. A client that fetches them separately
/// while a swap lands between two of them would otherwise assemble a **mixed**
/// set — a document from one generation with diagnostics from another — with no
/// way to notice. Equal header values across the triple prove one generation.
///
/// It is deliberately **not** set on `/api/health` (which reports liveness, not
/// an artifact), on `/api/source` (file content, not a snapshot artifact), or on
/// static assets (which are build-time constants, not generated output).
pub const SNAPSHOT_HEADER: &str = "x-cratevista-snapshot";

/// Builds a `200 OK` JSON response from already-canonical bytes.
fn stored_json(bytes: &[u8]) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, JSON_CONTENT_TYPE)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes.to_vec()))
        .expect("static response builds")
}

/// Builds an artifact response: canonical bytes plus the snapshot token.
///
/// `token` comes from the **same** [`ArtifactSnapshot`](crate::ArtifactSnapshot)
/// the bytes came from, because each handler loads the snapshot exactly once —
/// so the header can never describe a different generation than the body.
fn artifact_json(bytes: &[u8], token: &str) -> Response {
    let mut response = stored_json(bytes);
    response.headers_mut().insert(
        SNAPSHOT_HEADER,
        token.parse().expect("the token is 64 lowercase hex chars"),
    );
    response
}

/// `GET /api/document` — the exact `document.json` bytes.
pub async fn document(State(state): State<Arc<AppState>>) -> Response {
    let snapshot = state.snapshot();
    artifact_json(&snapshot.document_bytes, snapshot.marker.token())
}

/// `GET /api/generation` — the exact `generation.json` bytes.
pub async fn generation(State(state): State<Arc<AppState>>) -> Response {
    let snapshot = state.snapshot();
    artifact_json(&snapshot.generation_bytes, snapshot.marker.token())
}

/// `GET /api/diagnostics` — the exact `diagnostics.json` bytes.
pub async fn diagnostics(State(state): State<Arc<AppState>>) -> Response {
    let snapshot = state.snapshot();
    artifact_json(&snapshot.diagnostics_bytes, snapshot.marker.token())
}

/// `GET /api/health` — liveness, schema version, the partial flag, and whether
/// watch mode is on.
///
/// Partial-but-valid stays `200` with `partial: true`. Exposes no paths,
/// usernames, or environment.
///
/// `watch_enabled` is **always present** and is a bare boolean: it is a
/// capability probe, not a status. A frontend must not open an `EventSource`
/// against a server that has no event stream, because `EventSource` reconnects
/// forever on failure — so the client asks first. It reveals only that watching
/// is on.
pub async fn health(State(state): State<Arc<AppState>>) -> Response {
    let snapshot = state.snapshot();
    let body = serde_json::json!({
        "status": "ok",
        "schema_version": snapshot.document.schema_version.as_str(),
        "partial": snapshot.partial,
        "watch_enabled": state.watch_enabled(),
    });
    let bytes = serde_json::to_vec(&body).expect("health body serializes");
    stored_json(&bytes)
}
