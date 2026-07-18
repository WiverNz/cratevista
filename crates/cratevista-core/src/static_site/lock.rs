//! The per-output advisory lock (PRD 10, Decision 2).
//!
//! A non-blocking **OS advisory** lock (flock / `LockFileEx`) on
//! `.cratevista-<output_key>.lock`, held for the whole build. Contention →
//! `build_output_busy`. Released on `Drop` **and** on process termination (the OS
//! drops the lock when the file handle closes), so a crash never leaves a
//! permanent lockout — this is why an OS advisory lock is used rather than a
//! `create_new` lock file.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs4::FileExt;
use fs4::TryLockError;

use super::error::BuildError;

/// A held per-output advisory lock. Dropping it releases the lock.
#[derive(Debug)]
pub struct OutputLock {
    // The order matters: `file` is dropped (closing the handle, releasing the OS
    // lock) after the explicit `unlock` in `Drop`.
    file: File,
    #[allow(dead_code)]
    path: PathBuf,
}

impl OutputLock {
    /// Acquires the lock for `output_key`, with the lock file living in `dir`
    /// (the output's parent).
    ///
    /// - contention (another live process holds it) → `build_output_busy`;
    /// - an **existing unlocked** lock file is reused (the file is opened, not
    ///   created-new), so a leftover from a previous run never blocks;
    /// - no stale-PID guessing.
    pub fn acquire(dir: &Path, output_key: &str) -> Result<OutputLock, BuildError> {
        let path = dir.join(format!(".cratevista-{output_key}.lock"));
        // `create(true)` opens an existing file or makes a new one — it is NOT
        // `create_new`, so a leftover unlocked lock file is reused, not an error.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|_| BuildError::Filesystem {
                context: "lock-open",
            })?;

        match FileExt::try_lock(&file) {
            Ok(()) => Ok(OutputLock { file, path }),
            Err(TryLockError::WouldBlock) => Err(BuildError::OutputBusy),
            Err(TryLockError::Error(_)) => Err(BuildError::Filesystem { context: "lock" }),
        }
    }

    /// Acquires the lock, runs `body` while holding it, then releases.
    ///
    /// On contention it returns `build_output_busy` **before** `body` is called, so
    /// no mutation hook ever runs without the lock.
    pub fn with<T>(
        dir: &Path,
        output_key: &str,
        body: impl FnOnce(&OutputLock) -> T,
    ) -> Result<T, BuildError> {
        let guard = OutputLock::acquire(dir, output_key)?;
        let value = body(&guard);
        drop(guard);
        Ok(value)
    }
}

impl Drop for OutputLock {
    fn drop(&mut self) {
        // Explicit release; closing the handle would release it anyway (that is the
        // crash-safety property), but unlocking first is tidy.
        let _ = FileExt::unlock(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tempfile::TempDir;

    #[test]
    fn first_guard_acquires_second_for_same_key_is_busy() {
        let dir = TempDir::new().unwrap();
        let key = "abcdef0123456789";
        let first = OutputLock::acquire(dir.path(), key).expect("first acquires");
        assert!(
            matches!(
                OutputLock::acquire(dir.path(), key),
                Err(BuildError::OutputBusy)
            ),
            "a second guard for the same key must be busy"
        );
        drop(first);
    }

    #[test]
    fn dropping_the_first_permits_reacquisition() {
        let dir = TempDir::new().unwrap();
        let key = "abcdef0123456789";
        {
            let _first = OutputLock::acquire(dir.path(), key).unwrap();
        } // released here
        let second = OutputLock::acquire(dir.path(), key);
        assert!(
            second.is_ok(),
            "the lock must be reacquirable after release"
        );
    }

    #[test]
    fn a_preexisting_unlocked_lock_file_does_not_block() {
        let dir = TempDir::new().unwrap();
        let key = "abcdef0123456789";
        // A leftover, unlocked lock file (as a crashed process would leave).
        std::fs::write(dir.path().join(format!(".cratevista-{key}.lock")), b"").unwrap();
        assert!(
            OutputLock::acquire(dir.path(), key).is_ok(),
            "a leftover unlocked lock file must be reusable"
        );
    }

    #[test]
    fn different_keys_in_the_same_parent_do_not_block() {
        let dir = TempDir::new().unwrap();
        let a = OutputLock::acquire(dir.path(), "aaaaaaaaaaaaaaaa").unwrap();
        let b = OutputLock::acquire(dir.path(), "bbbbbbbbbbbbbbbb");
        assert!(b.is_ok(), "different output keys use different locks");
        drop(a);
    }

    #[test]
    fn contention_occurs_before_any_body_runs() {
        let dir = TempDir::new().unwrap();
        let key = "abcdef0123456789";
        let _held = OutputLock::acquire(dir.path(), key).unwrap();
        let ran = AtomicBool::new(false);
        let result = OutputLock::with(dir.path(), key, |_| ran.store(true, Ordering::SeqCst));
        assert_eq!(result, Err(BuildError::OutputBusy));
        assert!(
            !ran.load(Ordering::SeqCst),
            "the body must not run when the lock is contended"
        );
    }

    #[test]
    fn with_runs_the_body_and_releases() {
        let dir = TempDir::new().unwrap();
        let key = "abcdef0123456789";
        let value = OutputLock::with(dir.path(), key, |_| 42).unwrap();
        assert_eq!(value, 42);
        // Released: a fresh acquire succeeds.
        assert!(OutputLock::acquire(dir.path(), key).is_ok());
    }
}
