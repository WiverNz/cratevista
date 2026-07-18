//! Schema version marker.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The schema version (`MAJOR.MINOR`) of a serialized artifact.
///
/// A transparent string so `document.json` carries `"schema_version": "1.1"`.
///
/// Consumers gate on the **major** only: a reader that supports major `1` must
/// accept every `1.x` artifact, because minor bumps are additive by policy
/// (ADR-0003). `1.0` documents therefore keep loading unchanged.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct SchemaVersion(String);

impl SchemaVersion {
    /// The current schema version string.
    ///
    /// `1.1` added the optional [`View::docs`](crate::View::docs) and
    /// [`View::examples`](crate::View::examples) fields (PRD-08 Amendment A);
    /// both are additive, so `1.0` artifacts remain valid and readable.
    pub const CURRENT: &'static str = "1.1";

    /// Returns the current schema version.
    pub fn current() -> Self {
        SchemaVersion(Self::CURRENT.to_string())
    }

    /// The version as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SchemaVersion {
    fn default() -> Self {
        Self::current()
    }
}
