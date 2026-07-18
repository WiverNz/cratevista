//! Documentation blocks attached to entities.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A rustdoc/Markdown documentation block for an entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DocBlock {
    /// The full documentation as Markdown.
    pub markdown: String,
    /// An optional short summary (e.g. the first paragraph).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Whether the underlying item is documented (used for coverage).
    pub documented: bool,
}
