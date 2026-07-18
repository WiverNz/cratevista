//! Public module 3 of crate 6.

/// Behaviour shared by the types in module 3.
pub trait Shape3 {
    /// Returns a stable identifier.
    fn id(&self) -> u32;
    /// Describes the value.
    fn describe(&self) -> String {
        format!("shape {}", self.id())
    }
}

/// A documented item (Item3_0).
pub struct Item3_0 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item3_0`].
pub enum State3_0 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item3_0 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item3_0 {
        Item3_0 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State3_0 {
        State3_0::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item3_0) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape3 for Item3_0 {
    fn id(&self) -> u32 {
        self.id
    }
}

pub struct Item3_1 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

pub enum State3_1 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item3_1 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item3_1 {
        Item3_1 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State3_1 {
        State3_1::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item3_1) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape3 for Item3_1 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// A documented item (Item3_2).
pub struct Item3_2 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item3_2`].
pub enum State3_2 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item3_2 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item3_2 {
        Item3_2 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State3_2 {
        State3_2::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item3_2) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape3 for Item3_2 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// A documented item (Item3_3).
pub struct Item3_3 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item3_3`].
pub enum State3_3 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item3_3 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item3_3 {
        Item3_3 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State3_3 {
        State3_3::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item3_3) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape3 for Item3_3 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// Builds the first item of module 3.
pub fn build_3() -> Item3_0 {
    Item3_0::new(3)
}
