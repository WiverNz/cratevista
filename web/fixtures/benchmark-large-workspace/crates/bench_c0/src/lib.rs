//! Benchmark crate 0 for the CrateVista large-graph benchmark.

pub mod m0;
pub mod m1;
mod m2;
pub mod m3;

/// The crate's entry type.
pub struct Root {
    /// How many modules this crate exposes.
    pub modules: u32,
}

impl Root {
    /// Creates the root.
    pub fn new() -> Root {
        Root { modules: 4 }
    }
}

impl Default for Root {
    fn default() -> Root {
        Root::new()
    }
}
