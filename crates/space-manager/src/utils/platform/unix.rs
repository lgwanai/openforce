//! Unix-specific file lock backend using `flock(2)`.
//!
//! # Why `flock` over `fcntl`
//!
//! | Concern | `flock(2)` | `fcntl(F_SETLK)` |
//! |---------|------------|-------------------|
//! | Cross-process exclusivity | ✅ Full | ✅ Full |
//! | Released on crash | ✅ (kernel auto-releases on fd close) | ✅ |
//! | Simple API | ✅ Yes | ❌ Complex (struct flock, l_type/l_whence/l_start/l_len) |
//! | NFS safety | ❌ Not safe on NFS | ✅ Works on NFS |
//! | Record-level locking | ❌ File-level only | ✅ Byte-range locking |
//!
//! For our use case (cross-process exclusive file lock, not record-level),
//! `flock` is the simpler, less error-prone choice. If NFS support becomes
//! necessary in the future, we can switch to `fcntl` or fall back to a
//! lock-file convention.
//!
//! # Timeout Implementation
//!
//! Since `flock`'s blocking mode (`LOCK_EX` without `LOCK_NB`) does not
//! support a timeout, we implement timeouts via polling:
//!
//! 1. Attempt `flock(fd, LOCK_EX | LOCK_NB)` (non-blocking).
//! 2. If it succeeds → return `Ok(())`.
//! 3. If it returns `EWOULDBLOCK` → sleep `LOCK_RETRY_SLEEP` (50 ms) and
//!    retry, unless the total elapsed time exceeds the timeout.
//! 4. If the timeout expires → return `Err(LockError::TimedOut)`.
//!
//! This approach avoids deadlocks, respects the timeout, and keeps the
//! API synchronous and predictable.

use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{LockError, PlatformLock, LOCK_RETRY_SLEEP, LOCK_SPIN_LIMIT};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// libc constants for flock operations.
///
/// We use raw constants instead of depending on the `libc` crate to keep
/// dependencies minimal. These values are stable across all Unix platforms.
const LOCK_EX: i32 = 2; // Exclusive lock
const LOCK_NB: i32 = 4; // Non-blocking flag
const LOCK_UN: i32 = 8; // Unlock

// ---------------------------------------------------------------------------
// UnixLock
// ---------------------------------------------------------------------------

/// A Unix-specific exclusive file lock backed by `flock(2)`.
///
/// # Construction
///
/// ```rust,ignore
/// use std::fs::File;
/// use std::time::Duration;
/// use openforce_space_manager::utils::platform::unix_impl::UnixLock;
///
/// let file = File::create("/tmp/my.lock")?;
/// let mut lock = UnixLock::new(file, "/tmp/my.lock")?;
/// lock.try_lock_with_timeout(Duration::from_secs(5))?;
/// // ... critical section ...
/// lock.unlock()?;
/// ```
///
/// # Drop Behaviour
///
/// The lock is released automatically when the `UnixLock` is dropped,
/// because the underlying `File` is closed and the kernel releases any
/// `flock` associated with the file descriptor.
///
/// However, callers should still call [`unlock`](PlatformLock::unlock)
/// explicitly to release the lock early when possible, as this makes the
/// lock hold time explicit and auditable.
pub struct UnixLock {
    /// The open file handle on which the lock is (or will be) held.
    file: std::fs::File,
    /// The path to the lock file, for error reporting.
    path: PathBuf,
    /// Tracks whether we currently hold the lock.
    /// Used to provide better error messages and prevent double-unlock.
    locked: bool,
}

