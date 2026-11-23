//! Linux-only, workspace-root anchored snapshot acquisition using openat2.
//! Fail closed when the syscall or required resolution flags are unavailable.

use std::fs::File;
use std::io::{Read};
use std::path::{Path, PathBuf};

use rustix::fs::{openat2, Mode, OFlags, ResolveFlags, CWD};
use rustix::fd::OwnedFd;

use crate::snapshot_payload::{
    build_snapshot_contract, CapturedWorkspaceSnapshot, SnapshotCaptureRequest,
    SnapshotPayloadError, WorkspaceSnapshotAdapter, MAX_SNAPSHOT_BYTES,
};

#[derive(Debug)]
pub enum LinuxSnapshotAdapterError {
    InvalidRoot,
}

#[cfg(target_os = "linux")]
fn metadata_matches(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    before.dev() == after.dev()
        && before.ino() == after.ino()
        && before.len() == after.len()
        && before.nlink() == after.nlink()
        && before.mtime() == after.mtime()
        && before.mtime_nsec() == after.mtime_nsec()
        && before.ctime() == after.ctime()
        && before.ctime_nsec() == after.ctime_nsec()
}

fn same_directory_identity(expected: &std::fs::Metadata, opened: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    expected.is_dir()
        && opened.is_dir()
        && expected.dev() == opened.dev()
        && expected.ino() == opened.ino()
}

pub struct LinuxWorkspaceSnapshotAdapter {
    root: File,
    canonical_root: PathBuf,
    workspace_id: String,
}

impl LinuxWorkspaceSnapshotAdapter {
    /// Securely acquire and retain the workspace root directory handle.
    ///
    /// Symlink components in the supplied root path are rejected. The handle is
    /// opened with openat2 and no-symlink resolution; the path metadata check
    /// binds the diagnostic canonical path to the directory actually opened.
    pub fn new(root: impl AsRef<Path>, workspace_id: impl Into<String>) -> Result<Self, LinuxSnapshotAdapterError> {
        let root_path = root.as_ref();
        let root_fd = openat2(
            &CWD,
            root_path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
        ).map_err(|_| LinuxSnapshotAdapterError::InvalidRoot)?;
        let root = File::from(root_fd);
        let opened_metadata = root.metadata().map_err(|_| LinuxSnapshotAdapterError::InvalidRoot)?;
        if !opened_metadata.is_dir() {
            return Err(LinuxSnapshotAdapterError::InvalidRoot);
        }

        let canonical_root = std::fs::canonicalize(root_path)
            .map_err(|_| LinuxSnapshotAdapterError::InvalidRoot)?;
        let path_metadata = std::fs::metadata(&canonical_root)
            .map_err(|_| LinuxSnapshotAdapterError::InvalidRoot)?;
        if !same_directory_identity(&opened_metadata, &path_metadata) {
            return Err(LinuxSnapshotAdapterError::InvalidRoot);
        }

        let workspace_id = workspace_id.into();
        if workspace_id.trim().is_empty() {
            return Err(LinuxSnapshotAdapterError::InvalidRoot);
        }
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
        (&mut file).take(limit as u64 + 1).read_to_end(&mut bytes)
            .map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
        if bytes.len() > limit { return Err(SnapshotPayloadError::LimitExceeded); }

        // Best-effort mutation detection: compare handle metadata before/after
        // the bounded read. The captured-byte digest remains authoritative;
        // metadata checks are not an atomic snapshot guarantee.
        let after = file.metadata().map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
        if !metadata_matches(&metadata, &after) {
            return Err(SnapshotPayloadError::AcquisitionFailed);
        }

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
    fn metadata_comparison_detects_content_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mutable");
        std::fs::write(&path, b"before").unwrap();
        let before = std::fs::metadata(&path).unwrap();
        // Length and timestamps are descriptor/filesystem-derived and should
        // change after a content mutation; use length to make this deterministic.
        std::fs::write(&path, b"after plus bytes").unwrap();
        let after = std::fs::metadata(&path).unwrap();
        assert!(!metadata_matches(&before, &after));
    }

