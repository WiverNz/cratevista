//! The two controlled `index.html` edits for a static site (PRD 10, Decisions 3–4).
//!
//! A static build makes exactly two narrow, validated insertions into the embedded
//! `index.html`, both inside `<head>` and both idempotent-by-rejection:
//!
//! - a **static-mode marker** `<meta name="cratevista-mode" content="static" />`, so
//!   the frontend loads from files and never opens an `EventSource` or hits `/api`;
//! - for a non-empty base path, exactly one `<base href="…" />`, placed **before**
//!   any URL-bearing element so HTML base resolution is deterministic.
//!
//! No full HTML parser is used: the bundle is a known, controlled Vite document, so
//! two string insertions anchored to the single validated `<head>` boundary are
//! safe and cannot silently corrupt minified JS/CSS (which are never touched).

use super::base_path::BasePath;
use super::error::BuildError;

/// The static-mode marker inserted into `<head>`.
const MODE_META: &str = r#"<meta name="cratevista-mode" content="static" />"#;
/// The attribute that marks an already-transformed document.
const MODE_MARKER_ATTR: &str = r#"name="cratevista-mode""#;

/// Applies the static-site head transformations to `html`.
///
/// Always inserts the static-mode marker exactly once. When `base` is present and
/// non-empty, also inserts exactly one `<base href>`. Returns
/// `build_filesystem_error` (the internal defensive code — this is a controlled
/// bundle, never user input) if the document is already transformed, or has a
/// missing/duplicate/malformed `<head>` boundary.
pub fn transform_index_html(html: &str, base: Option<&BasePath>) -> Result<String, BuildError> {
    let lower = html.to_ascii_lowercase();

    // Reject an already-transformed document (idempotency by rejection).
    if lower.contains(&MODE_MARKER_ATTR.to_ascii_lowercase()) {
        return Err(internal("index-already-transformed"));
    }

    let insertion_point = head_open_end(&lower)?;

    // Build the injected block. The base element goes first so it precedes every
    // URL-bearing element; the mode marker follows.
    let mut injected = String::new();
    if let Some(base) = base
        && !base.is_empty()
    {
        injected.push_str(&format!(r#"<base href="{}" />"#, base.as_str()));
    }
    injected.push_str(MODE_META);

    let mut out = String::with_capacity(html.len() + injected.len());
    out.push_str(&html[..insertion_point]);
    out.push_str(&injected);
    out.push_str(&html[insertion_point..]);
    Ok(out)
}

/// The byte offset just past the single `<head …>` opening tag, validating that
/// exactly one `<head` and one `</head>` boundary exist.
fn head_open_end(lower: &str) -> Result<usize, BuildError> {
    // Exactly one opening and one closing head boundary.
    if lower.matches("<head").count() != 1 || lower.matches("</head>").count() != 1 {
        return Err(internal("index-head-boundary"));
    }
    let open = lower
        .find("<head")
        .ok_or_else(|| internal("index-head-boundary"))?;
    // The opening tag ends at the first '>' after "<head". A '<' in the attribute
    // region (between "<head" and that '>') means a malformed/nested boundary.
    let after = &lower[open..];
    let close_rel = after
        .find('>')
        .ok_or_else(|| internal("index-head-boundary"))?;
    if after["<head".len()..close_rel].contains('<') {
        return Err(internal("index-head-boundary"));
    }
    Ok(open + close_rel + 1)
}

fn internal(context: &'static str) -> BuildError {
    BuildError::Filesystem { context }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(value: &str) -> BasePath {
        BasePath::parse(value).unwrap()
    }

    const DOC: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<link rel=\"stylesheet\" href=\"./assets/index.abc12345.css\"></head>\
<body><script src=\"./assets/index.def67890.js\"></script></body></html>";

    #[test]
    fn inserts_the_static_mode_marker_exactly_once() {
        let out = transform_index_html(DOC, None).unwrap();
        assert_eq!(out.matches(MODE_META).count(), 1);
        assert!(out.contains(r#"name="cratevista-mode""#));
    }

    #[test]
    fn no_base_element_without_a_base_path() {
        let out = transform_index_html(DOC, None).unwrap();
        assert!(!out.contains("<base"), "{out}");
        // And an explicitly empty base path also inserts none.
        let out_empty = transform_index_html(DOC, Some(&base(""))).unwrap();
        assert!(!out_empty.contains("<base"), "{out_empty}");
    }

    #[test]
    fn inserts_exactly_one_base_element_before_url_bearing_elements() {
        let out = transform_index_html(DOC, Some(&base("/demo/"))).unwrap();
        assert_eq!(out.matches("<base ").count(), 1);
        assert!(out.contains(r#"<base href="/demo/" />"#));
        // The base element precedes the stylesheet link and the script.
        let base_at = out.find("<base ").unwrap();
        assert!(base_at < out.find("<link").unwrap());
        assert!(base_at < out.find("<script").unwrap());
        // ...and sits inside <head>.
        assert!(base_at > out.find("<head>").unwrap());
        assert!(base_at < out.find("</head>").unwrap());
    }

    #[test]
    fn relative_asset_references_are_untouched() {
        let out = transform_index_html(DOC, Some(&base("/demo/"))).unwrap();
        assert!(out.contains(r#"href="./assets/index.abc12345.css""#));
        assert!(out.contains(r#"src="./assets/index.def67890.js""#));
    }

    #[test]
    fn rejects_an_already_transformed_document() {
        let once = transform_index_html(DOC, None).unwrap();
        assert!(matches!(
            transform_index_html(&once, None),
            Err(BuildError::Filesystem { .. })
        ));
    }

    #[test]
    fn rejects_a_missing_or_duplicate_head_boundary() {
        assert!(matches!(
            transform_index_html("<html><body></body></html>", None),
            Err(BuildError::Filesystem { .. })
        ));
        assert!(matches!(
            transform_index_html("<head></head><head></head>", None),
            Err(BuildError::Filesystem { .. })
        ));
    }

    #[test]
    fn handles_a_head_with_attributes() {
        let doc = "<html><head lang=\"en\"><title>x</title></head><body></body></html>";
        let out = transform_index_html(doc, Some(&base("/r/"))).unwrap();
        // Injected right after the opening <head lang="en"> tag, before <title>.
        assert!(
            out.contains(r#"<head lang="en"><base href="/r/" />"#),
            "{out}"
        );
        assert!(out.find("<base ").unwrap() < out.find("<title>").unwrap());
    }

    #[test]
    fn does_not_modify_minified_body_script() {
        let doc = "<head></head><body><script>var a=1;let b=2;</script></body>";
        let out = transform_index_html(doc, None).unwrap();
        assert!(out.contains("<script>var a=1;let b=2;</script>"));
    }
}
