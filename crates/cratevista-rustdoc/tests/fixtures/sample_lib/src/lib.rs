//! Sample library fixture for CrateVista rustdoc-adapter tests.
//!
//! Path-only crate with no external dependencies. Exercises modules, structs,
//! enums, unions, traits, impls, functions, methods, fields, variants, type
//! aliases, consts, statics, macros, re-exports, and private items.

/// A greeter that stores a name.
pub struct Greeter {
    /// The name to greet.
    pub name: String,
}

impl Greeter {
    /// Creates a new greeter.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Returns a greeting string.
    pub fn greet(&self) -> Greeting {
        Greeting::Hello
    }

    /// A private helper (only present with --document-private-items).
    fn private_len(&self) -> usize {
        self.name.len()
    }
}

/// Something that can be greeted.
pub trait Greetable {
    /// Produces the greeting.
    fn greeting(&self) -> Greeting;
}

impl Greetable for Greeter {
    fn greeting(&self) -> Greeting {
        self.greet()
    }
}

/// The kind of greeting.
pub enum Greeting {
    /// A plain hello.
    Hello,
    /// A named greeting.
    Named {
        /// Who is greeted.
        who: String,
    },
}

/// A fallible operation returning a domain error.
pub fn try_build(name: &str) -> Result<Greeter, BuildError> {
    if name.is_empty() {
        Err(BuildError::Empty)
    } else {
        Ok(Greeter::new(name))
    }
}

/// An error building a [`Greeter`].
pub enum BuildError {
    /// The name was empty.
    Empty,
}

/// A pairing of a greeter and its greeting (intra-crate field types).
pub struct Pair {
    /// The greeter.
    pub greeter: Greeter,
    /// The chosen greeting.
    pub greeting: Greeting,
}

/// Describes a pair (accepts an intra-crate type).
pub fn describe(pair: &Pair) -> u32 {
    pair.greeter.name.len() as u32
}

/// A free function returning a constant.
pub fn answer() -> u32 {
    42
}

/// A public type alias.
pub type Name = String;

/// A public constant.
pub const GREETING: &str = "Hi";

/// A public static.
pub static COUNT: u32 = 0;

/// A public module.
pub mod util {
    /// Doubles a value.
    pub fn double(x: u32) -> u32 {
        x * 2
    }

    /// A nested struct re-exported at the crate root.
    pub struct Helper;
}

/// Re-export of a nested item at the crate root.
pub use util::Helper;

/// A declarative macro.
#[macro_export]
macro_rules! shout {
    ($e:expr) => {
        format!("{}!", $e)
    };
}
