//! Public module 0 of crate 6.

/// Behaviour shared by the types in module 0.
pub trait Shape0 {
    /// Returns a stable identifier.
    fn id(&self) -> u32;
    /// Describes the value.
    fn describe(&self) -> String {
        format!("shape {}", self.id())
    }
}

/// A documented item (Item0_0).
pub struct Item0_0 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item0_0`].
pub enum State0_0 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item0_0 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item0_0 {
        Item0_0 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State0_0 {
        State0_0::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item0_0) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape0 for Item0_0 {
    fn id(&self) -> u32 {
        self.id
    }
}

pub struct Item0_1 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

pub enum State0_1 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item0_1 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item0_1 {
        Item0_1 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State0_1 {
        State0_1::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item0_1) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape0 for Item0_1 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// A documented item (Item0_2).
pub struct Item0_2 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item0_2`].
pub enum State0_2 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item0_2 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item0_2 {
        Item0_2 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State0_2 {
        State0_2::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item0_2) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape0 for Item0_2 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// A documented item (Item0_3).
pub struct Item0_3 {
    /// The numeric identifier.
    pub id: u32,
    /// A human-readable label.
    pub label: String,
}

/// The state of a [`Item0_3`].
pub enum State0_3 {
    /// Not yet started.
    Idle,
    /// Currently running.
    Running(u32),
    /// Finished.
    Done,
}

impl Item0_3 {
    /// Creates a new value.
    pub fn new(id: u32) -> Item0_3 {
        Item0_3 { id, label: String::new() }
    }
    /// Returns the current state.
    pub fn state(&self) -> State0_3 {
        State0_3::Running(self.id)
    }
    /// Accepts another item and merges it.
    pub fn merge(&mut self, other: &Item0_3) {
        self.id = self.id.wrapping_add(other.id);
    }
}

impl Shape0 for Item0_3 {
    fn id(&self) -> u32 {
        self.id
    }
}

/// Builds the first item of module 0.
pub fn build_0() -> Item0_0 {
    Item0_0::new(0)
}