    #[test]
    fn metadata_comparison_accepts_unchanged_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stable");
        std::fs::write(&path, b"stable").unwrap();
        let before = std::fs::metadata(&path).unwrap();
        let after = std::fs::metadata(&path).unwrap();
        assert!(metadata_matches(&before, &after));
    }

    #[test]
    fn captures_consistent_payload_during_atomic_path_replacement_stress() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.txt");
        let first = vec![b'A'; 4096];
        let second = vec![b'B'; 8192];
        std::fs::write(&target, &first).unwrap();

        let adapter = Arc::new(LinuxWorkspaceSnapshotAdapter::new(dir.path(), "workspace").unwrap());
        let barrier = Arc::new(Barrier::new(2));
        let writer_target = target.clone();
        let writer_barrier = barrier.clone();
        let first_writer = first.clone();
        let second_writer = second.clone();
        let writer = thread::spawn(move || {
            writer_barrier.wait();
            for i in 0..100 {
                let replacement = writer_target.with_file_name(format!("replacement-{i}.tmp"));
                let bytes = if i % 2 == 0 { &first_writer } else { &second_writer };
                std::fs::write(&replacement, bytes).unwrap();
                std::fs::rename(&replacement, &writer_target).unwrap();
            }
        });

        barrier.wait();
        let request = SnapshotCaptureRequest::new("target.txt", 8192).unwrap();
        let mut successful_captures = 0usize;
        for _ in 0..100 {
            if let Ok(captured) = adapter.capture(&request) {
                successful_captures += 1;
                let bytes = captured.bytes();
                assert!(bytes == first.as_slice() || bytes == second.as_slice(),
                    "capture returned unexpected partial or mixed payload");
                assert_eq!(captured.snapshot().byte_length, bytes.len() as u64);
                assert_eq!(crate::snapshot_payload::validate_snapshot_payload(captured.snapshot(), bytes), Ok(()));
            }
        }
        writer.join().unwrap();
        assert!(successful_captures > 0, "stress loop never produced a successful capture");
    }


    #[test]
    fn rejects_symlinked_workspace_root() {
        let parent = tempfile::tempdir().unwrap();
        let actual = tempfile::tempdir().unwrap();
        let link = parent.path().join("workspace-link");
        symlink(actual.path(), &link).unwrap();
        assert!(matches!(
            LinuxWorkspaceSnapshotAdapter::new(&link, "workspace"),
            Err(LinuxSnapshotAdapterError::InvalidRoot)
        ));
    }

    #[test]
    fn rejects_non_directory_workspace_root() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not-a-directory");
        std::fs::write(&file, b"file").unwrap();
        assert!(matches!(
            LinuxWorkspaceSnapshotAdapter::new(&file, "workspace"),
            Err(LinuxSnapshotAdapterError::InvalidRoot)
        ));
    }

    #[test]
    fn retained_root_handle_survives_path_replacement() {
        let parent = tempfile::tempdir().unwrap();
        let workspace = parent.path().join("workspace");
        let moved_workspace = parent.path().join("workspace-moved");
        let outside = parent.path().join("outside");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(workspace.join("target.txt"), b"inside").unwrap();
        std::fs::write(outside.join("target.txt"), b"outside").unwrap();

        let adapter = LinuxWorkspaceSnapshotAdapter::new(&workspace, "workspace").unwrap();
        std::fs::rename(&workspace, &moved_workspace).unwrap();
        symlink(&outside, &workspace).unwrap();

        let captured = adapter.capture(&SnapshotCaptureRequest::new("target.txt", 100).unwrap()).unwrap();
        assert_eq!(captured.bytes(), b"inside");
    }

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

    #[test]
    fn intermediate_directory_swap_never_escapes_retained_workspace_root() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let parent = tempfile::tempdir().unwrap();
        let workspace = parent.path().join("workspace");
        let outside = parent.path().join("outside");
        let nested = workspace.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(nested.join("target.txt"), b"inside-workspace").unwrap();
        std::fs::write(outside.join("target.txt"), b"OUTSIDE-SENTINEL").unwrap();

        let adapter = Arc::new(
            LinuxWorkspaceSnapshotAdapter::new(&workspace, "workspace").unwrap(),
        );
        let barrier = Arc::new(Barrier::new(2));
        let writer_workspace = workspace.clone();
        let writer_outside = outside.clone();
        let writer_barrier = barrier.clone();

        let writer = thread::spawn(move || {
            writer_barrier.wait();
            for index in 0..200 {
                let held = writer_workspace.join(format!("nested-held-{index}"));
                let active = writer_workspace.join("nested");

                // Move the real directory away, briefly replace its pathname
                // with an outside symlink, then restore the original directory.
                // openat2(BENEATH | NO_SYMLINKS) must either open the original
                // in-workspace object or fail closed; it must never follow the
                // attacker-controlled replacement.
                std::fs::rename(&active, &held).unwrap();
                std::os::unix::fs::symlink(&writer_outside, &active).unwrap();
                std::thread::yield_now();
                std::fs::remove_file(&active).unwrap();
                std::fs::rename(&held, &active).unwrap();
            }
        });

        barrier.wait();
        let request = SnapshotCaptureRequest::new("nested/target.txt", 128).unwrap();
        for _ in 0..400 {
            match adapter.capture(&request) {
                Ok(captured) => {
                    assert_eq!(
                        captured.bytes(),
                        b"inside-workspace",
                        "capture escaped through the swapped intermediate directory",
                    );
                    assert_eq!(
                        crate::snapshot_payload::validate_snapshot_payload(
                            captured.snapshot(),
                            captured.bytes(),
                        ),
                        Ok(()),
                    );
                }
                Err(SnapshotPayloadError::AcquisitionFailed) => {}
                Err(other) => panic!("unexpected capture error during path swap: {other:?}"),
            }
        }
        writer.join().unwrap();

        // The workspace-relative path must still resolve to the original file
        // after the adversarial swap sequence has completed.
        let final_capture = adapter.capture(&request).unwrap();
        assert_eq!(final_capture.bytes(), b"inside-workspace");
    }

}
