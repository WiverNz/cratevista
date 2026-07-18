//! A small, deterministic path-pattern matcher for workspace-member globs.
//!
//! # Why this exists rather than a `glob` dependency
//!
//! `cratevista-watch` ships **`notify` and `tokio` only**. A glob crate would be
//! a third shipped dependency for a matcher this small, and — more to the point —
//! for one whose failure mode has to be chosen deliberately. See "Unsupported
//! syntax" below.
//!
//! # Supported syntax
//!
//! Matching is **component-wise** over `/`-separated, workspace-relative paths:
//!
//! | Syntax | Meaning |
//! | --- | --- |
//! | `crates/demo` | literal components |
//! | `*` | any run of characters **within one component** |
//! | `?` | exactly one character within one component |
//! | `[ab]` | one character from the set |
//! | `[!ab]` | one character **not** in the set |
//! | `**` | any number of components, including none |
//!
//! `*` never crosses a `/`: `crates/*` matches `crates/demo` and **not**
//! `crates/demo/nested`. Only `**` spans components.
//!
//! # Unsupported syntax fails closed
//!
//! A malformed pattern — an unterminated `[`, say — matches **nothing**. That is
//! the deliberate choice: the alternative failure mode is a pattern that quietly
//! broadens into "every `Cargo.toml` in the tree", which would make every
//! unrelated vendored manifest a workspace member and regenerate on files that
//! have nothing to do with the project. Watching too little is a missing feature;
//! watching everything is a wrong answer.

/// Whether `path` matches `pattern`.
///
/// Both are `/`-separated and workspace-relative. Empty patterns match nothing.
pub fn matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').filter(|part| !part.is_empty()).collect();
    let path: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if pattern.is_empty() {
        return false;
    }
    match_components(&pattern, &path)
}

/// Component-wise match, with `**` consuming any number of components.
fn match_components(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.first() {
        // Both exhausted: a match.
        None => path.is_empty(),
        Some(&"**") => {
            // `**` may consume nothing, or any prefix of the remaining path.
            for taken in 0..=path.len() {
                if match_components(&pattern[1..], &path[taken..]) {
                    return true;
                }
            }
            false
        }
        Some(head) => match path.first() {
            None => false,
            Some(part) => {
                match_component(head, part) && match_components(&pattern[1..], &path[1..])
            }
        },
    }
}

/// Whether one path component matches one pattern component.
///
/// Returns `false` for malformed syntax rather than guessing — see the module
/// docs on failing closed.
fn match_component(pattern: &str, part: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let part: Vec<char> = part.chars().collect();
    match_chars(&pattern, &part)
}

fn match_chars(pattern: &[char], part: &[char]) -> bool {
    match pattern.first() {
        None => part.is_empty(),
        Some('*') => {
            // `*` stays inside the component: the caller already split on `/`.
            for taken in 0..=part.len() {
                if match_chars(&pattern[1..], &part[taken..]) {
                    return true;
                }
            }
            false
        }
        Some('?') => !part.is_empty() && match_chars(&pattern[1..], &part[1..]),
        Some('[') => match class(pattern) {
            // Malformed class: match nothing rather than broaden.
            None => false,
            Some((set, negated, rest)) => {
                let Some(candidate) = part.first() else {
                    return false;
                };
                let inside = set.contains(candidate);
                (inside != negated) && match_chars(rest, &part[1..])
            }
        },
        Some(literal) => {
            !part.is_empty() && part[0] == *literal && match_chars(&pattern[1..], &part[1..])
        }
    }
}

/// Parses `[abc]` / `[!abc]`, returning the set, whether it is negated, and the
/// rest of the pattern. `None` when the class is unterminated.
fn class(pattern: &[char]) -> Option<(Vec<char>, bool, &[char])> {
    let mut index = 1;
    let negated = matches!(pattern.get(1), Some('!'));
    if negated {
        index += 1;
    }
    let mut set = Vec::new();
    while index < pattern.len() {
        match pattern[index] {
            ']' if !set.is_empty() => return Some((set, negated, &pattern[index + 1..])),
            character => set.push(character),
        }
        index += 1;
    }
    None
}

