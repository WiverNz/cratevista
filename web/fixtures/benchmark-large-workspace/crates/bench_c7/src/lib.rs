//! Benchmark crate 7 for the CrateVista large-graph benchmark.

pub mod m0;
pub mod m1;
mod m2;
pub mod m3;

/// Bridges to the previous crate, creating a cross-crate type reference.
pub fn bridge() -> bench_c6::m0::Item0_0 {
    bench_c6::m0::build_0()
}

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
