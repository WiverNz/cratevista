//! Router assembly, security headers, SPA/API fallback, and the serve loop.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::http::{HeaderValue, header::HeaderName};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::api;
use crate::assets;
use crate::error::ServerError;
use crate::events;
use crate::shutdown::ShutdownSignal;
use crate::source;
use crate::state::AppState;

/// The Content-Security-Policy applied to every response.
///
/// **PRD-07 extension point:** this is the single source of truth for the CSP.
/// The interactive UI may deliberately widen it (e.g. `connect-src` for SSE in
/// PRD 09), but a self-hosted external-file SPA works under it.
///
/// **PRD-07 amendment (approved):** `style-src-attr 'unsafe-inline'` is allowed
/// narrowly so React Flow can position nodes via dynamically-assigned inline
/// `style` attributes (`transform: translate(x,y)`), and `worker-src 'self'`
/// permits the same-origin ELK layout module worker. It deliberately keeps **no**
/// `script-src`/`style-src` `'unsafe-inline'`, **no** `'unsafe-eval'`, and **no**
/// remote origins. See `docs/adr/0006-server-and-security.md` and
/// `PRD/issue_07_interactive_explorer_ui.md`.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self'; style-src-attr 'unsafe-inline'; connect-src 'self'; \
     worker-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'";

