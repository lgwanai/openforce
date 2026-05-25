//! Platform-specific file locking abstractions.
//!
//! This module defines the [`PlatformLock`] trait and its error type,
//! providing a cross-platform interface for cross-process exclusive file
//! locking with configurable timeout support.
//!
//! # Architecture
//!
//! The [`PlatformLock`] trait abstracts over two backends:
//!
//! | Platform | Mechanism | Source |
//! |----------|-----------|--------|
//! | Unix     | `flock(2)` | `unix.rs` |
//! | Windows  | `LockFileEx` | `windows.rs` |
//!
//! # SRE Design
//!
//! - **Cross-process exclusivity**: Only one process can hold the lock on a
//!   given file at a time; other processes must fail or wait.
//! - **Timeout mechanism**: Lock acquisition accepts a configurable `Duration`
//!   after which a `TimedOut` error is returned.
//! - **Automatic cleanup**: The lock is released when the `FileLock` (or the
//!   underlying file descriptor / handle) is dropped.
//! - **No deadlock**: Acquisition uses non-blocking primitive with backoff
//!   sleep, so a single process cannot deadlock itself.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

// ---------------------------------------------------------------------------
// Public Re-exports
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod unix_impl;
#[cfg(windows)]
mod windows_impl;

#[cfg(unix)]
pub use unix_impl::UnixLock;

// ---------------------------------------------------------------------------
// LockError
// ---------------------------------------------------------------------------

/// Errors that can occur during platform lock operations.
///
/// Every variant provides enough context for structured logging and
/// operator debugging. The `Io` variant transparently wraps
/// `std::io::Error` for OS-level failures.
#[derive(Error, Debug)]
pub enum LockError {
    /// The lock is already held by another process and `try_lock` was used
    /// without blocking.
    #[error("lock would block; another process holds the lock")]
    WouldBlock,

    /// The lock could not be acquired within the configured timeout.
    #[error("timed out waiting for lock after {duration:?}")]
    TimedOut {
        /// The total duration we waited before giving up.
        duration: Duration,
    },

    /// An I/O error occurred while operating on the lock file.
    #[error("I/O error: {source}")]
    Io {
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The lock state is inconsistent (e.g., file descriptor closed
    /// unexpectedly).
    #[error("lock poisoned: {detail}")]
    Poisoned {
        /// Human-readable detail about the poisoned state.
        detail: String,
    },
}

// ---------------------------------------------------------------------------
// PlatformLock Trait
// ---------------------------------------------------------------------------

/// A cross-platform, cross-process exclusive file lock.
///
/// # Contract
///
/// 1. **Exclusivity**: At most one process may hold the lock on a given file
///    at any moment. The guarantee holds even if the lock-holding process
///    crashes (the OS kernel releases the lock automatically).
///
/// 2. **Reentrancy**: The behaviour of acquiring a lock already held by the
///    **same process** on the **same file descriptor** is platform-dependent.
///    On Unix via `flock`, a second `lock()` call on the same fd is a no-op
///    (recursive). Callers should avoid reentrant usage.
///
/// 3. **Timeout**: [`try_lock_with_timeout`] returns `Err(TimedOut)` if the
///    lock cannot be acquired within the specified duration.
///
/// 4. **Non-blocking fallback**: [`try_lock`] returns `Err(WouldBlock)`
///    immediately if the lock is held by another process.
///
/// # Lifetimes
///
/// The lock is valid for the lifetime of the implementing object. Dropping
/// the object releases the lock.
pub trait PlatformLock: Send {
    /// Acquires an exclusive lock, blocking until acquired.
    ///
    /// This method will block the calling thread until the lock is acquired
    /// or an I/O error occurs. On Unix, this delegates to `flock(fd, LOCK_EX)`.
    fn lock(&mut self) -> Result<(), LockError>;

    /// Attempts to acquire an exclusive lock without blocking.
    ///
    /// Returns:
    /// - `Ok(())` if the lock was acquired.
    /// - `Err(WouldBlock)` if the lock is held by another process.
    /// - `Err(Io { .. })` if an OS-level error occurs.
    fn try_lock(&mut self) -> Result<(), LockError>;

    /// Attempts to acquire the lock within the specified timeout.
    ///
    /// The implementation polls the lock in a loop with short sleeps,
    /// respecting the total timeout. Returns `Err(TimedOut { duration })`
    /// if the lock cannot be acquired within the deadline.
    ///
    /// # Parameters
    ///
    /// * `timeout` — Maximum wait duration. Zero means "try once" (same
    ///   as [`try_lock`]).
    fn try_lock_with_timeout(&mut self, timeout: Duration) -> Result<(), LockError>;

    /// Releases the lock.
    ///
    /// On Unix, this delegates to `flock(fd, LOCK_UN)`. The lock is also
    /// released automatically when the file descriptor is closed (i.e., when
    /// the implementing object is dropped).
    fn unlock(&mut self) -> Result<(), LockError>;
}

// ---------------------------------------------------------------------------
// Default retry strategy used by Unix backend
// ---------------------------------------------------------------------------

/// Default polling interval for lock retry loops (50 ms).
pub(crate) const LOCK_RETRY_SLEEP: Duration = Duration::from_millis(50);

/// Maximum number of consecutive immediate retries before yielding to the
/// OS scheduler.
pub(crate) const LOCK_SPIN_LIMIT: u32 = 10;