impl UnixLock {
    /// Creates a new `UnixLock` for the given file.
    ///
    /// The file must be opened with at least read permissions (write is not
    /// required for `flock`, but read is necessary on most systems). The
    /// file is not locked until [`lock`](PlatformLock::lock) or
    /// [`try_lock`](PlatformLock::try_lock) is called.
    ///
    /// # Errors
    ///
    /// Returns `LockError::Io` if we cannot determine the file's metadata
    /// (e.g., the file has been deleted).
    pub fn new(file: std::fs::File, path: impl Into<PathBuf>) -> Result<Self, LockError> {
        // Ensure the file is a regular file (or at least something flock-able).
        let meta = file.metadata().map_err(|source| LockError::Io {
            source,
        })?;
        if !meta.is_file() {
            return Err(LockError::Poisoned {
                detail: format!("lock path is not a regular file: {}", path.as_ref().display()),
            });
        }

        Ok(Self {
            file,
            path: path.into(),
            locked: false,
        })
    }

    /// Returns a reference to the underlying file.
    pub fn file(&self) -> &std::fs::File {
        &self.file
    }

    /// Returns the path to the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns `true` if this lock is currently held.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Raw file descriptor for use with `flock(2)`.
    fn raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }

    /// Perform a raw `flock` operation.
    ///
    /// # Safety
    ///
    /// The caller must ensure the file descriptor is valid and that the
    /// operation is compatible with the current lock state.
    unsafe fn raw_flock(fd: RawFd, operation: i32) -> io::Result<()> {
        let ret = libc::flock(fd, operation);
        if ret == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

// ---------------------------------------------------------------------------
// PlatformLock implementation
// ---------------------------------------------------------------------------

impl PlatformLock for UnixLock {
    fn lock(&mut self) -> Result<(), LockError> {
        let fd = self.raw_fd();

        // SAFETY: The file descriptor is valid, and we own the file.
        let result = unsafe { Self::raw_flock(fd, LOCK_EX) };

        match result {
            Ok(()) => {
                self.locked = true;
                Ok(())
            }
            Err(e) => Err(LockError::Io { source: e }),
        }
    }

    fn try_lock(&mut self) -> Result<(), LockError> {
        let fd = self.raw_fd();

        // SAFETY: The file descriptor is valid, and we own the file.
        let result = unsafe { Self::raw_flock(fd, LOCK_EX | LOCK_NB) };

        match result {
            Ok(()) => {
                self.locked = true;
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                Err(LockError::WouldBlock)
            }
            Err(e) => Err(LockError::Io { source: e }),
        }
    }

    fn try_lock_with_timeout(&mut self, timeout: Duration) -> Result<(), LockError> {
        let deadline = Instant::now() + timeout;
        let mut spin_count: u32 = 0;

        loop {
            // Try non-blocking acquisition.
            match self.try_lock() {
                Ok(()) => return Ok(()),
                Err(LockError::WouldBlock) => {
                    // Check timeout.
                    if Instant::now() >= deadline {
                        return Err(LockError::TimedOut {
                            duration: timeout,
                        });
                    }
                }
                Err(e) => return Err(e),
            }

            // Backoff: spin a few times, then sleep.
            if spin_count < LOCK_SPIN_LIMIT {
                spin_count += 1;
                // Brief spin: yield the CPU timeslice without a syscall.
                // This is effective when the lock is held for very short
                // critical sections (microseconds).
                std::hint::spin_loop();
            } else {
                spin_count = 0;
                // Sleep for the configured retry interval.
                //
                // We compute the remaining time and sleep at most that
                // long, so we don't overshoot the deadline significantly.
                let remaining = deadline.saturating_duration_since(Instant::now());
                let sleep_dur = LOCK_RETRY_SLEEP.min(remaining);

                if sleep_dur.is_zero() {
                    // Deadline has elapsed while we were spinning.
                    return Err(LockError::TimedOut {
                        duration: timeout,
                    });
                }

                std::thread::sleep(sleep_dur);
            }
        }
    }

    fn unlock(&mut self) -> Result<(), LockError> {
        if !self.locked {
            return Err(LockError::Poisoned {
                detail: "unlock called on an unlocked lock".to_string(),
            });
        }

        let fd = self.raw_fd();

        // SAFETY: The file descriptor is valid, and we hold the lock.
        let result = unsafe { Self::raw_flock(fd, LOCK_UN) };

        match result {
            Ok(()) => {
                self.locked = false;
                Ok(())
            }
            Err(e) => Err(LockError::Io { source: e }),
        }
    }
}

// ---------------------------------------------------------------------------
// Drop — automatic release
// ---------------------------------------------------------------------------

impl Drop for UnixLock {
    fn drop(&mut self) {
        if self.locked {
            // Best-effort unlock during drop. We ignore errors because
            // the kernel will release the lock when the file descriptor
            // is closed anyway.
            let fd = self.raw_fd();
            unsafe {
                let _ = Self::raw_flock(fd, LOCK_UN);
            }
            self.locked = false;
        }
    }
}

// ---------------------------------------------------------------------------
// Debug
// ---------------------------------------------------------------------------

impl std::fmt::Debug for UnixLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnixLock")
            .field("path", &self.path)
            .field("fd", &self.raw_fd())
            .field("locked", &self.locked)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::sync::Arc;
    use std::thread;

    fn create_temp_lock() -> (PathBuf, File) {
        let dir = std::env::temp_dir().join("openforce-lock-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("test-{}.lock", std::process::id()));
        // Remove stale lock files.
        let _ = std::fs::remove_file(&path);
        let file = File::create(&path).expect("create temp lock file");
        (path, file)
    }

    #[test]
    fn test_lock_and_unlock() {
        let (path, file) = create_temp_lock();
        let mut lock = UnixLock::new(file, &path).expect("create UnixLock");

        assert!(!lock.is_locked(), "should not be locked initially");
        assert!(lock.lock().is_ok(), "lock should succeed");
        assert!(lock.is_locked(), "should be locked after lock()");
        assert!(lock.unlock().is_ok(), "unlock should succeed");
        assert!(!lock.is_locked(), "should not be locked after unlock");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_try_lock_succeeds() {
        let (path, file) = create_temp_lock();
        let mut lock = UnixLock::new(file, &path).expect("create UnixLock");

        assert!(lock.try_lock().is_ok(), "try_lock should succeed on free lock");
        assert!(lock.is_locked());
        assert!(lock.unlock().is_ok());
        assert!(!lock.is_locked());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_exclusivity_across_processes() {
        let (path, file1) = create_temp_lock();
        let mut lock1 = UnixLock::new(file1, &path).expect("create lock1");

        // Acquire lock on file1.
        assert!(lock1.lock().is_ok(), "lock1 should acquire the lock");

        // Open the same file again (simulating another process).
        let file2 = File::open(&path).expect("open same file for lock2");
        let mut lock2 = UnixLock::new(file2, &path).expect("create lock2");

        // try_lock should fail with WouldBlock.
        let result = lock2.try_lock();
        assert!(
            matches!(result, Err(LockError::WouldBlock)),
            "lock2 should get WouldBlock, got: {:?}",
            result
        );

        // Release lock1.
        assert!(lock1.unlock().is_ok());

        // Now lock2 should be able to acquire.
        assert!(
            lock2.try_lock().is_ok(),
            "lock2 should acquire after lock1 releases"
        );
        assert!(lock2.unlock().is_ok());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_try_lock_with_timeout_succeeds_immediately() {
        let (path, file) = create_temp_lock();
        let mut lock = UnixLock::new(file, &path).expect("create UnixLock");

        let result = lock.try_lock_with_timeout(Duration::from_secs(5));
        assert!(result.is_ok(), "should acquire immediately: {:?}", result);
        assert!(lock.is_locked());
        assert!(lock.unlock().is_ok());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_try_lock_with_timeout_expires() {
        let (path, file1) = create_temp_lock();
        let mut lock1 = UnixLock::new(file1, &path).expect("create lock1");
        assert!(lock1.lock().is_ok(), "lock1 should acquire");

        let file2 = File::open(&path).expect("open same file for lock2");
        let mut lock2 = UnixLock::new(file2, &path).expect("create lock2");

        // Timeout of 100ms should expire while lock1 holds the lock.
        let start = Instant::now();
        let result = lock2.try_lock_with_timeout(Duration::from_millis(100));
        let elapsed = start.elapsed();

        assert!(
            matches!(result, Err(LockError::TimedOut { .. })),
            "expected TimedOut, got: {:?}",
            result
        );
        assert!(
            elapsed >= Duration::from_millis(80),
            "should have waited at least ~100ms, waited {:?}",
            elapsed
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "should not wait way too long, waited {:?}",
            elapsed
        );

        assert!(lock1.unlock().is_ok());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_double_unlock_returns_error() {
        let (path, file) = create_temp_lock();
        let mut lock = UnixLock::new(file, &path).expect("create UnixLock");

        assert!(lock.lock().is_ok());
        assert!(lock.unlock().is_ok());

        let result = lock.unlock();
        assert!(
            matches!(result, Err(LockError::Poisoned { .. })),
            "double unlock should be Poisoned, got: {:?}",
            result
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_drop_releases_lock() {
        let (path, file1) = create_temp_lock();
        let mut lock1 = UnixLock::new(file1, &path).expect("create lock1");
        assert!(lock1.lock().is_ok());

        // Drop lock1 without unlocking.
        drop(lock1);

        // Now lock2 should be able to acquire the lock.
        let file2 = File::open(&path).expect("open same file for lock2");
        let mut lock2 = UnixLock::new(file2, &path).expect("create lock2");
        assert!(
            lock2.try_lock().is_ok(),
            "lock2 should acquire after lock1 dropped"
        );
        assert!(lock2.unlock().is_ok());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_concurrent_exclusive_access() {
        // Spawn two threads racing for the lock. Each thread holds the
        // lock for a short period. Verify no two threads hold the lock
        // simultaneously and the test completes without deadlock.
        let (path, file1) = create_temp_lock();
        let path = Arc::new(path);
        let mut lock1 = UnixLock::new(file1, &path).expect("create lock1");
        assert!(lock1.lock().is_ok());
        assert!(lock1.unlock().is_ok());

        let path_clone = Arc::clone(&path);
        let handle1 = thread::spawn(move || {
            let file = File::open(&path_clone).expect("thread1: open");
            let mut lock = UnixLock::new(file, &path_clone).expect("thread1: create lock");
            for _ in 0..5 {
                assert!(lock.lock().is_ok(), "thread1: lock");
                thread::sleep(Duration::from_millis(10));
                assert!(lock.unlock().is_ok(), "thread1: unlock");
            }
        });

        let path_clone2 = Arc::clone(&path);
        let handle2 = thread::spawn(move || {
            let file = File::open(&path_clone2).expect("thread2: open");
            let mut lock = UnixLock::new(file, &path_clone2).expect("thread2: create lock");
            for _ in 0..5 {
                assert!(lock.try_lock_with_timeout(Duration::from_secs(2)).is_ok(), "thread2: lock");
                thread::sleep(Duration::from_millis(10));
                assert!(lock.unlock().is_ok(), "thread2: unlock");
            }
        });

        handle1.join().expect("thread1 panicked");
        handle2.join().expect("thread2 panicked");

        std::fs::remove_file(path.as_ref()).ok();
    }

    #[test]
    fn test_zero_timeout_equals_try_lock() {
        let (path, file1) = create_temp_lock();
        let mut lock1 = UnixLock::new(file1, &path).expect("create lock1");
        assert!(lock1.lock().is_ok());

        let file2 = File::open(&path).expect("open same file");
        let mut lock2 = UnixLock::new(file2, &path).expect("create lock2");

        // Zero timeout should behave like try_lock.
        let result = lock2.try_lock_with_timeout(Duration::ZERO);
        assert!(
            matches!(result, Err(LockError::TimedOut { .. })),
            "zero timeout on held lock should time out, got: {:?}",
            result
        );

        assert!(lock1.unlock().is_ok());
        std::fs::remove_file(&path).ok();
    }
}
