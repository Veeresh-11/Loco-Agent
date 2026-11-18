//! Linux-only, workspace-root anchored snapshot acquisition using openat2.
//! Fail closed when the syscall or required resolution flags are unavailable.

use std::fs::{File, OpenOptions};
use std::io::{Read};
use std::path::{Path, PathBuf};

use rustix::fs::{openat2, Mode, OFlags, ResolveFlags};
use rustix::fd::OwnedFd;

use crate::snapshot_payload::{
    build_snapshot_contract, CapturedWorkspaceSnapshot, SnapshotCaptureRequest,
    SnapshotPayloadError, WorkspaceSnapshotAdapter, MAX_SNAPSHOT_BYTES,
};

#[derive(Debug)]
pub enum LinuxSnapshotAdapterError {
    InvalidRoot,
}

pub struct LinuxWorkspaceSnapshotAdapter {
    root: File,
    canonical_root: PathBuf,
    workspace_id: String,
}

impl LinuxWorkspaceSnapshotAdapter {
    /// Canonicalize and open the root once; future captures use this directory handle.
    pub fn new(root: impl AsRef<Path>, workspace_id: impl Into<String>) -> Result<Self, LinuxSnapshotAdapterError> {
        let canonical_root = std::fs::canonicalize(root).map_err(|_| LinuxSnapshotAdapterError::InvalidRoot)?;
        let root = OpenOptions::new().read(true).open(&canonical_root)
            .map_err(|_| LinuxSnapshotAdapterError::InvalidRoot)?;
        if !root.metadata().map_err(|_| LinuxSnapshotAdapterError::InvalidRoot)?.is_dir() {
            return Err(LinuxSnapshotAdapterError::InvalidRoot);
        }
        let workspace_id = workspace_id.into();
        if workspace_id.trim().is_empty() { return Err(LinuxSnapshotAdapterError::InvalidRoot); }
        Ok(Self { root, canonical_root, workspace_id })
    }

    pub fn root_path(&self) -> &Path { &self.canonical_root }
}

impl WorkspaceSnapshotAdapter for LinuxWorkspaceSnapshotAdapter {
    fn capture(&self, request: &SnapshotCaptureRequest) -> Result<CapturedWorkspaceSnapshot, SnapshotPayloadError> {
        let fd: OwnedFd = openat2(
            &self.root,
            request.relative_path(),
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
            ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
        ).map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;

        let mut file = File::from(fd);
        let metadata = file.metadata().map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
        if !metadata.is_file() { return Err(SnapshotPayloadError::InvalidFileType); }
        // Strict provenance policy: reject files with multiple hard links because
        // another path outside this workspace may reference the same inode.
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 { return Err(SnapshotPayloadError::HardLinkNotAllowed); }
        let limit = MAX_SNAPSHOT_BYTES.min(request.max_bytes());
        if metadata.len() > limit as u64 { return Err(SnapshotPayloadError::LimitExceeded); }

        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(limit as u64 + 1).read_to_end(&mut bytes)
            .map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
        if bytes.len() > limit { return Err(SnapshotPayloadError::LimitExceeded); }

        let identity = format!("dev:{}:ino:{}:captured_len:{}", metadata.dev(), metadata.ino(), bytes.len());
        let contract = build_snapshot_contract(&self.workspace_id, request.relative_path(), &identity, &bytes)?;
        CapturedWorkspaceSnapshot::new(contract, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use crate::snapshot_payload::SnapshotCaptureRequest;

    #[test]
    fn captures_regular_file_and_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sample.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("empty.txt"), b"").unwrap();
        let adapter = LinuxWorkspaceSnapshotAdapter::new(dir.path(), "workspace").unwrap();
        let a = adapter.capture(&SnapshotCaptureRequest::new("sample.txt", 100).unwrap()).unwrap();
        let b = adapter.capture(&SnapshotCaptureRequest::new("empty.txt", 100).unwrap()).unwrap();
        assert_eq!(a.bytes(), b"hello");
        assert!(b.bytes().is_empty());
    }