/// The literal prefix before the first pattern character.
///
/// `crates/*` → `crates`; `tools/*/plugins/*` → `tools`; `members/new` →
/// `members/new`. Core registers this prefix with the OS watcher — broadly — while
/// classification stays narrow.
pub fn static_prefix(pattern: &str) -> String {
    let mut prefix: Vec<&str> = Vec::new();
    for component in pattern.split('/').filter(|part| !part.is_empty()) {
        if component.contains(['*', '?', '[']) {
            break;
        }
        prefix.push(component);
    }
    prefix.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_components_match_exactly() {
        assert!(matches("crates/demo", "crates/demo"));
        assert!(!matches("crates/demo", "crates/other"));
        assert!(!matches("crates/demo", "crates/demo/nested"));
        assert!(!matches("crates/demo", "crates"));
    }

    #[test]
    fn a_star_stays_inside_one_component() {
        assert!(matches("crates/*", "crates/demo"));
        assert!(
            !matches("crates/*", "crates/demo/nested"),
            "`*` must never cross a separator — only `**` spans components"
        );
        assert!(!matches("crates/*", "other/demo"));
    }

    #[test]
    fn a_partial_star_matches_a_prefix_within_the_component() {
        // The case that makes the pattern predicate load-bearing.
        assert!(matches("crates/a*", "crates/api"));
        assert!(matches("crates/a*", "crates/a"));
        assert!(!matches("crates/a*", "crates/billing"));
    }

    #[test]
    fn a_question_mark_matches_exactly_one_character() {
        assert!(matches("crates/ap?", "crates/api"));
        assert!(!matches("crates/ap?", "crates/ap"));
        assert!(!matches("crates/ap?", "crates/apis"));
    }

    #[test]
    fn a_character_class_matches_one_of_its_members() {
        assert!(matches("crates/[ab]pi", "crates/api"));
        assert!(matches("crates/[ab]pi", "crates/bpi"));
        assert!(!matches("crates/[ab]pi", "crates/cpi"));
    }

    #[test]
    fn a_negated_class_excludes_its_members() {
        assert!(matches("crates/[!ab]pi", "crates/cpi"));
        assert!(!matches("crates/[!ab]pi", "crates/api"));
    }

    #[test]
    fn a_double_star_spans_components_including_none() {
        assert!(matches("crates/**", "crates"));
        assert!(matches("crates/**", "crates/demo"));
        assert!(matches("crates/**", "crates/demo/nested/deep"));
        assert!(!matches("crates/**", "other/demo"));
        assert!(matches("**/demo", "crates/inner/demo"));
        assert!(matches("**/demo", "demo"));
    }

    #[test]
    fn tools_star_plugins_star_matches_only_that_shape() {
        assert!(matches("tools/*/plugins/*", "tools/a/plugins/b"));
        assert!(!matches("tools/*/plugins/*", "tools/a/plugins"));
        assert!(!matches("tools/*/plugins/*", "tools/a/b/plugins/c"));
    }

    #[test]
    fn a_malformed_pattern_matches_nothing_rather_than_everything() {
        // Failing closed is the whole point: the alternative is a pattern that
        // quietly becomes "every Cargo.toml in the tree".
        assert!(!matches("crates/[unterminated", "crates/anything"));
        assert!(!matches("crates/[unterminated", "crates/[unterminated"));
        assert!(!matches("", "crates/demo"));
    }

    #[test]
    fn an_empty_class_is_malformed_and_matches_nothing() {
        assert!(!matches("crates/[]", "crates/x"));
    }

    #[test]
    fn the_static_prefix_stops_at_the_first_pattern_character() {
        assert_eq!(static_prefix("crates/*"), "crates");
        assert_eq!(static_prefix("crates/a*"), "crates");
        assert_eq!(static_prefix("tools/*/plugins/*"), "tools");
        assert_eq!(static_prefix("members/new"), "members/new");
        assert_eq!(static_prefix("**/demo"), "");
        assert_eq!(static_prefix("crates/[ab]pi"), "crates");
        assert_eq!(static_prefix("crates/ap?"), "crates");
    }
}
