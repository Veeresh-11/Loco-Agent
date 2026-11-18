//! Bounded workspace file reads (prototype).
//!
//! This is deliberately not wired into agent execution. Path validation and
//! opening remain separate operations and are vulnerable to TOCTOU races if a
//! workspace is concurrently modified. A production executor should use
//! capability/handle-relative OS primitives and enforce policy + approval first.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::path_validation::{validate_workspace_path, PathValidationError};

pub const MAX_WORKSPACE_READ_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug)]
pub enum WorkspaceReadError {
    Path(PathValidationError),
    Io(io::Error),
    NotRegularFile,
    LimitExceeded,
    InvalidLimit,
}

impl From<io::Error> for WorkspaceReadError {
    fn from(value: io::Error) -> Self { Self::Io(value) }
}

/// Read a workspace-relative regular file with a caller-provided hard byte cap.
/// Rejects a file whose metadata length already exceeds the cap and also reads
/// at most cap+1 bytes to detect growth after metadata was queried.
pub fn read_workspace_file(
    workspace_root: &Path,
    relative_path: &Path,
    max_bytes: usize,
) -> Result<Vec<u8>, WorkspaceReadError> {
    if max_bytes == 0 || max_bytes > MAX_WORKSPACE_READ_BYTES { return Err(WorkspaceReadError::InvalidLimit); }

    let resolved = validate_workspace_path(workspace_root, relative_path)
        .map_err(WorkspaceReadError::Path)?;
    let metadata = fs::metadata(&resolved)?;
    if !metadata.is_file() { return Err(WorkspaceReadError::NotRegularFile); }
    if metadata.len() > max_bytes as u64 { return Err(WorkspaceReadError::LimitExceeded); }

    let mut file = File::open(&resolved)?;
    let mut data = Vec::with_capacity(metadata.len().min(max_bytes as u64) as usize);
    file.take(max_bytes as u64 + 1).read_to_end(&mut data)?;
    if data.len() > max_bytes { return Err(WorkspaceReadError::LimitExceeded); }
    Ok(data)
}

/// Returns the validated resolved path only. This is not a write permission
/// grant; callers must not write through it until race-resistant operations
/// and the authorization/approval gate exist.
pub fn resolve_workspace_target(
    workspace_root: &Path,
    relative_path: &Path,
) -> Result<PathBuf, PathValidationError> {
    validate_workspace_path(workspace_root, relative_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reads_file_within_limit() {
        let t = tempdir().unwrap();
        fs::create_dir_all(t.path().join("ws")).unwrap();
        fs::write(t.path().join("ws/data.txt"), b"hello").unwrap();
        assert_eq!(read_workspace_file(t.path().join("ws").as_path(), Path::new("data.txt"), 5).unwrap(), b"hello");
    }

    #[test]
    fn rejects_file_over_limit() {
        let t = tempdir().unwrap();
        fs::create_dir_all(t.path().join("ws")).unwrap();
        fs::write(t.path().join("ws/data.txt"), b"hello!").unwrap();
        assert!(matches!(read_workspace_file(t.path().join("ws").as_path(), Path::new("data.txt"), 5), Err(WorkspaceReadError::LimitExceeded)));
    }

    #[test]
    fn rejects_zero_limit() {
        let t = tempdir().unwrap();
        fs::create_dir_all(t.path().join("ws")).unwrap();
        fs::write(t.path().join("ws/data.txt"), b"x").unwrap();
        assert!(matches!(read_workspace_file(t.path().join("ws").as_path(), Path::new("data.txt"), 0), Err(WorkspaceReadError::InvalidLimit)));
    }

    #[test]
    fn rejects_directories() {
        let t = tempdir().unwrap();
        fs::create_dir_all(t.path().join("ws/subdir")).unwrap();
        assert!(matches!(read_workspace_file(t.path().join("ws").as_path(), Path::new("subdir"), 10), Err(WorkspaceReadError::NotRegularFile)));
    }

    #[test]
    fn rejects_parent_traversal() {
        let t = tempdir().unwrap();
        fs::create_dir_all(t.path().join("ws")).unwrap();
        assert!(matches!(read_workspace_file(t.path().join("ws").as_path(), Path::new("../secret"), 10), Err(WorkspaceReadError::Path(PathValidationError::ParentTraversal))));
    }

    #[test]
    fn rejects_symlink_escape_if_supported() {
        let t = tempdir().unwrap();
        let root = t.path().join("ws");
        let outside = t.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), b"secret").unwrap();
        #[cfg(unix)] let link = std::os::unix::fs::symlink(&outside, root.join("escape"));
        #[cfg(windows)] let link = std::os::windows::fs::symlink_dir(&outside, root.join("escape"));
        if link.is_err() { return; }
        assert!(matches!(read_workspace_file(&root, Path::new("escape/secret.txt"), 100), Err(WorkspaceReadError::Path(PathValidationError::OutsideWorkspace))));
    }
}
