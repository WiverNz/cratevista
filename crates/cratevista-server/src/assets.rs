//! Embedded static assets and the SPA fallback.
//!
//! The prebuilt frontend under `crates/cratevista-server/embedded/` is embedded at
//! build time via `rust-embed` (with `debug-embed`, so a missing `embedded/` fails
//! the build in every profile — no Node.js is ever required at run time). The
//! bundle lives **inside** this crate so `cargo package` includes it. Unknown
//! non-API paths fall back to `index.html` so client-side routing works. Only
//! provably **fingerprinted** filenames get long-term immutable caching;
//! `index.html` and plain `app.js` / `style.css` are `no-cache`.

use std::borrow::Cow;

use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::Response;
use rust_embed::Embed;

// Crate-local (resolved against `CARGO_MANIFEST_DIR`), so it is part of this
// package's file set. `web/dist` no longer exists — this is the sole bundle.
#[derive(Embed)]
#[folder = "embedded"]
struct Assets;

/// Serves the embedded asset for `path`, falling back to `index.html` for any
/// unknown non-API path (the SPA convention). Always returns `200` with the
/// correct `Content-Type` and a `Cache-Control` chosen by [`cache_control_for`].
pub fn serve_path(path: &str) -> Response {
    let trimmed = path.trim_start_matches('/');
    let key = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };
    match Assets::get(key) {
        Some(file) => asset_response(key, file.data.as_ref()),
        None => match Assets::get("index.html") {
            Some(index) => asset_response("index.html", index.data.as_ref()),
            None => Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("embedded assets missing"))
                .expect("error response builds"),
        },
    }
}

fn asset_response(name: &str, bytes: &[u8]) -> Response {
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache_control_for(name))
        .body(Body::from(bytes.to_vec()))
        .expect("asset response builds")
}

/// The `Cache-Control` value for an asset name.
///
/// `index.html` is `no-cache` (always revalidate). A **fingerprinted** asset
/// (see [`is_fingerprinted`]) is safe to cache immutably for a year. Everything
/// else — including plain `app.js` / `style.css` — is `no-cache`.
fn cache_control_for(name: &str) -> &'static str {
    if name.ends_with("index.html") {
        "no-cache"
    } else if is_fingerprinted(name) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

/// Whether `name` satisfies the public path contract shared by [`serve_path`] and
/// [`embedded_assets`]: relative, `/`-separated, non-empty, no leading slash, no
/// backslash, and no empty / `.` / `..` component.
///
/// rust-embed already yields such names, so this is a guard rather than a
/// transform: it makes the one path policy explicit in a single place so the
/// enumeration API and the serving path cannot silently drift.
fn is_embeddable_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && name
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