/// Builds the API + SPA router with security headers and a state seam.
pub fn build_router(state: Arc<AppState>) -> Router {
    let mut router = Router::new()
        .route("/api/health", get(api::health))
        .route("/api/document", get(api::document))
        .route("/api/generation", get(api::generation))
        .route("/api/diagnostics", get(api::diagnostics))
        .route("/api/source", get(source::source));

    // `/api/events` exists **only** when this server is actually watching. An
    // ordinary `serve` has nothing to publish, so the honest answer is the
    // router's usual unknown-API 404 — not an endpoint that accepts a connection
    // and then says nothing forever. `/api/health.watch_enabled` is what a client
    // consults to find out, rather than probing this route.
    if state.watch_enabled() {
        router = router.route("/api/events", get(events::events));
    }

    router
        .fallback(fallback)
        .layer(header_layer(
            header::CONTENT_SECURITY_POLICY,
            CONTENT_SECURITY_POLICY,
        ))
        .layer(header_layer(header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .layer(header_layer(header::REFERRER_POLICY, "same-origin"))
        .layer(header_layer(header::X_FRAME_OPTIONS, "DENY"))
        .with_state(state)
}

/// Runs the server on an already-bound listener until `shutdown` fires, then
/// completes graceful shutdown. Blocks (awaits) until then.
pub async fn run(
    listener: tokio::net::TcpListener,
    state: Arc<AppState>,
    shutdown: ShutdownSignal,
) -> Result<(), ServerError> {
    let app = build_router(state);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown.wait())
        .await
        .map_err(|error| ServerError::ShutdownFailed(error.to_string()))
}

fn header_layer(name: HeaderName, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(name, HeaderValue::from_static(value))
}

/// Fallback: unknown `/api/*` → JSON `404`; anything else → SPA asset/index.
async fn fallback(uri: Uri) -> Response {
    if uri.path().starts_with("/api/") {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": "unknown API route" }
            })),
        )
            .into_response();
    }
    assets::serve_path(uri.path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;

    use cratevista_schema::canonical::to_canonical_string;
    use cratevista_schema::{
        ArtifactHashes, Counts, DiagnosticsReport, ExplorerDocument, GenerationReport, Generator,
        Project, SchemaVersion, Timestamp,
    };

    use crate::options::{ArtifactPaths, SnapshotLoadOptions, SourceAccessPolicy};
    use crate::snapshot::load_snapshot;

    fn hex(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }

    fn write_valid_snapshot(dir: &std::path::Path) {
        write_snapshot_at(dir, "2026-07-14T00:00:00Z");
    }

    /// Writes a valid snapshot whose `generation.json` differs by timestamp, so
    /// the marker — and therefore the snapshot token — differs too.
    fn write_snapshot_at(dir: &std::path::Path, generated_at: &str) {
        let project = Project {
            id: "workspace".into(),
            name: "ws".into(),
            description: String::new(),
            root: None,
            repository_url: None,
            default_branch: None,
        };
        let document = to_canonical_string(&ExplorerDocument::new(project, vec![], vec![], vec![]))
            .unwrap()
            .into_bytes();
        let diagnostics = to_canonical_string(&DiagnosticsReport::new(vec![]))
            .unwrap()
            .into_bytes();
        let report = GenerationReport {
            generator: Generator {
                name: "cargo-cratevista".into(),
                version: "0.1.0".into(),
            },
            generated_at: Timestamp::new(generated_at),
            toolchain: None,
            rustdoc_format_version: None,
            input_hashes: Default::default(),
            counts: Counts {
                entities: 0,
                relations: 0,
                views: 0,
                diagnostics: 0,
            },
            durations_ms: Default::default(),
            artifact_hashes: Some(ArtifactHashes {
                document_blake3: hex(&document),
                diagnostics_blake3: hex(&diagnostics),
            }),
            partial: false,
        };
        let generation = to_canonical_string(&report).unwrap().into_bytes();
        std::fs::write(dir.join("document.json"), &document).unwrap();
        std::fs::write(dir.join("diagnostics.json"), &diagnostics).unwrap();
        std::fs::write(dir.join("generation.json"), &generation).unwrap();
    }

    fn test_state(source: SourceAccessPolicy) -> Arc<AppState> {
        let dir = tempfile::tempdir().unwrap();
        write_valid_snapshot(dir.path());
        let snapshot = load_snapshot(
            &ArtifactPaths::in_dir(dir.path()),
            &SnapshotLoadOptions::default(),
        )
        .unwrap();
        AppState::new(snapshot, source)
    }

    /// Loads a snapshot whose generation timestamp — and thus token — is `stamp`.
    fn snapshot_at(stamp: &str) -> crate::snapshot::ArtifactSnapshot {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot_at(dir.path(), stamp);
        load_snapshot(
            &ArtifactPaths::in_dir(dir.path()),
            &SnapshotLoadOptions::default(),
        )
        .unwrap()
    }

    /// `GET uri`, returning the `X-CrateVista-Snapshot` header (if any).
    async fn snapshot_header(router: Router, uri: &str) -> Option<String> {
        let response = router
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        response
            .headers()
            .get(crate::api::SNAPSHOT_HEADER)
            .map(|value| value.to_str().unwrap().to_string())
    }

    // --- PRD-09 server-events harness -------------------------------------

    use crate::events::{SseOptions, sse_response};
    use crate::{EVENT_CHANNEL_CAPACITY, ServerEvent};
    use std::time::Duration;

    /// A watch-enabled state, built directly: no CLI enables watching yet.
    fn watching_state() -> Arc<AppState> {
        let dir = tempfile::tempdir().unwrap();
        write_valid_snapshot(dir.path());
        let snapshot = load_snapshot(
            &ArtifactPaths::in_dir(dir.path()),
            &SnapshotLoadOptions::default(),
        )
        .unwrap();
        AppState::new_watching(snapshot, SourceAccessPolicy::Disabled)
    }

    /// Awaits `future` with a generous watchdog.
    ///
    /// Never fires in a correct run — every wait is unblocked by a published event
    /// or by the stream ending. It exists so a broken stream fails in seconds with
    /// a named message instead of hanging CI. Not a correctness assertion: the
    /// assertions are on the frames themselves.
    async fn within<T>(what: &str, future: impl std::future::Future<Output = T>) -> T {
        match tokio::time::timeout(Duration::from_secs(5), future).await {
            Ok(value) => value,
            Err(_) => panic!("timed out waiting for {what} — the SSE stream did not progress"),
        }
    }

    /// Reads raw SSE frames off a real response body.
    ///
    /// Asserting on bytes rather than on a parsed abstraction is the point: the
    /// wire format *is* the contract a browser reads.
    struct Frames {
        body: axum::body::BodyDataStream,
    }

    fn frames_of(
        receiver: tokio::sync::broadcast::Receiver<ServerEvent>,
        options: SseOptions,
    ) -> Frames {
        let response = sse_response(receiver, options);
        Frames {
            body: response.into_body().into_data_stream(),
        }
    }

    impl Frames {
        /// The next frame, or `None` once the stream ends.
        async fn next_frame(&mut self) -> Option<String> {
            use futures_core::Stream;
            use std::pin::Pin;
            let chunk = std::future::poll_fn(|cx| Pin::new(&mut self.body).poll_next(cx)).await?;
            Some(String::from_utf8(chunk.expect("a body chunk").to_vec()).expect("utf-8"))
        }

        /// The next frame, asserting the stream has not ended.
        async fn next(&mut self) -> String {
            self.next_frame().await.expect("the stream ended early")
        }
    }

    async fn body_string(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn document_returns_exact_stored_bytes() {
        let state = test_state(SourceAccessPolicy::Disabled);
        let stored = String::from_utf8(state.snapshot().document_bytes.to_vec()).unwrap();
        let router = build_router(state);
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/document")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json; charset=utf-8"
        );
        assert_eq!(body_string(response).await, stored);
    }

    #[tokio::test]
    async fn health_reports_schema_version_and_partial() {
        let router = build_router(test_state(SourceAccessPolicy::Disabled));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
        assert_eq!(body["status"], "ok");
        // Not a literal: the health endpoint reports whatever the schema's
        // current version is, so a minor bump must not silently rot this test.
        assert_eq!(body["schema_version"], SchemaVersion::CURRENT);
        assert_eq!(body["partial"], false);
    }

    #[tokio::test]
    async fn head_request_has_empty_body() {
        let router = build_router(test_state(SourceAccessPolicy::Disabled));
        let response = router
            .oneshot(
                Request::builder()
                    .method("HEAD")
                    .uri("/api/document")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_string(response).await, "");
    }

    #[tokio::test]
    async fn unknown_api_route_is_json_404() {
        let router = build_router(test_state(SourceAccessPolicy::Disabled));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
        assert_eq!(body["error"]["code"], "not_found");
    }

    #[tokio::test]
    async fn wrong_method_is_405_with_allow() {
        let router = build_router(test_state(SourceAccessPolicy::Disabled));
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/document")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        let allow = response
            .headers()
            .get(header::ALLOW)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(allow.contains("GET"), "Allow header lists GET: {allow}");
    }

    #[tokio::test]
    async fn spa_fallback_serves_index_html() {
        let router = build_router(test_state(SourceAccessPolicy::Disabled));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/some/client/route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(content_type.starts_with("text/html"));
        assert!(body_string(response).await.contains("CrateVista"));
    }

    /// The bundle's real hashed asset names, discovered from the embedded files
    /// rather than hard-coded: the content hash changes on every rebuild.
    fn embedded_asset(extension: &str) -> String {
        crate::assets::embedded_names()
            .into_iter()
            .find(|name| name.starts_with("assets/") && name.ends_with(extension))
            .unwrap_or_else(|| panic!("the embedded bundle must contain an {extension} asset"))
    }

    #[tokio::test]
    async fn bundle_js_and_css_served_with_correct_mime() {
        let js = format!("/{}", embedded_asset(".js"));
        let css = format!("/{}", embedded_asset(".css"));
        for (path, expected) in [(js.as_str(), "javascript"), (css.as_str(), "text/css")] {
            let router = build_router(test_state(SourceAccessPolicy::Disabled));
            let response = router
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let content_type = response
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            assert!(
                content_type.contains(expected),
                "{path} content-type {content_type} contains {expected}"
            );
        }
    }

    #[tokio::test]
    async fn security_headers_and_csp_present() {
        let router = build_router(test_state(SourceAccessPolicy::Disabled));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let headers = response.headers();
        assert_eq!(
            headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );
        assert_eq!(headers.get(header::X_FRAME_OPTIONS).unwrap(), "DENY");
        assert_eq!(headers.get(header::REFERRER_POLICY).unwrap(), "same-origin");
        assert_eq!(
            headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            CONTENT_SECURITY_POLICY
        );
        // PRD-07 amendment: the ONLY inline allowance is the narrow
        // `style-src-attr 'unsafe-inline'` (for React Flow node geometry).
        assert!(CONTENT_SECURITY_POLICY.contains("style-src-attr 'unsafe-inline'"));
        assert!(CONTENT_SECURITY_POLICY.contains("worker-src 'self'"));
        assert!(CONTENT_SECURITY_POLICY.contains("connect-src 'self'"));
        // No broad script/style inline, no eval, no framing/object escape.
        assert!(!CONTENT_SECURITY_POLICY.contains("script-src 'self' 'unsafe-inline'"));
        assert!(!CONTENT_SECURITY_POLICY.contains("style-src 'self' 'unsafe-inline'"));
        assert!(!CONTENT_SECURITY_POLICY.contains("unsafe-eval"));
        // The only `unsafe-inline` token in the whole policy is the style-attr one.
        assert_eq!(CONTENT_SECURITY_POLICY.matches("unsafe-inline").count(), 1);
        // No permissive CORS.
        assert!(headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    }

    #[tokio::test]
    async fn source_disabled_returns_403() {
        let router = build_router(test_state(SourceAccessPolicy::Disabled));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/source?path=src/lib.rs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
        assert_eq!(body["error"]["code"], "source_disabled");
    }

    #[tokio::test]
    async fn source_encoded_traversal_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let state = test_state(SourceAccessPolicy::Enabled {
            root: root.path().to_path_buf(),
            max_bytes: 1024,
        });
        let router = build_router(state);
        // %2e%2e%2f decodes to "../"; axum decodes before the handler sees it.
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/source?path=%2e%2e%2f%2e%2e%2fsecret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
        assert_eq!(body["error"]["code"], "source_path_invalid");
    }

    // --- PRD-06 amendment A1: X-CrateVista-Snapshot -----------------------

    #[tokio::test]
    async fn all_three_artifact_routes_expose_one_token_from_one_snapshot() {
        let state = test_state(SourceAccessPolicy::Disabled);
        let expected = state.snapshot().marker.token().to_string();

        let document = snapshot_header(build_router(state.clone()), "/api/document").await;
        let generation = snapshot_header(build_router(state.clone()), "/api/generation").await;
        let diagnostics = snapshot_header(build_router(state.clone()), "/api/diagnostics").await;

        // One token, from one AppState snapshot, on all three.
        assert_eq!(document.as_deref(), Some(expected.as_str()));
        assert_eq!(generation.as_deref(), Some(expected.as_str()));
        assert_eq!(diagnostics.as_deref(), Some(expected.as_str()));
    }

    #[tokio::test]
    async fn replace_snapshot_changes_all_three_tokens_atomically() {
        let state = test_state(SourceAccessPolicy::Disabled);
        let before = state.snapshot().marker.token().to_string();

        // A genuinely different generation.
        let next = snapshot_at("2026-07-15T00:00:00Z");
        let after_expected = next.marker.token().to_string();
        assert_ne!(before, after_expected, "the fixture must actually differ");

        state.replace_snapshot(next);

        let document = snapshot_header(build_router(state.clone()), "/api/document").await;
        let generation = snapshot_header(build_router(state.clone()), "/api/generation").await;
        let diagnostics = snapshot_header(build_router(state.clone()), "/api/diagnostics").await;

        // All three moved together — no route is left on the old generation.
        assert_eq!(document.as_deref(), Some(after_expected.as_str()));
        assert_eq!(generation.as_deref(), Some(after_expected.as_str()));
        assert_eq!(diagnostics.as_deref(), Some(after_expected.as_str()));
        assert_ne!(document.as_deref(), Some(before.as_str()));
    }

    #[tokio::test]
    async fn the_snapshot_header_is_not_on_unrelated_routes() {
        let state = test_state(SourceAccessPolicy::Disabled);
        // Health reports liveness, not an artifact; assets are build-time
        // constants; source is file content, not a snapshot artifact.
        assert_eq!(
            snapshot_header(build_router(state.clone()), "/api/health").await,
            None
        );
        assert_eq!(
            snapshot_header(build_router(state.clone()), "/index.html").await,
            None
        );
        assert_eq!(
            snapshot_header(build_router(state), "/api/source?path=src/lib.rs").await,
            None
        );
    }

    #[tokio::test]
    async fn the_snapshot_header_preserves_cache_control_and_body_bytes() {
        let state = test_state(SourceAccessPolicy::Disabled);
        let stored = String::from_utf8(state.snapshot().document_bytes.to_vec()).unwrap();
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/document")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert!(
            response
                .headers()
                .get(crate::api::SNAPSHOT_HEADER)
                .is_some()
        );
        assert_eq!(body_string(response).await, stored);
    }

    // --- PRD-06 amendment A2: /api/health.watch_enabled -------------------

    #[tokio::test]
    async fn health_reports_watch_disabled_for_ordinary_serve_and_open() {
        let state = test_state(SourceAccessPolicy::Disabled);
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
        // Always present, and false unless the process is actually watching.
        assert_eq!(body["watch_enabled"], serde_json::json!(false));
    }

    #[tokio::test]
    async fn health_serializes_watch_enabled_true_for_a_watching_state() {
        // Constructed directly: no CLI surface enables watching in this phase.
        let state = AppState::new_watching(
            snapshot_at("2026-07-14T00:00:00Z"),
            SourceAccessPolicy::Disabled,
        );
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
        assert_eq!(body["watch_enabled"], serde_json::json!(true));
        assert_eq!(body["status"], serde_json::json!("ok"));
    }

    #[tokio::test]
    async fn a_non_watching_state_serves_no_event_stream() {
        // Superseded the step-2.2-era "not exposed yet" test: the route now exists,
        // but only for a watching state. For `serve` there is still no stream.
        let state = test_state(SourceAccessPolicy::Disabled);
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        assert!(
            !content_type.contains("text/event-stream"),
            "a non-watching server must not stream events, got {content_type}"
        );
    }

    // --- PRD-09 server events: route availability -------------------------

    #[tokio::test]
    async fn api_events_is_the_ordinary_unknown_api_404_when_not_watching() {
        // Not registered at all — so this is the router's existing fallback, not a
        // special case. A `serve` has nothing to publish, and an endpoint that
        // accepts a connection and then says nothing forever would be worse than
        // an honest 404.
        let state = test_state(SourceAccessPolicy::Disabled);
        assert!(!state.watch_enabled());
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
        assert_eq!(body["error"]["code"], "not_found");
        assert_eq!(body["error"]["message"], "unknown API route");
    }

    #[tokio::test]
    async fn api_events_is_an_sse_stream_when_watching() {
        let state = watching_state();
        assert!(state.watch_enabled());
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        // The global security headers still apply to a stream.
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_SECURITY_POLICY)
                .unwrap(),
            CONTENT_SECURITY_POLICY
        );
        assert_eq!(
            response
                .headers()
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .unwrap(),
            "nosniff"
        );
        // Not an artifact route: no snapshot token.
        assert!(
            response
                .headers()
                .get(crate::api::SNAPSHOT_HEADER)
                .is_none()
        );
    }

    // --- SSE wire contract ------------------------------------------------

    #[tokio::test]
    async fn the_stream_begins_with_the_retry_directive_and_no_id() {
        let state = watching_state();
        let mut frames = frames_of(state.subscribe_events(), SseOptions::default());

        let first = within("the retry frame", frames.next()).await;
        assert_eq!(first, "retry: 1000\n\n", "the reconnect hint comes first");
        assert!(!first.contains("id:"), "replay must be impossible: {first}");
    }

    #[tokio::test]
    async fn each_variant_produces_its_exact_event_name_and_payload() {
        let state = watching_state();
        let mut frames = frames_of(state.subscribe_events(), SseOptions::default());
        assert_eq!(within("retry", frames.next()).await, "retry: 1000\n\n");

        state.publish_event(ServerEvent::GenerationStarted);
        assert_eq!(
            within("started", frames.next()).await,
            "event: generation-started\ndata: {}\n\n"
        );

        state.publish_event(ServerEvent::GenerationSucceeded { partial: false });
        assert_eq!(
            within("succeeded", frames.next()).await,
            "event: generation-succeeded\ndata: {\"partial\":false}\n\n"
        );

        state.publish_event(ServerEvent::GenerationSucceeded { partial: true });
        assert_eq!(
            within("succeeded partial", frames.next()).await,
            "event: generation-succeeded\ndata: {\"partial\":true}\n\n"
        );

        state.publish_event(ServerEvent::GenerationFailed {
            code: "rustdoc_failed".into(),
            message: "the crate did not compile".into(),
        });
        assert_eq!(
            within("failed", frames.next()).await,
            "event: generation-failed\ndata: {\"code\":\"rustdoc_failed\",\"message\":\"the crate did not compile\"}\n\n"
        );
    }

    #[tokio::test]
    async fn no_frame_ever_carries_an_id_field() {
        let state = watching_state();
        let mut frames = frames_of(state.subscribe_events(), SseOptions::default());
        state.publish_event(ServerEvent::GenerationStarted);
        state.publish_event(ServerEvent::GenerationFailed {
            code: "c".into(),
            message: "m".into(),
        });
        for _ in 0..3 {
            let frame = within("a frame", frames.next()).await;
            assert!(
                !frame.contains("id:"),
                "an id would let a client ask for replay: {frame}"
            );
        }
    }

    #[tokio::test]
    async fn a_last_event_id_header_is_ignored_without_error_or_replay() {
        let state = watching_state();
        // Events published before anyone connects are not replayed to a new
        // subscriber that asks for them.
        state.publish_event(ServerEvent::GenerationStarted);

        let response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/api/events")
                    .header("Last-Event-ID", "42")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "ignored, not rejected");
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
    }

    #[tokio::test]
    async fn a_keepalive_is_a_comment_and_not_a_fourth_event_type() {
        // Injected interval: the production constant stays 15 s, and a test must
        // not wait 15 s to see a comment. The wait is on the stream, not a sleep.
        let state = watching_state();
        let mut frames = frames_of(
            state.subscribe_events(),
            SseOptions {
                keepalive: Duration::from_millis(10),
            },
        );
        assert_eq!(within("retry", frames.next()).await, "retry: 1000\n\n");

        let keepalive = within("the keepalive comment", frames.next()).await;
        assert!(
            keepalive.starts_with(':'),
            "a keepalive must be an SSE comment: {keepalive:?}"
        );
        assert!(
            !keepalive.contains("event:"),
            "a keepalive is not a fourth event type: {keepalive:?}"
        );
        assert!(!keepalive.contains("id:"));
    }

    // --- fan-out ----------------------------------------------------------

    #[tokio::test]
    async fn two_subscribers_receive_the_same_events_in_publication_order() {
        let state = watching_state();
        let mut first = frames_of(state.subscribe_events(), SseOptions::default());
        let mut second = frames_of(state.subscribe_events(), SseOptions::default());
        assert_eq!(within("retry", first.next()).await, "retry: 1000\n\n");
        assert_eq!(within("retry", second.next()).await, "retry: 1000\n\n");

        state.publish_event(ServerEvent::GenerationStarted);
        state.publish_event(ServerEvent::GenerationSucceeded { partial: true });

        for stream in [&mut first, &mut second] {
            assert_eq!(
                within("started", stream.next()).await,
                "event: generation-started\ndata: {}\n\n"
            );
            assert_eq!(
                within("succeeded", stream.next()).await,
                "event: generation-succeeded\ndata: {\"partial\":true}\n\n"
            );
        }
    }

    #[tokio::test]
    async fn dropping_one_subscriber_neither_blocks_nor_fails_the_other() {
        let state = watching_state();
        let doomed = state.subscribe_events();
        let mut survivor = frames_of(state.subscribe_events(), SseOptions::default());
        assert_eq!(within("retry", survivor.next()).await, "retry: 1000\n\n");

        drop(doomed);

        state.publish_event(ServerEvent::GenerationStarted);
        assert_eq!(
            within("started", survivor.next()).await,
            "event: generation-started\ndata: {}\n\n"
        );
    }

    #[tokio::test]
    async fn publishing_with_no_subscribers_is_harmless() {
        let state = watching_state();
        // No panic, no error surfaced: a server nobody has opened still works.
        state.publish_event(ServerEvent::GenerationStarted);
        state.publish_event(ServerEvent::GenerationFailed {
            code: "c".into(),
            message: "m".into(),
        });

        // And a later subscriber is not handed the backlog as if it were live.
        let mut frames = frames_of(state.subscribe_events(), SseOptions::default());
        assert_eq!(within("retry", frames.next()).await, "retry: 1000\n\n");
        state.publish_event(ServerEvent::GenerationSucceeded { partial: false });
        assert_eq!(
            within("only the new event", frames.next()).await,
            "event: generation-succeeded\ndata: {\"partial\":false}\n\n"
        );
    }

    #[tokio::test]
    async fn a_lagging_subscriber_terminates_instead_of_silently_skipping() {
        // Overflow the capacity-16 channel before reading a single event. A
        // truncated sequence would let the client believe it had seen everything;
        // ending the stream makes it reconnect and refetch, which is the only
        // correct recovery.
        let state = watching_state();
        let mut frames = frames_of(state.subscribe_events(), SseOptions::default());
        assert_eq!(within("retry", frames.next()).await, "retry: 1000\n\n");

        for _ in 0..(EVENT_CHANNEL_CAPACITY + 8) {
            state.publish_event(ServerEvent::GenerationStarted);
        }

        // The stream ends rather than yielding a silently-truncated history.
        assert_eq!(
            within("the lagged stream to end", frames.next_frame()).await,
            None,
            "a lagged subscriber must be dropped, not fed a gap"
        );
    }

    #[tokio::test]
    async fn a_closed_sender_ends_the_stream_cleanly() {
        let state = watching_state();
        let mut frames = frames_of(state.subscribe_events(), SseOptions::default());
        assert_eq!(within("retry", frames.next()).await, "retry: 1000\n\n");

        // The state — and with it the only sender — goes away.
        drop(state);
        assert_eq!(
            within("the stream to close", frames.next_frame()).await,
            None,
            "shutdown ends the stream, it does not hang"
        );
    }
}
