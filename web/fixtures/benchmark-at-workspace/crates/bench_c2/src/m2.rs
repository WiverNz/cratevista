//! Internal module 2 of crate 2.

/// Behaviour shared by the types in module 2.
pub trait Shape2 {
    /// Returns a stable identifier.
    fn id(&self) -> u32;
    /// Describes the value.
    fn describe(&self) -> String {
        format!("shape {}", self.id())
    }
}

/// A documented item (Item2_0).
pub struct Item2_0 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item2_0`].
pub enum State2_0 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item2_0 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item2_0 {
        Item2_0 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State2_0 {
        State2_0::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item2_0) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape2 for Item2_0 {
    fn id(&self) -> u32 {
        self.id
    }
}

pub struct Item2_1 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

pub enum State2_1 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item2_1 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item2_1 {
        Item2_1 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State2_1 {
        State2_1::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item2_1) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape2 for Item2_1 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// A documented item (Item2_2).
pub struct Item2_2 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item2_2`].
pub enum State2_2 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item2_2 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item2_2 {
        Item2_2 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State2_2 {
        State2_2::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item2_2) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape2 for Item2_2 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// A documented item (Item2_3).
pub struct Item2_3 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item2_3`].
pub enum State2_3 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item2_3 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item2_3 {
        Item2_3 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State2_3 {
        State2_3::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item2_3) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape2 for Item2_3 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// Builds the first item of module 2.
pub fn build_2() -> Item2_0 {
    Item2_0::new(2)
}
