//! Public model types (documented + undocumented, structs/enums/traits/impls).

/// A documented widget with an id and a name.
pub struct Widget {
    /// Stable identifier.
    pub id: u32,
    /// Human-readable name.
    pub name: String,
}

/// A documented color enum.
pub enum Color {
    /// Red.
    Red,
    /// Green.
    Green,
    /// Blue.
    Blue,
}

// A public-but-undocumented struct (drives documentation-coverage).
pub struct Marker;

/// A documented rendering trait.
pub trait Render {
    /// Renders the value to a string.
    fn render(&self) -> String;
}

impl Render for Widget {
    fn render(&self) -> String {
        self.name.clone()
    }
}

impl Widget {
    /// Creates a new widget (documented public method).
    pub fn new(id: u32, name: String) -> Self {
        Widget { id, name }
    }

    /// Returns the widget's color (references the `Color` type).
    pub fn color(&self) -> Color {
        let _ = crate::internal::seed();
        Color::Red
    }
}
