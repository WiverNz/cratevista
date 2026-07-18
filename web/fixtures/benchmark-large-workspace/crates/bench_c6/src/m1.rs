//! Public module 1 of crate 6.

/// Behaviour shared by the types in module 1.
pub trait Shape1 {
    /// Returns a stable identifier.
    fn id(&self) -> u32;
    /// Describes the value.
    fn describe(&self) -> String {
        format!("shape {}", self.id())
    }
}

/// A documented item (Item1_0).
pub struct Item1_0 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item1_0`].
pub enum State1_0 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item1_0 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item1_0 {
        Item1_0 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State1_0 {
        State1_0::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item1_0) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape1 for Item1_0 {
    fn id(&self) -> u32 {
        self.id
    }
}

pub struct Item1_1 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

pub enum State1_1 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item1_1 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item1_1 {
        Item1_1 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State1_1 {
        State1_1::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item1_1) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape1 for Item1_1 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// A documented item (Item1_2).
pub struct Item1_2 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item1_2`].
pub enum State1_2 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item1_2 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item1_2 {
        Item1_2 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State1_2 {
        State1_2::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item1_2) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape1 for Item1_2 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// A documented item (Item1_3).
pub struct Item1_3 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item1_3`].
pub enum State1_3 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item1_3 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item1_3 {
        Item1_3 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State1_3 {
        State1_3::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item1_3) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape1 for Item1_3 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// Builds the first item of module 1.
pub fn build_1() -> Item1_0 {
    Item1_0::new(1)
}
