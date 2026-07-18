//! A deliberately uncompilable crate.
//!
//! This crate exists ONLY so that `cargo cratevista generate --keep-going`
//! encounters a failing rustdoc target and produces a genuinely partial
//! document. Do not "fix" it — the compile error is the point.

/// References a type that does not exist, so rustdoc fails on this target.
pub fn broken() -> ThisTypeDoesNotExist {
    unresolved_function_call()
}
