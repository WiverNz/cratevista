//! Structured, user-facing diagnostics rendered as human text or JSON.
//!
//! This is the *runtime* diagnostic used by the CLI. The schema-embedded
//! diagnostic (issue 02) is a separate but field-aligned type; both share the
//! `severity` / `code` / `message` core fields.

use std::fmt;

use serde::Serialize;

/// Severity of a [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A fatal problem.
    Error,
    /// A non-fatal problem worth surfacing.
    Warning,
    /// Informational context.
    Info,
}

impl Severity {
    fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// A structured, user-facing message.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    /// Severity of the message.
    pub severity: Severity,
    /// A short, stable machine-readable code (e.g. `"unimplemented"`).
    pub code: String,
    /// The human-readable message.
    pub message: String,
    /// Optional actionable remediation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// Optional `(key, value)` context pairs.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<(String, String)>,
}

impl Diagnostic {
    fn new(severity: Severity, code: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic {
            severity,
            code: code.into(),
            message: message.into(),
            remediation: None,
            context: Vec::new(),
        }
    }

    /// Creates an error diagnostic.
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic::new(Severity::Error, code, message)
    }

    /// Creates a warning diagnostic.
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic::new(Severity::Warning, code, message)
    }

    /// Creates an informational diagnostic.
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic::new(Severity::Info, code, message)
    }

    /// Attaches actionable remediation guidance.
    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = Some(remediation.into());
        self
    }

    /// Attaches a `(key, value)` context pair.
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.push((key.into(), value.into()));
        self
    }

    /// Serializes the diagnostic to a single-line JSON object.
    pub fn to_json(&self) -> String {
        // Serialization of a plain struct of strings cannot fail.
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"))
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}[{}]: {}",
            self.severity.label(),
            self.code,
            self.message
        )?;
        for (key, value) in &self.context {
            write!(f, "\n  {key}: {value}")?;
        }
        if let Some(remediation) = &self.remediation {
            write!(f, "\n  help: {remediation}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_rendering_includes_all_parts() {
        let diagnostic = Diagnostic::error("unimplemented", "not implemented yet")
            .with_context("command", "generate")
            .with_remediation("try later");
        let rendered = diagnostic.to_string();
        assert!(rendered.contains("error[unimplemented]: not implemented yet"));
        assert!(rendered.contains("command: generate"));
        assert!(rendered.contains("help: try later"));
    }

    #[test]
    fn json_rendering_round_trips_fields() {
        let diagnostic = Diagnostic::warning("no_nightly", "nightly missing");
        let json = diagnostic.to_json();
        assert!(json.contains("\"severity\":\"warning\""));
        assert!(json.contains("\"code\":\"no_nightly\""));
        assert!(json.contains("\"message\":\"nightly missing\""));
        // Optional fields are omitted when empty.
        assert!(!json.contains("remediation"));
        assert!(!json.contains("context"));
    }
}
