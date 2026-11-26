//! Platform-selected workspace adapter construction.
//!
//! The caller must pass a workspace root selected explicitly by the host UI.
//! This module treats that path as untrusted input and delegates acquisition to
//! the platform's retained-handle adapter. It never derives a root from a
//! proposal path and never falls back to pathname-based capture.

use std::path::Path;

use crate::snapshot_payload::WorkspaceSnapshotAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFactoryError {
    InvalidSelection,
    UnsupportedPlatform,
}

/// Construct a native read-only adapter for an explicitly selected workspace.
///
/// A successful return means only that a native adapter retained a root handle.
/// It is not user authentication, approval, or permission to execute an action.
pub fn create_workspace_adapter(
    selected_root: &Path,
    workspace_id: impl Into<String>,
) -> Result<Box<dyn WorkspaceSnapshotAdapter>, WorkspaceFactoryError> {
    let workspace_id = workspace_id.into();
    if workspace_id.trim().is_empty() {
        return Err(WorkspaceFactoryError::InvalidSelection);
    }

    #[cfg(target_os = "linux")]
    {
        let adapter = crate::linux_snapshot_adapter::LinuxWorkspaceSnapshotAdapter::new(
            selected_root,
            workspace_id,
        )
        .map_err(|_| WorkspaceFactoryError::InvalidSelection)?;
        return Ok(Box::new(adapter));
    }

    #[cfg(target_os = "macos")]
    {
        let adapter = crate::macos_snapshot_adapter::MacOsWorkspaceSnapshotAdapter::new(
            selected_root,
            workspace_id,
        )
        .map_err(|_| WorkspaceFactoryError::InvalidSelection)?;
        return Ok(Box::new(adapter));
    }

    #[cfg(target_os = "windows")]
    {
        let (volume_guid_root, relative_workspace) =
            windows_selection_to_volume_root(selected_root)?;
        let adapter = crate::windows_root_handle::WindowsWorkspaceSnapshotAdapter::new(
            &volume_guid_root,
            &relative_workspace,
            workspace_id,
        )
        .map_err(|_| WorkspaceFactoryError::InvalidSelection)?;
        return Ok(Box::new(adapter));
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (selected_root, workspace_id);
        Err(WorkspaceFactoryError::UnsupportedPlatform)
    }
}

#[cfg(target_os = "windows")]
fn windows_selection_to_volume_root(
    selected_root: &Path,
) -> Result<(String, String), WorkspaceFactoryError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
    };

    let selected = validate_windows_selected_path(selected_root)?;
    let selected_wide: Vec<u16> = selected
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // Fixed, bounded buffers: paths or volume names that do not fit are rejected.
    let mut mount_buffer = [0u16; 1024];
    // SAFETY: both strings are NUL-terminated and buffers remain writable for
    // the synchronous Win32 calls. No caller-controlled pointer is retained.
    let mount_ok = unsafe {
        GetVolumePathNameW(
            selected_wide.as_ptr(),
            mount_buffer.as_mut_ptr(),
            mount_buffer.len() as u32,
        )
    };
    if mount_ok == 0 {
        return Err(WorkspaceFactoryError::InvalidSelection);
    }
    let mount_end = mount_buffer
        .iter()
        .position(|unit| *unit == 0)
        .ok_or(WorkspaceFactoryError::InvalidSelection)?;
    let mount = String::from_utf16(&mount_buffer[..mount_end])
        .map_err(|_| WorkspaceFactoryError::InvalidSelection)?;

    let mut volume_buffer = [0u16; 128];
    // SAFETY: mount_buffer contains a verified NUL-terminated Win32 result and
    // volume_buffer is a writable output buffer of the declared capacity.
    let volume_ok = unsafe {
        GetVolumeNameForVolumeMountPointW(
            mount_buffer.as_ptr(),
            volume_buffer.as_mut_ptr(),
            volume_buffer.len() as u32,
        )
    };
    if volume_ok == 0 {
        return Err(WorkspaceFactoryError::InvalidSelection);
    }
    let volume_end = volume_buffer
        .iter()
        .position(|unit| *unit == 0)
        .ok_or(WorkspaceFactoryError::InvalidSelection)?;
    let volume_guid_root = String::from_utf16(&volume_buffer[..volume_end])
        .map_err(|_| WorkspaceFactoryError::InvalidSelection)?;

    // Both values are now valid Unicode strings decoded strictly from the
    // exact UTF-16 values used/returned by Win32. Never make path decisions on
    // a lossy representation of the caller's original OsStr.
    let selected_normalized = normalize_windows_drive_path(&selected)
        .ok_or(WorkspaceFactoryError::InvalidSelection)?;
    let mount_normalized = normalize_windows_drive_path(&mount)
        .ok_or(WorkspaceFactoryError::InvalidSelection)?;
    if !mount_normalized.is_ascii() {
        return Err(WorkspaceFactoryError::InvalidSelection);
    }
    let mount_prefix = mount_normalized.trim_end_matches('\\');
    if selected_normalized.len() <= mount_prefix.len()
        || !selected_normalized[..mount_prefix.len()].eq_ignore_ascii_case(mount_prefix)
        || selected_normalized.as_bytes().get(mount_prefix.len()) != Some(&b'\\')
    {
        return Err(WorkspaceFactoryError::InvalidSelection);
    }

    let relative = selected_normalized[mount_prefix.len() + 1..].trim_end_matches('\\');
    if relative.is_empty() {
        return Err(WorkspaceFactoryError::InvalidSelection);
    }
    let components: Vec<&str> = relative.split('\\').collect();
    if components.iter().any(|part| {
        part.is_empty()
            || *part == "."
            || *part == ".."
            || part.contains(':')
            || part.contains('/')
            || part.contains('\\')
    }) {
        return Err(WorkspaceFactoryError::InvalidSelection);
    }

    Ok((volume_guid_root, components.join("/")))
}