/// Every embedded frontend asset as `(normalized relative path, exact bytes)`, in
/// **deterministic lexicographic order** by path.
///
/// This is the enumeration counterpart to [`serve_path`]: both read the same
/// private embedded [`Assets`], so a static site materialized from this API is
/// byte-for-byte the UI the server serves. It is intended for **site
/// materialization**, not per-request serving.
///
/// # Guarantees
///
/// - **Deterministic order.** rust-embed's own iteration order is unspecified, so
///   the entries are collected and sorted by path here. The bundle is small.
/// - **Real files only.** It enumerates the embedded files exactly; it never adds
///   the SPA-fallback entry that [`serve_path`] synthesizes for unknown paths.
/// - **Path contract.** Every returned name is relative, `/`-separated, non-empty,
///   has no leading slash and no `.`/`..` component — the same key [`serve_path`]
///   accepts, so `serve_path(name)` returns exactly the bytes paired with it here.
///
/// The item type is deliberately `(String, Cow<'static, [u8]>)`: no `rust_embed`
/// type and no axum response type crosses this boundary.
pub fn embedded_assets() -> impl Iterator<Item = (String, Cow<'static, [u8]>)> {
    let mut entries: Vec<(String, Cow<'static, [u8]>)> = Assets::iter()
        .map(|name| {
            let name = name.into_owned();
            // `iter()` yields exactly the keys `get()` accepts, so this is
            // infallible; the guard turns a future embed regression into a loud
            // failure here rather than a silently malformed public path.
            debug_assert!(
                is_embeddable_name(&name),
                "embedded asset name violates the path contract: {name:?}"
            );
            let data = Assets::get(&name)
                .expect("Assets::iter yields only embedded keys")
                .data;
            (name, data)
        })
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries.into_iter()
}

/// Whether a filename carries a content-hash fingerprint segment, e.g.
/// `app.4f3a2b1c.js` (a middle, dot-separated segment of at least 8 hex
/// characters). Plain `app.js` is **not** fingerprinted.
pub fn is_fingerprinted(name: &str) -> bool {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let parts: Vec<&str> = file.split('.').collect();
    if parts.len() < 3 {
        return false;
    }
    parts[1..parts.len() - 1]
        .iter()
        .any(|segment| segment.len() >= 8 && segment.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The names of every embedded asset. Used by tests to discover the bundle's
/// content-hashed filenames instead of hard-coding names that change on rebuild.
#[cfg(test)]
pub(crate) fn embedded_names() -> Vec<String> {
    Assets::iter().map(|f| f.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_bundle_is_embedded() {
        // The real PRD-07 Vite bundle: index.html + fingerprinted assets/.
        assert!(
            Assets::get("index.html").is_some(),
            "index.html must be embedded"
        );
        let names = embedded_names();
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("assets/") && n.ends_with(".js")),
            "a bundled script must be embedded: {names:?}"
        );
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("assets/") && n.ends_with(".css")),
            "a bundled stylesheet must be embedded: {names:?}"
        );
        // The ELK layout worker ships as a same-origin asset (never a blob:).
        assert!(
            names
                .iter()
                .any(|n| n.contains("elk.worker") && n.ends_with(".js")),
            "the ELK worker must be embedded: {names:?}"
        );
    }

    #[test]
    fn index_html_has_no_inline_script_or_style_and_only_self_hosted_assets() {
        let index = Assets::get("index.html").unwrap();
        let text = String::from_utf8(index.data.to_vec()).unwrap();
        // A strict CSP forbids inline JS/CSS.
        assert!(!text.contains("<script>"), "no inline <script>");
        assert!(!text.contains("<style"), "no inline <style>");
        // Relative, self-hosted assets only (Vite `base: "./"`), no remote origin.
        assert!(
            text.contains("./assets/"),
            "assets are referenced relatively"
        );
        assert!(!text.contains("http://"), "no remote origin");
        assert!(!text.contains("https://"), "no remote origin");
    }

    /// The embedded index is **server mode**: the static-mode marker is injected
    /// only into the copy `materialize_static_site` writes, never into the bundle
    /// the server serves. A marker here would put the live server into static mode.
    #[test]
    fn embedded_index_has_no_static_mode_marker() {
        let index = Assets::get("index.html").expect("index.html embedded");
        let text = String::from_utf8(index.data.to_vec()).unwrap();
        assert!(
            !text.contains("cratevista-mode"),
            "the embedded server index must not carry the static-mode marker"
        );
    }

    #[test]
    fn production_assets_are_fingerprinted_and_immutable() {
        for name in embedded_names().iter().filter(|n| n.starts_with("assets/")) {
            assert!(is_fingerprinted(name), "{name} should be fingerprinted");
            assert_eq!(
                cache_control_for(name),
                "public, max-age=31536000, immutable",
                "{name} should be immutably cacheable"
            );
        }
        assert_eq!(cache_control_for("index.html"), "no-cache");
    }

    #[test]
    fn fingerprint_detection_rule() {
        assert!(is_fingerprinted("app.4f3a2b1c.js"));
        assert!(is_fingerprinted("assets/index.deadbeef12.css"));
        assert!(!is_fingerprinted("app.js"));
        assert!(!is_fingerprinted("style.css"));
        assert!(!is_fingerprinted("index.html"));
        assert!(!is_fingerprinted("app.min.js")); // "min" is not >=8 hex
    }

    #[test]
    fn plain_assets_are_not_immutable() {
        assert_eq!(cache_control_for("app.js"), "no-cache");
        assert_eq!(cache_control_for("style.css"), "no-cache");
        assert_eq!(cache_control_for("index.html"), "no-cache");
        assert_eq!(
            cache_control_for("app.4f3a2b1c.js"),
            "public, max-age=31536000, immutable"
        );
    }

    // --- embedded_assets() (PRD 10 Phase 1) -------------------------------

    /// The names `embedded_assets` yields, in order.
    fn enumerated_names() -> Vec<String> {
        embedded_assets().map(|(name, _)| name).collect()
    }

    /// 1. The site's entry point is enumerated.
    #[test]
    fn embedded_assets_includes_index_html() {
        assert!(
            enumerated_names().iter().any(|name| name == "index.html"),
            "index.html must be enumerated"
        );
    }

    /// 2. Every fingerprinted asset the bundle actually ships is enumerated, and
    ///    the JS/CSS/worker the UI needs are among them (mirrors
    ///    `production_bundle_is_embedded`, so a missing fingerprinted file fails).
    #[test]
    fn embedded_assets_includes_every_fingerprinted_asset() {
        let names = enumerated_names();
        for embedded in embedded_names() {
            if is_fingerprinted(&embedded) {
                assert!(
                    names.contains(&embedded),
                    "fingerprinted asset {embedded} must be enumerated"
                );
            }
        }
        assert!(
            names
                .iter()
                .any(|name| name.starts_with("assets/") && name.ends_with(".js")),
            "a bundled script must be enumerated: {names:?}"
        );
        assert!(
            names
                .iter()
                .any(|name| name.starts_with("assets/") && name.ends_with(".css")),
            "a bundled stylesheet must be enumerated: {names:?}"
        );
        assert!(
            names
                .iter()
                .any(|name| name.contains("elk.worker") && name.ends_with(".js")),
            "the ELK worker must be enumerated: {names:?}"
        );
    }

    /// 3. Order is lexicographic, not rust-embed's unspecified iteration order.
    #[test]
    fn embedded_assets_paths_are_sorted_lexicographically() {
        let names = enumerated_names();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "paths must be lexicographically ordered");
    }

    /// 4. No path appears twice.
    #[test]
    fn embedded_assets_paths_are_unique() {
        let names = enumerated_names();
        let mut deduped = names.clone();
        deduped.dedup();
        assert_eq!(
            names.len(),
            deduped.len(),
            "paths must be unique: {names:?}"
        );
    }

    /// 5. Every path honours the relative, traversal-free contract.
    #[test]
    fn embedded_assets_paths_are_normalized_and_traversal_free() {
        for name in enumerated_names() {
            assert!(
                is_embeddable_name(&name),
                "path violates the contract: {name:?}"
            );
            assert!(!name.is_empty());
            assert!(!name.starts_with('/'), "{name} has a leading slash");
            assert!(!name.contains('\\'), "{name} has a backslash");
            assert!(
                name.split('/')
                    .all(|c| c != "." && c != ".." && !c.is_empty()),
                "{name} has a dot/empty component"
            );
        }
    }

    /// 6. The enumerated name set is exactly the private embedded-name seam — no
    ///    file is dropped, none is invented.
    #[test]
    fn embedded_assets_name_set_equals_the_embedded_name_seam() {
        let mut enumerated = enumerated_names();
        let mut seam = embedded_names();
        enumerated.sort();
        seam.sort();
        assert_eq!(
            enumerated, seam,
            "enumeration must match the embedded files exactly"
        );
    }

    /// 7. For every enumerated asset, `serve_path` returns a `200` with **exactly**
    ///    the paired bytes — the enumeration and the serving path cannot diverge.
    ///    Compares full response bodies, not counts.
    #[tokio::test]
    async fn serve_path_returns_the_same_bytes_for_every_enumerated_asset() {
        for (name, bytes) in embedded_assets() {
            let response = serve_path(&name);
            assert_eq!(response.status(), StatusCode::OK, "serving {name}");
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body collects");
            assert_eq!(
                body.as_ref(),
                bytes.as_ref(),
                "serve_path body differs from the enumerated bytes for {name}"
            );
        }
    }

    /// 8. The API is deterministic call to call — same order, same bytes.
    #[test]
    fn embedded_assets_is_deterministic_across_calls() {
        let first: Vec<(String, Vec<u8>)> = embedded_assets()
            .map(|(name, bytes)| (name, bytes.into_owned()))
            .collect();
        let second: Vec<(String, Vec<u8>)> = embedded_assets()
            .map(|(name, bytes)| (name, bytes.into_owned()))
            .collect();
        assert_eq!(
            first, second,
            "repeated calls must return identical ordered entries"
        );
    }

    /// 9. No synthetic SPA-fallback entry: every enumerated pair is a genuine
    ///    embedded file, and `index.html` is present exactly once (not injected as
    ///    a fabricated fallback for other names).
    #[test]
    fn embedded_assets_exposes_no_synthetic_fallback_entry() {
        for (name, bytes) in embedded_assets() {
            let embedded = Assets::get(&name)
                .unwrap_or_else(|| panic!("{name} is enumerated but not a real embedded file"));
            assert_eq!(
                embedded.data.as_ref(),
                bytes.as_ref(),
                "{name} is not the genuine embedded file's bytes"
            );
        }
        let index_count = enumerated_names()
            .iter()
            .filter(|name| *name == "index.html")
            .count();
        assert_eq!(index_count, 1, "index.html must appear exactly once");
    }

    /// 10. An unknown path is never enumerated, even though `serve_path` would
    ///     answer it via the SPA fallback.
    #[test]
    fn embedded_assets_does_not_enumerate_unknown_paths() {
        let unknown = "this/path/is/not/embedded.js";
        assert!(
            Assets::get(unknown).is_none(),
            "precondition: the probe path must not be a real asset"
        );
        assert!(
            !enumerated_names().iter().any(|name| name == unknown),
            "an unknown path must never be enumerated"
        );
        // ...even though serving it falls back to index.html (unchanged behavior).
        assert_eq!(serve_path(unknown).status(), StatusCode::OK);
    }
}