    #[test]
    fn rejects_symlinks_and_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), b"secret").unwrap();
        symlink(outside.path().join("secret"), dir.path().join("link")).unwrap();
        let adapter = LinuxWorkspaceSnapshotAdapter::new(dir.path(), "workspace").unwrap();
        assert!(adapter.capture(&SnapshotCaptureRequest::new("link", 100).unwrap()).is_err());
        assert!(SnapshotCaptureRequest::new("../secret", 100).is_err());
    }

    #[test]
    fn accepts_exact_request_limit_and_rejects_one_byte_over() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("exact"), b"1234").unwrap();
        std::fs::write(dir.path().join("over"), b"12345").unwrap();
        let adapter = LinuxWorkspaceSnapshotAdapter::new(dir.path(), "workspace").unwrap();
        let exact = adapter.capture(&SnapshotCaptureRequest::new("exact", 4).unwrap()).unwrap();
        assert_eq!(exact.bytes(), b"1234");
        assert!(matches!(
            adapter.capture(&SnapshotCaptureRequest::new("over", 4).unwrap()),
            Err(SnapshotPayloadError::LimitExceeded)
        ));
    }

    #[test]
    fn rejects_directory_and_intermediate_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        std::fs::write(outside.path().join("target.txt"), b"outside").unwrap();
        symlink(outside.path(), dir.path().join("linked-dir")).unwrap();
        let adapter = LinuxWorkspaceSnapshotAdapter::new(dir.path(), "workspace").unwrap();
        assert!(matches!(
            adapter.capture(&SnapshotCaptureRequest::new("nested", 100).unwrap()),
            Err(SnapshotPayloadError::InvalidFileType)
        ));
        assert!(adapter.capture(&SnapshotCaptureRequest::new("linked-dir/target.txt", 100).unwrap()).is_err());
    }

    #[test]
    fn rejects_missing_target_without_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = LinuxWorkspaceSnapshotAdapter::new(dir.path(), "workspace").unwrap();
        assert_eq!(
            adapter.capture(&SnapshotCaptureRequest::new("missing.txt", 100).unwrap()),
            Err(SnapshotPayloadError::AcquisitionFailed)
        );
    }

    #[test]
    fn rejects_hard_linked_file_under_strict_provenance_policy() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("origin.txt"), b"shared data").unwrap();
        std::fs::hard_link(outside.path().join("origin.txt"), workspace.path().join("inside.txt")).unwrap();
        let adapter = LinuxWorkspaceSnapshotAdapter::new(workspace.path(), "workspace").unwrap();
        assert!(matches!(
            adapter.capture(&SnapshotCaptureRequest::new("inside.txt", 100).unwrap()),
            Err(SnapshotPayloadError::HardLinkNotAllowed)
        ));
    }

    #[test]
    fn fifo_target_is_rejected_without_blocking() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("pipe");
        let c_path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: c_path is a valid NUL-terminated path; mkfifo only creates a
        // named FIFO at that path and does not dereference Rust memory.
        let result = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
        assert_eq!(result, 0, "mkfifo failed");
        let adapter = LinuxWorkspaceSnapshotAdapter::new(dir.path(), "workspace").unwrap();
        assert!(matches!(
            adapter.capture(&SnapshotCaptureRequest::new("pipe", 64).unwrap()),
            Err(SnapshotPayloadError::InvalidFileType)
        ));
    }

    #[test]
    fn enforces_request_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("data"), b"12345").unwrap();
        let adapter = LinuxWorkspaceSnapshotAdapter::new(dir.path(), "workspace").unwrap();
        assert!(matches!(adapter.capture(&SnapshotCaptureRequest::new("data", 4).unwrap()),
            Err(SnapshotPayloadError::LimitExceeded)));
    }
}
