//! `cvapp` — depends on `cvcore` to exercise crate-dependency and cross-crate
//! type-reference relations.

use cvcore::{Render, Widget};

/// A documented service that wraps a core `Widget`.
pub struct Service {
    widget: Widget,
}

impl Service {
    /// Builds a service from a `Widget` (accepts the cross-crate type).
    pub fn new(widget: Widget) -> Self {
        Service { widget }
    }

    /// Describes the wrapped widget (returns a `String`).
    pub fn describe(&self) -> String {
        self.widget.render()
    }
}
