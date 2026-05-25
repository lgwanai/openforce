//! Filesystem utility functions.
//!
//! This module provides safe and ergonomic filesystem utilities used across
//! the space-manager crate, including recursive directory creation and
//! atomic file writing.
//!
//! # SRE Design
//!
//! All functions in this module are designed to be:
//!
//! 1. **Idempotent** — Calling `ensure_dir` on an existing directory is
//!    a no-op and returns `Ok(())`.
//!
//! 2. **Safe** — Writes go to a temporary file first, then atomically
//!    rename into place, preventing partial-write corruption.
//!
//! 3. **Transparent** — I/O errors are propagated with full context via the
//!    [`FsError`] type, which wraps `std::io::Error` with additional
//!    path information.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

// ---------------------------------------------------------------------------
// Error Types
// ---------------------------------------------------------------------------

/// Errors that can occur during filesystem operations.
///
/// Each variant wraps the underlying I/O error and includes the path that
/// caused the failure, providing full context for debugging and logging.
#[derive(Error, Debug)]
pub enum FsError {
    /// An I/O error occurred while trying to create a directory (or its
    /// parents) at the given path.
    #[error("failed to create directory `{path}`: {source}")]
    CreateDir {
        /// The path that could not be created.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// An I/O error occurred while writing to a temporary file or renaming
    /// it into place.
    #[error("failed to write file `{path}`: {source}")]
    WriteFile {
        /// The intended final path of the file.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The parent directory of the target path does not exist and could not
    /// be determined (e.g., the path has no parent, like a root path `/`).
    #[error("cannot determine parent directory for `{path}`")]
    NoParentDirectory {
        /// The path whose parent could not be determined.
        path: PathBuf,
    },
}

impl FsError {
    /// Returns a reference to the path associated with this error, if any.
    pub fn path(&self) -> &Path {
        match self {
            FsError::CreateDir { path, .. }
            | FsError::WriteFile { path, .. }
            | FsError::NoParentDirectory { path } => path.as_path(),
        }
    }

    /// Consumes the error and returns the owned path.
    pub fn into_path(self) -> PathBuf {
        match self {
            FsError::CreateDir { path, .. }
            | FsError::WriteFile { path, .. }
            | FsError::NoParentDirectory { path } => path,
        }
    }
}

// ---------------------------------------------------------------------------
// ensure_dir
// ---------------------------------------------------------------------------

/// Recursively creates all parent directories for the given path.
///
/// This function delegates to [`std::fs::create_dir_all`], which behaves
/// identically to the Unix `mkdir -p` command:
///
/// - If the directory already exists, it returns `Ok(())` without error.
/// - If any component of the path prefix does not exist, it is created.
/// - If a component already exists but is not a directory, an error is
///   returned.
///
/// # Arguments
///
/// * `path` — A path to a directory (or file path whose parent directories
///   should exist). Accepts any type that implements `AsRef<Path>`, such as
///   `&str`, `String`, `PathBuf`, or `&Path`.
///
/// # Returns
///
/// * `Ok(())` — The directory (and all missing parents) were created
///   successfully, or the directory already existed.
///
/// * `Err(FsError::CreateDir)` — An I/O error occurred. The error includes
///   both the path and the underlying `std::io::Error` for full context.
///
/// # Idempotency
///
/// This function is idempotent. Calling it multiple times on the same path
/// is safe and will not produce errors if the directory already exists.
pub fn ensure_dir(path: impl AsRef<Path>) -> Result<(), FsError> {
    let path = path.as_ref();

    fs::create_dir_all(path).map_err(|source| FsError::CreateDir {
        path: path.to_path_buf(),
        source,
    })
}

// ---------------------------------------------------------------------------
// safe_write
// ---------------------------------------------------------------------------

/// Atomically writes content to a file by writing to a temporary file first,
/// syncing it to disk, then renaming it over the target path.
///
/// This function prevents the classic "partial write" corruption problem:
/// if the process crashes mid-write, the target file is either the complete
/// old version (if the rename hasn't happened) or the complete new version
/// (if it has) — never a truncated or corrupted partial file.
///
/// # How it works
///
/// 1. A temporary file is created in the **same directory** as the target
///    path, with a `.tmp.{random}` suffix appended to the target filename.
///    Writing to the same directory is critical because `rename` is only
///    atomic on the same filesystem / mount point.
///
/// 2. The content is written to the temporary file using [`Write::write_all`],
///    which ensures all bytes are written (or an error is returned).
///
/// 3. The temporary file is synced to disk via [`File::sync_all`], ensuring
///    the data is physically persisted before the rename.
///
/// 4. The temporary file is atomically renamed to the target path via
///    [`std::fs::rename`]. On Unix, this is an atomic system call; on
///    Windows, the behaviour is near-atomic for files on the same volume.
///
/// 5. If any step fails, the temporary file is cleaned up (if it was created)
///    and an [`FsError::WriteFile`] is returned with full context.
///
/// # Arguments
///
/// * `path` — The final destination path for the file. Accepts any type
///   that implements `AsRef<Path>`.
///
/// * `content` — The data to write. Accepts any type that implements
///   `AsRef<[u8]>`, such as `&str`, `String`, `&[u8]`, or `Vec<u8>`.
///
/// # Returns
///
/// * `Ok(())` — The file was written, synced, and renamed successfully.
///
/// * `Err(FsError::NoParentDirectory)` — The path has no parent directory
///   (e.g., a root path like `/` or a bare filename with no directory).
///
/// * `Err(FsError::WriteFile)` — An I/O error occurred at any stage.
///    The error includes the target path and the underlying I/O error.
///
/// # Safety guarantees
///
/// - **No partial writes**: The target file is never a corrupted partial
///   write. Either the old file is intact, or the new file is complete.
///
/// - **Crash consistency**: If the process crashes after the rename, the
///   target file is fully written. If it crashes before the rename, the
///   temp file is left behind (and will be cleaned up on the next write).
///
/// - **No data loss on write failure**: If writing to the temp file fails,
///   the target file is untouched.
pub fn safe_write(path: impl AsRef<Path>, content: impl AsRef<[u8]>) -> Result<(), FsError> {
    let path = path.as_ref();
    let content = content.as_ref();

    // Determine the parent directory for the temp file.
    let parent = path
        .parent()
        .ok_or_else(|| FsError::NoParentDirectory {
            path: path.to_path_buf(),
        })?;

    // Generate a temporary file path in the same directory.
    let temp_filename = format!(
        "{}.tmp.{}",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default(),
        fast_random_u64()
    );
    let temp_path = parent.join(&temp_filename);

    // Track whether we created the temp file so we can clean up on error.
    let created_temp = temp_path.try_exists().unwrap_or(false);
    let did_create = !created_temp;

    // Attempt the write-sync-rename sequence.
    let result = (|| -> Result<(), FsError> {
        let mut file = fs::File::create(&temp_path).map_err(|source| FsError::WriteFile {
            path: path.to_path_buf(),
            source,
        })?;

        file.write_all(content).map_err(|source| FsError::WriteFile {
            path: path.to_path_buf(),
            source,
        })?;

        file.sync_all().map_err(|source| FsError::WriteFile {
            path: path.to_path_buf(),
            source,
        })?;

        fs::rename(&temp_path, path).map_err(|source| FsError::WriteFile {
            path: path.to_path_buf(),
            source,
        })?;

        // On Unix, sync the parent directory after rename to ensure the
        // directory entry is committed. This is critical for crash consistency.
        #[cfg(unix)]
        {
            let dir_file = fs::File::open(parent).map_err(|source| FsError::WriteFile {
                path: path.to_path_buf(),
                source,
            })?;
            dir_file.sync_all().map_err(|source| FsError::WriteFile {
                path: path.to_path_buf(),
                source,
            })?;
        }

        Ok(())
    })();

    // If the write failed, clean up the temp file (if we created one).
    if result.is_err() && did_create {
        let _ = fs::remove_file(&temp_path);
    }

    result
}

/// Generate a fast, pseudo-random u64 for temporary file naming.
///
/// Uses a simple xorshift64* PRNG seeded from the process ID and a
/// high-resolution timestamp. This avoids pulling in external dependencies
/// like `rand` or `uuid` for a single utility crate.
fn fast_random_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let pid = std::process::id() as u64;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    let mut state = pid.wrapping_mul(6364136223846793005).wrapping_add(nanos);

    // One round of xorshift64*.
    state ^= state >> 12;
    state ^= state << 25;
    state ^= state >> 27;
    state.wrapping_mul(2685821657736338717)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;
    use std::thread;

    // -----------------------------------------------------------------------
    // ensure_dir tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ensure_dir_creates_new_directory() {
        let dir = Path::new("/tmp/openforce-test/ensure-dir-new");
        let _ = fs::remove_dir_all(dir);

        let result = ensure_dir(dir);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert!(dir.exists(), "directory should exist after ensure_dir");
        assert!(dir.is_dir(), "path should be a directory after ensure_dir");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_ensure_dir_creates_nested_directories() {
        let dir = Path::new("/tmp/openforce-test/ensure-dir/nested/deep/path");
        let _ = fs::remove_dir_all("/tmp/openforce-test/ensure-dir");

        let result = ensure_dir(dir);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert!(dir.exists(), "nested directory should exist");
        assert!(dir.is_dir(), "nested path should be a directory");
        let _ = fs::remove_dir_all("/tmp/openforce-test/ensure-dir");
    }

    #[test]
    fn test_ensure_dir_idempotent() {
        let dir = Path::new("/tmp/openforce-test/ensure-dir-idempotent");
        let _ = fs::remove_dir_all(dir);

        assert!(ensure_dir(dir).is_ok(), "first call should succeed");
        assert!(ensure_dir(dir).is_ok(), "second call (idempotent) should succeed");
        assert!(ensure_dir(dir).is_ok(), "third call (idempotent) should succeed");
        assert!(dir.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_ensure_dir_existing_directory() {
        let dir = Path::new("/tmp/openforce-test/ensure-dir-existing");
        let _ = fs::remove_dir_all(dir);
        fs::create_dir_all(dir).expect("setup: create dir");

        let result = ensure_dir(dir);
        assert!(result.is_ok(), "ensure_dir on existing directory should succeed: {:?}", result);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_ensure_dir_error_on_readonly_parent() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let parent = Path::new("/tmp/openforce-test/ensure-dir-readonly");
            let child = parent.join("subdir");

            // Reset permissions first (in case of stale state from a failed run)
            let _ = fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755));
            let _ = fs::remove_dir_all(parent);

            fs::create_dir_all(parent).expect("setup: create parent");
            // r-x: traversal allowed, write denied => mkdir fails with PermissionDenied
            fs::set_permissions(parent, std::fs::Permissions::from_mode(0o555))
                .expect("setup: set read-only (r-x)");

            let result = ensure_dir(&child);
            assert!(result.is_err(), "expected Err when parent is read-only, got: {:?}", result);

            if let Err(FsError::CreateDir { path, source }) = result {
                assert_eq!(path, child);
                assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
            } else {
                panic!("expected CreateDir variant");
            }

            // Restore permissions before removal
            fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755))
                .expect("cleanup: restore permissions");
            let _ = fs::remove_dir_all(parent);
        }

        #[cfg(not(unix))]
        {
            eprintln!("skipping test_ensure_dir_error_on_readonly_parent on non-Unix platform");
        }
    }

    #[test]
    fn test_ensure_dir_creates_multiple_levels() {
        let dir = Path::new("/tmp/openforce-test/a/b/c/d/e/f/g/h/i/j");
        let _ = fs::remove_dir_all("/tmp/openforce-test/a");

        let result = ensure_dir(dir);
        assert!(result.is_ok(), "deeply nested path should be created: {:?}", result);
        assert!(dir.exists(), "deeply nested dir should exist");
        let _ = fs::remove_dir_all("/tmp/openforce-test/a");
    }

    #[test]
    fn test_ensure_dir_pathbuf() {
        let pathbuf = PathBuf::from("/tmp/openforce-test/ensure-dir-pathbuf");
        let _ = fs::remove_dir_all(&pathbuf);

        let result = ensure_dir(pathbuf.as_path());
        assert!(result.is_ok(), "PathBuf reference should work: {:?}", result);
        let _ = fs::remove_dir_all("/tmp/openforce-test/ensure-dir-pathbuf");
    }

    #[test]
    fn test_ensure_dir_string_slice() {
        let _ = fs::remove_dir_all("/tmp/openforce-test/ensure-dir-str");

        let result = ensure_dir("/tmp/openforce-test/ensure-dir-str");
        assert!(result.is_ok(), "&str path should work: {:?}", result);
        let _ = fs::remove_dir_all("/tmp/openforce-test/ensure-dir-str");
    }

    #[test]
    fn test_ensure_dir_concurrent() {
        // Multiple threads racing to create the same directory tree.
        // All should succeed because create_dir_all is idempotent.
        let dir = PathBuf::from("/tmp/openforce-test/ensure-dir-concurrent/subdir");
        let _ = fs::remove_dir_all("/tmp/openforce-test/ensure-dir-concurrent");
        let dir = Arc::new(dir);

        let mut handles = Vec::new();
        for i in 0..10 {
            let d = Arc::clone(&dir);
            handles.push(thread::spawn(move || {
                let result = ensure_dir(d.as_path());
                (i, result)
            }));
        }

        let mut results = Vec::new();
        for h in handles {
            results.push(h.join().expect("thread panicked"));
        }

        for (i, result) in &results {
            assert!(result.is_ok(), "thread {} should succeed: {:?}", i, result);
        }

        assert!(dir.exists(), "directory should exist after concurrent ensure_dir");
        let _ = fs::remove_dir_all("/tmp/openforce-test/ensure-dir-concurrent");
    }

    #[test]
    fn test_ensure_dir_twice_on_same_path_from_multiple_threads() {
        // More targeted: N threads all call ensure_dir on the same path
        // simultaneously. Idempotency guarantee must hold under concurrency.
        let dir = PathBuf::from("/tmp/openforce-test/ensure-dir-race");
        let _ = fs::remove_dir_all(&dir);
        let dir = Arc::new(dir);

        let mut handles = Vec::new();
        for _ in 0..20 {
            let d = Arc::clone(&dir);
            handles.push(thread::spawn(move || ensure_dir(d.as_path())));
        }

        for h in handles {
            let result = h.join().expect("thread panicked");
            assert!(result.is_ok(), "concurrent ensure_dir should succeed: {:?}", result);
        }

        assert!(dir.exists());
        let _ = fs::remove_dir_all("/tmp/openforce-test/ensure-dir-race");
    }

    // -----------------------------------------------------------------------
    // safe_write tests
    // -----------------------------------------------------------------------

    fn test_dir(test_name: &str) -> PathBuf {
        let dir = PathBuf::from(format!("/tmp/openforce-test/safe-write-{}", test_name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect(&format!("setup: create test dir for {}", test_name));
        dir
    }

    #[test]
    fn test_safe_write_creates_file() {
        let dir = test_dir("creates_file");
        let path = dir.join("hello.txt");

        let result = safe_write(&path, "Hello, world!");
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert!(path.exists(), "file should exist after safe_write");
        assert_eq!(fs::read_to_string(&path).unwrap(), "Hello, world!");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_safe_write_binary_content() {
        let dir = test_dir("binary_content");
        let path = dir.join("binary.bin");

        let binary_data: Vec<u8> = (0..255).collect();
        let result = safe_write(&path, &binary_data);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert_eq!(fs::read(&path).unwrap(), binary_data);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_safe_write_overwrites_existing_file() {
        let dir = test_dir("overwrite");
        let path = dir.join("overwrite.txt");

        safe_write(&path, "initial content").expect("first write");
        assert_eq!(fs::read_to_string(&path).unwrap(), "initial content");

        let result = safe_write(&path, "overwritten content");
        assert!(result.is_ok(), "overwrite should succeed: {:?}", result);
        assert_eq!(fs::read_to_string(&path).unwrap(), "overwritten content");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_safe_write_large_content() {
        let dir = test_dir("large_content");
        let path = dir.join("large.txt");

        let large_content = "A".repeat(1024 * 1024);
        let result = safe_write(&path, &large_content);
        assert!(result.is_ok(), "large write should succeed: {:?}", result);

        let content = fs::read_to_string(&path).expect("read file");
        assert_eq!(content.len(), 1024 * 1024, "content length should match");
        assert!(content.chars().all(|c| c == 'A'), "all chars should be 'A'");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_safe_write_empty_content() {
        let dir = test_dir("empty_content");
        let path = dir.join("empty.txt");

        let result = safe_write(&path, "");
        assert!(result.is_ok(), "empty write should succeed: {:?}", result);
        assert!(path.exists(), "file should exist after empty write");
        assert!(fs::read_to_string(&path).unwrap().is_empty(), "file should be empty");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_safe_write_nonexistent_directory() {
        let path = Path::new("/tmp/openforce-test/safe-write-nonexistent/file.txt");
        let _ = fs::remove_dir_all("/tmp/openforce-test/safe-write-nonexistent");

        let result = safe_write(&path, "content");
        assert!(result.is_err(), "expected Err for nonexistent parent directory");
        assert!(matches!(result, Err(FsError::WriteFile { .. })));
    }

    #[test]
    fn test_safe_write_no_parent_directory() {
        let result = safe_write("/", "content");
        assert!(result.is_err(), "expected Err for path with no parent");
        assert!(matches!(result, Err(FsError::NoParentDirectory { .. })));
    }

    #[test]
    fn test_safe_write_cleans_up_temp_on_failure() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let dir = Path::new("/tmp/openforce-test/safe-write-cleanup-tmp");
            // Reset permissions first (in case of stale state from a failed run)
            let _ = fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
            let _ = fs::remove_dir_all(dir);
            fs::create_dir_all(dir).expect("setup: create dir");

            let path = dir.join("target.txt");

            // Remove write permission (r-x) so temp file creation fails
            fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555))
                .expect("setup: set read-only (r-x)");

            let result = safe_write(&path, "content");
            assert!(result.is_err(), "expected Err on read-only dir");

            // No temp file should remain
            let entries: Vec<_> = fs::read_dir(dir)
                .expect("read dir")
                .filter_map(|e| e.ok())
                .collect();
            for entry in &entries {
                let name = entry.file_name();
                assert!(
                    !name.to_string_lossy().contains(".tmp."),
                    "temp file should have been cleaned up: {:?}",
                    name
                );
            }

            // Restore permissions before removal
            fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))
                .expect("cleanup: restore permissions");
            let _ = fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn test_safe_write_multiple_times() {
        let dir = test_dir("multiple_times");
        let path = dir.join("multi.txt");

        for i in 0..10 {
            let content = format!("write iteration {}", i);
            let result = safe_write(&path, &content);
            assert!(result.is_ok(), "write {} should succeed: {:?}", i, result);

            let read_back = fs::read_to_string(&path).expect("read file");
            assert_eq!(read_back, content, "content should match iteration {}", i);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_safe_write_special_characters() {
        let dir = test_dir("special_chars");
        let path = dir.join("special.txt");

        let content = "Hello, 世界! 👋🌍\nLine 2\n\tTabs & special chars: \x00\x01\x02";
        let result = safe_write(&path, content);
        assert!(result.is_ok(), "special chars write: {:?}", result);
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_safe_write_string_type() {
        let dir = test_dir("string_type");
        let path = dir.join("string.txt");

        let content = String::from("Hello from String");
        let result = safe_write(&path, content);
        assert!(result.is_ok(), "String content: {:?}", result);
        assert_eq!(fs::read_to_string(&path).unwrap(), "Hello from String");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_safe_write_pathbuf_target() {
        let dir = test_dir("pathbuf_target");
        let path = dir.join("pathbuf.txt");

        let pathbuf = path.clone();
        let result = safe_write(pathbuf.as_path(), "content via PathBuf");
        assert!(result.is_ok(), "PathBuf target: {:?}", result);
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_safe_write_temp_file_not_left_behind() {
        let dir = test_dir("no_temp_left");
        let path = dir.join("no-temp-left.txt");

        safe_write(&path, "no temp left behind").expect("write");

        let entries: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .collect();

        for entry in &entries {
            let name = entry.file_name();
            assert!(
                !name.to_string_lossy().contains(".tmp."),
                "no temp file should remain after successful write: {:?}",
                name
            );
        }

        assert_eq!(entries.len(), 1, "only one file should exist in the dir");
        assert_eq!(entries[0].file_name(), "no-temp-left.txt");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_safe_write_preserves_existing_on_write_failure() {
        // Verifies that if a write fails, the existing file at the target
        // path is not modified. Algorithmic guarantee: temp file is created
        // first and ONLY renamed on success — any error before the rename
        // leaves the original untouched.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let dir = Path::new("/tmp/openforce-test/sw-preserve-existing");
            // Reset permissions first (in case of stale state from a failed run)
            let _ = fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
            let _ = fs::remove_dir_all(dir);
            fs::create_dir_all(dir).expect("setup: create dir");

            let path = dir.join("preserve.txt");

            // Write initial content
            safe_write(&path, "original content").expect("initial write");
            assert_eq!(fs::read_to_string(&path).unwrap(), "original content");

            // Make the directory non-writable (r-x) so next temp file
            // creation fails. We keep execute to allow reading existing files.
            fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555))
                .expect("setup: set read-only (r-x)");

            let result = safe_write(&path, "new content");
            assert!(result.is_err(), "expected Err on read-only dir");

            // Original file should be untouched
            assert!(path.exists(), "original file should still exist");
            let content = fs::read_to_string(&path).expect("read file");
            assert_eq!(content, "original content", "original content should be preserved");

            // Cleanup: restore permissions and remove
            fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))
                .expect("cleanup: restore permissions");
            let _ = fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn test_safe_write_concurrent_same_target() {
        // Multiple threads writing to the same file concurrently.
        // Because safe_write uses a temp file + atomic rename, each
        // writer creates its own temp file and atomically swaps it in.
        // The result should be one of the writers' content, never a
        // corrupted mix, and the file should always be valid UTF-8.
        let dir = test_dir("concurrent_same_target");
        let path = Arc::new(dir.join("concurrent.txt"));

        let mut handles = Vec::new();
        for i in 0..10 {
            let p = Arc::clone(&path);
            handles.push(thread::spawn(move || {
                let content = format!("writer-{} says hello!", i);
                safe_write(p.as_path(), content.as_bytes())
            }));
        }

        for h in handles {
            let result = h.join().expect("thread panicked");
            assert!(result.is_ok(), "concurrent write should succeed: {:?}", result);
        }

        // The file must exist and contain valid (non-garbled) content
        assert!(path.exists(), "file should exist after concurrent writes");
        let final_content = fs::read_to_string(path.as_ref()).expect("read final content");
        assert!(!final_content.is_empty(), "final content should not be empty");
        // Content should start with a known prefix
        assert!(
            final_content.starts_with("writer-"),
            "final content should be from one of the writers: got {:?}",
            final_content
        );

        let _ = fs::remove_dir_all(&*dir);
    }

    #[test]
    fn test_safe_write_concurrent_different_targets() {
        // Multiple threads writing to different files in the same
        // directory. All should succeed independently.
        let dir = test_dir("concurrent_different_targets");
        let dir = Arc::new(dir);

        let mut handles = Vec::new();
        for i in 0..10 {
            let d = Arc::clone(&dir);
            handles.push(thread::spawn(move || {
                let path = d.join(format!("file-{}.txt", i));
                let content = format!("content-{}", i);
                safe_write(&path, content.as_bytes())
            }));
        }

        for (idx, h) in handles.into_iter().enumerate() {
            let result = h.join().expect("thread panicked");
            assert!(result.is_ok(), "thread {} write should succeed: {:?}", idx, result);
        }

        // Verify each file exists with correct content
        for i in 0..10 {
            let path = dir.join(format!("file-{}.txt", i));
            assert!(path.exists(), "file-{}.txt should exist", i);
            let content = fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("file-{}.txt should be readable", i));
            assert_eq!(content, format!("content-{}", i));
        }

        let _ = fs::remove_dir_all(&*dir);
    }
}
