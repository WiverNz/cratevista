//! Configuration diagnostics: a stable code, an actionable message, and a
//! location precise enough to jump to.
//!
//! Every diagnostic is **non-fatal to generation**: the valid parts of a
//! configuration still produce an overlay, and the document is still built. A
//! malformed file costs its own contents, never the whole run.
//!
//! Locations are **workspace-relative**, never absolute — the same rule the rest
//! of the tool follows, so a diagnostic can be pasted into an issue without
//! leaking the author's filesystem layout.

use std::fmt;
use std::ops::Range;

/// Stable diagnostic codes. These are part of the tool's contract: they appear
/// in `diagnostics.json` and users may match on them.
pub mod code {
    /// The file is not valid TOML.
    pub const PARSE_ERROR: &str = "config_parse_error";
    /// A required field is missing, or a value has the wrong type/shape.
    pub const INVALID_STRUCTURE: &str = "config_invalid_structure";
    /// Two `[[entity]]` blocks declare the same id anywhere in the config set.
    pub const DUPLICATE_ENTITY_ID: &str = "config_duplicate_entity_id";
    /// Two `[[flow]]` blocks declare the same id.
    pub const DUPLICATE_FLOW_ID: &str = "config_duplicate_flow_id";
    /// Two stages within one flow declare the same id, or the same order.
    pub const INVALID_STAGE: &str = "config_invalid_stage";
    /// A `manual:` reference names an entity no `[[entity]]` block declares.
    pub const UNKNOWN_MANUAL_REFERENCE: &str = "config_unknown_manual_reference";
    /// A required identifier/kind is empty or malformed.
    pub const INVALID_ID: &str = "config_invalid_id";
    /// The file could not be read.
    pub const READ_FAILED: &str = "config_read_failed";
    /// A `source` path is absolute, escapes the workspace, or is malformed.
    pub const INVALID_SOURCE_PATH: &str = "config_invalid_source_path";
    /// Two `[[flow.relation]]`s derive the same relation id (same kind, from, to
    /// and role), so one would be lost. Give one a distinct `role`.
    pub const DUPLICATE_RELATION: &str = "config_duplicate_relation";
    /// The same discovered entity is overridden more than once.
    pub const DUPLICATE_OVERRIDE: &str = "config_duplicate_override";
    /// A referenced doc/example path is absolute, traverses, or is malformed.
    pub const INVALID_FILE_PATH: &str = "config_invalid_file_path";
    /// A referenced doc/example file does not exist.
    pub const MISSING_FILE: &str = "config_missing_file";
    /// A referenced path is a directory or other non-file.
    pub const NOT_A_FILE: &str = "config_not_a_file";
    /// A referenced path resolves outside the workspace (e.g. via a symlink).
    pub const PATH_ESCAPES_WORKSPACE: &str = "config_path_escapes_workspace";
    /// A referenced doc file is not valid UTF-8.
    pub const NOT_UTF8: &str = "config_not_utf8";
    /// A referenced example file is not valid UTF-8.
    pub const EXAMPLE_NOT_UTF8: &str = "config_example_not_utf8";
    /// An example exceeds the embedded-content size cap and was dropped whole.
    pub const EXAMPLE_TOO_LARGE: &str = "config_example_too_large";
}

/// A 1-based source position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column, counted in characters (not bytes), so it lines up with
    /// what an editor shows for non-ASCII content.
    pub column: u32,
}

/// Resolves a byte offset into a 1-based line/column within `source`.
///
/// Returns `None` when the offset is out of range, so a bad span degrades to a
/// file-level diagnostic instead of panicking or reporting a fictional spot.
pub fn position_of(source: &str, byte_offset: usize) -> Option<Position> {
    if byte_offset > source.len() || !source.is_char_boundary(byte_offset) {
        return None;
    }
    let mut line = 1u32;
    let mut column = 1u32;
    for (offset, character) in source.char_indices() {
        if offset >= byte_offset {
            break;
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    Some(Position { line, column })
}

/// One configuration problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    /// A stable code from [`code`].
    pub code: &'static str,
    /// Human-readable, actionable description.
    pub message: String,
    /// The workspace-relative file it came from.
    pub file: String,
    /// Where in that file, when a span was available.
    pub position: Option<Position>,
}

impl ConfigDiagnostic {
    /// A file-level diagnostic (no position available).
    pub fn new(code: &'static str, message: impl Into<String>, file: impl Into<String>) -> Self {
        ConfigDiagnostic {
            code,
            message: message.into(),
            file: file.into(),
            position: None,
        }
    }

    /// Attaches a position resolved from a byte span, if it resolves.
    pub fn at(mut self, source: &str, span: Range<usize>) -> Self {
        self.position = position_of(source, span.start);
        self
    }

    /// Attaches an already-resolved position.
    pub fn at_position(mut self, position: Option<Position>) -> Self {
        self.position = position;
        self
    }
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.position {
            Some(Position { line, column }) => {
                write!(formatter, "{}:{line}:{column}: {}", self.file, self.message)
            }
            None => write!(formatter, "{}: {}", self.file, self.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_is_one_based_and_counts_lines() {
        let source = "a = 1\nb = 2\n";
        assert_eq!(
            position_of(source, 0),
            Some(Position { line: 1, column: 1 })
        );
        // Start of line 2.
        assert_eq!(
            position_of(source, 6),
            Some(Position { line: 2, column: 1 })
        );
        // Third character of line 2.
        assert_eq!(
            position_of(source, 8),
            Some(Position { line: 2, column: 3 })
        );
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        // A multi-byte identifier must not push the column past what an editor
        // shows: "é" is 2 bytes but 1 column.
        let source = "é = 1";
        let byte_offset = source.find('=').unwrap();
        assert_eq!(
            position_of(source, byte_offset),
            Some(Position { line: 1, column: 3 })
        );
    }

    #[test]
    fn an_out_of_range_or_misaligned_offset_degrades_to_none() {
        let source = "é";
        assert_eq!(position_of(source, 999), None);
        // Byte 1 is inside the two-byte 'é'.
        assert_eq!(position_of(source, 1), None);
    }

    #[test]
    fn display_includes_the_location_when_known() {
        let diagnostic =
            ConfigDiagnostic::new(code::INVALID_ID, "empty id", ".cratevista/flows/a.toml")
                .at_position(Some(Position { line: 3, column: 5 }));
        assert_eq!(
            diagnostic.to_string(),
            ".cratevista/flows/a.toml:3:5: empty id"
        );
        let bare = ConfigDiagnostic::new(code::READ_FAILED, "unreadable", "cratevista.toml");
        assert_eq!(bare.to_string(), "cratevista.toml: unreadable");
    }
}