#[cfg(target_os = "windows")]
fn validate_windows_selected_path(selected_root: &Path) -> Result<String, WorkspaceFactoryError> {
    const MAX_WINDOWS_PATH_UNITS: usize = 32_767;

    if !selected_root.is_absolute() {
        return Err(WorkspaceFactoryError::InvalidSelection);
    }

    // Windows paths are UTF-16. Reject ill-formed UTF-16 rather than silently
    // replacing code units, and reject NUL before passing the path to Win32.
    let selected = selected_root
        .to_str()
        .ok_or(WorkspaceFactoryError::InvalidSelection)?;
    if selected.contains('\\0')
        || selected.encode_utf16().count() > MAX_WINDOWS_PATH_UNITS
    {
        return Err(WorkspaceFactoryError::InvalidSelection);
    }
    Ok(selected.to_owned())
}

#[cfg(target_os = "windows")]
fn normalize_windows_drive_path(value: &str) -> Option<String> {
    let replaced = value.replace('/', "\\");
    let without_extended_prefix = replaced
        .strip_prefix(r"\\?\")
        .unwrap_or(&replaced);
    let without_trailing_separator = if without_extended_prefix.len() > 3 {
        without_extended_prefix.trim_end_matches('\\')
    } else {
        without_extended_prefix
    };
    let bytes = without_trailing_separator.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || without_trailing_separator.starts_with(r"\\")
    {
        return None;
    }
    let components = &without_trailing_separator[3..];
    if !components.is_empty()
        && components
            .split('\\')
            .any(|part| part.is_empty() || part == "." || part == ".." || part.contains(':'))
    {
        return None;
    }
    Some(without_trailing_separator.to_string())
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;

    #[test]
    fn selected_path_validation_rejects_embedded_nul_and_overlong_paths() {
        use std::path::PathBuf;

        assert_eq!(
            validate_windows_selected_path(Path::new("C:\\\\work\\\\repo\\0hidden")),
            Err(WorkspaceFactoryError::InvalidSelection)
        );
        let overlong = format!("C:\\\\{}", "a".repeat(32_768));
        assert_eq!(
            validate_windows_selected_path(Path::new(&overlong)),
            Err(WorkspaceFactoryError::InvalidSelection)
        );
        let _ = PathBuf::new();
    }

    #[test]
    fn selected_path_validation_rejects_unpaired_utf16_surrogates() {
        use std::os::windows::ffi::OsStringExt;
        let invalid = std::ffi::OsString::from_wide(&[b'C' as u16, b':' as u16, b'\\\\' as u16, 0xD800]);
        let path = std::path::PathBuf::from(invalid);
        assert_eq!(
            validate_windows_selected_path(&path),
            Err(WorkspaceFactoryError::InvalidSelection)
        );
    }

    #[test]
    fn windows_path_normalization_accepts_drive_paths_and_extended_prefix() {
        assert_eq!(normalize_windows_drive_path(r"C:\").as_deref(), Some(r"C:\"));
        assert_eq!(
            normalize_windows_drive_path(r"C:\work\repo").as_deref(),
            Some(r"C:\work\repo")
        );
        assert_eq!(
            normalize_windows_drive_path(r"\\?\C:\work\repo").as_deref(),
            Some(r"C:\work\repo")
        );
    }

    #[test]
    fn windows_path_normalization_rejects_unc_and_traversal() {
        assert!(normalize_windows_drive_path(r"\\server\share\repo").is_none());
        assert!(normalize_windows_drive_path(r"C:\work\..\outside").is_none());
        assert!(normalize_windows_drive_path(r"C:\work\.\repo").is_none());
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod unix_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn factory_retains_a_selected_directory_and_rejects_symlink_roots() {
        let parent = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let selected = parent.path().join("selected");
        std::fs::create_dir(&selected).unwrap();
        std::fs::write(selected.join("probe.txt"), b"factory-probe").unwrap();

        let adapter = create_workspace_adapter(&selected, "factory-test-workspace")
            .expect("a real selected directory should produce a native adapter");
        let request = crate::snapshot_payload::SnapshotCaptureRequest::new("probe.txt", 1024)
            .unwrap();
        let snapshot = adapter.capture(&request).unwrap();
        assert_eq!(snapshot.bytes(), b"factory-probe");

        let outside = parent.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let link = parent.path().join("selected-link");
        symlink(&outside, &link).unwrap();
        assert!(create_workspace_adapter(&link, "symlink-workspace").is_err());
    }

    #[test]
    fn factory_rejects_empty_workspace_identity() {
        let parent = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        assert_eq!(
            create_workspace_adapter(parent.path(), "  ").err(),
            Some(WorkspaceFactoryError::InvalidSelection)
        );
    }
}
