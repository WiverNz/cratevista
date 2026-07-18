//! `--base-path` parsing and normalization (PRD 10, Decision 3).
//!
//! A [`BasePath`] is a **typed, already-validated** value: once parsed, its
//! normalized form cannot violate the contract, so downstream code never
//! re-checks it. Phase 2A only *parses and normalizes*; writing a `<base href>`
//! into `index.html` is Phase 2B and is not done here.

use super::error::BuildError;

/// A validated base path.
///
/// The normalized form is one of:
/// - **empty** — no base element (pure relative hosting);
/// - `"/"` — site root;
/// - `"/segment/…/"` — a subpath, always leading- and trailing-slashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasePath {
    /// The normalized value; see the type docs for the shape.
    normalized: String,
}

impl BasePath {
    /// Parses and normalizes a `--base-path` argument.
    ///
    /// Accepts `""`, `"/"`, `"repo"`, `"/repo"`, `"/repo/"`, `"a/b"`. Rejects a
    /// scheme, `//host`, a query or fragment, `..`, a backslash, a control
    /// character, or interior whitespace — each with `build_invalid_base_path`.
    pub fn parse(input: &str) -> Result<BasePath, BuildError> {
        // Rejections first, on the raw input, so nothing dangerous is normalized
        // away before it is seen.
        if input.contains('\\') {
            return Err(invalid("it contains a backslash"));
        }
        if input.chars().any(|c| c.is_control()) {
            return Err(invalid("it contains a control character"));
        }
        // Interior whitespace is refused; a value that is entirely whitespace is
        // treated as empty (no base) after the trim below rather than rejected,
        // but interior whitespace (e.g. "a b") is a mistake.
        let trimmed = input.trim();
        if trimmed.chars().any(char::is_whitespace) {
            return Err(invalid("it contains interior whitespace"));
        }
        if trimmed.contains('?') {
            return Err(invalid("it contains a query string"));
        }
        if trimmed.contains('#') {
            return Err(invalid("it contains a fragment"));
        }
        // A scheme (`http:`, `https:`, `file:`, `scheme:`) or a `//host` authority.
        if trimmed.starts_with("//") {
            return Err(invalid("it looks like a network path"));
        }
        if has_scheme(trimmed) {
            return Err(invalid("it looks like a URL with a scheme"));
        }

        // Empty (or all-whitespace) → no base element.
        if trimmed.is_empty() {
            return Ok(BasePath {
                normalized: String::new(),
            });
        }
        // Root.
        if trimmed == "/" {
            return Ok(BasePath {
                normalized: "/".to_string(),
            });
        }

        // Segment path. Split on `/`, drop empty segments (from leading/trailing
        // slashes), reject `.`/`..` and re-frame as `/a/b/`.
        let mut segments = Vec::new();
        for segment in trimmed.split('/') {
            if segment.is_empty() {
                continue;
            }
            if segment == ".." {
                return Err(invalid("it contains a `..` segment"));
            }
            if segment == "." {
                return Err(invalid("it contains a `.` segment"));
            }
            segments.push(segment);
        }
        if segments.is_empty() {
            // Only slashes, e.g. "//" was caught above; "/" handled above; a value
            // like "///" trims to "///" → all-empty segments → treat as root.
            return Ok(BasePath {
                normalized: "/".to_string(),
            });
        }
        Ok(BasePath {
            normalized: format!("/{}/", segments.join("/")),
        })
    }

    /// The normalized value (empty means "no base element").
    pub fn as_str(&self) -> &str {
        &self.normalized
    }

    /// Whether this base path adds no `<base>` element.
    pub fn is_empty(&self) -> bool {
        self.normalized.is_empty()
    }
}

fn invalid(reason: &'static str) -> BuildError {
    BuildError::InvalidBasePath { reason }
}

/// Whether `value` begins with a `scheme:` prefix (`ascii-letter` then
/// `[a-z0-9+.-]*` then `:`), which is what makes `http:`/`file:`/`mailto:` a URL.
fn has_scheme(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b':' => return index > 0,
            b if b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-') => {}
            _ => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(input: &str) -> String {
        BasePath::parse(input).expect("valid").as_str().to_string()
    }

    #[test]
    fn accepted_values_normalize_exactly() {
        assert_eq!(norm(""), "");
        assert_eq!(norm("/"), "/");
        assert_eq!(norm("repo"), "/repo/");
        assert_eq!(norm("/repo"), "/repo/");
        assert_eq!(norm("/repo/"), "/repo/");
        assert_eq!(norm("a/b"), "/a/b/");
    }

    #[test]
    fn empty_means_no_base_element() {
        assert!(BasePath::parse("").unwrap().is_empty());
        assert!(!BasePath::parse("/").unwrap().is_empty());
        assert!(!BasePath::parse("repo").unwrap().is_empty());
    }

    #[test]
    fn schemes_are_rejected() {
        for value in ["http://x", "https://x", "file:/x", "mailto:a", "scheme:x"] {
            assert!(
                matches!(
                    BasePath::parse(value),
                    Err(BuildError::InvalidBasePath { .. })
                ),
                "{value} must be rejected"
            );
        }
    }

    #[test]
    fn network_path_is_rejected() {
        assert!(matches!(
            BasePath::parse("//host/x"),
            Err(BuildError::InvalidBasePath { .. })
        ));
    }

    #[test]
    fn query_and_fragment_are_rejected() {
        assert!(matches!(
            BasePath::parse("/repo?x=1"),
            Err(BuildError::InvalidBasePath { .. })
        ));
        assert!(matches!(
            BasePath::parse("/repo#frag"),
            Err(BuildError::InvalidBasePath { .. })
        ));
    }

    #[test]
    fn traversal_is_rejected() {
        assert!(matches!(
            BasePath::parse("/a/../b"),
            Err(BuildError::InvalidBasePath { .. })
        ));
        assert!(matches!(
            BasePath::parse(".."),
            Err(BuildError::InvalidBasePath { .. })
        ));
    }

    #[test]
    fn backslash_is_rejected() {
        assert!(matches!(
            BasePath::parse("\\repo"),
            Err(BuildError::InvalidBasePath { .. })
        ));
        assert!(matches!(
            BasePath::parse("a\\b"),
            Err(BuildError::InvalidBasePath { .. })
        ));
    }

    #[test]
    fn control_characters_are_rejected() {
        assert!(matches!(
            BasePath::parse("/re\npo"),
            Err(BuildError::InvalidBasePath { .. })
        ));
        assert!(matches!(
            BasePath::parse("/re\tpo"),
            Err(BuildError::InvalidBasePath { .. })
        ));
    }

    #[test]
    fn interior_whitespace_is_rejected() {
        assert!(matches!(
            BasePath::parse("a b"),
            Err(BuildError::InvalidBasePath { .. })
        ));
        // Surrounding whitespace trims to empty rather than erroring.
        assert_eq!(norm("   "), "");
    }

    #[test]
    fn a_normalized_value_reparses_to_itself() {
        for value in ["", "/", "/repo/", "/a/b/"] {
            assert_eq!(norm(value), value);
        }
    }
}
