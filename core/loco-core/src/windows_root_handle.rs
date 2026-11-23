//! Windows-only trusted-volume-root and handle-relative directory traversal prototype.
//!
//! This is deliberately not a WorkspaceSnapshotAdapter yet. It establishes a
//! narrow root capability and proves one-component directory opens. No writes,
//! file reads, path-based fallback, or authorization decisions are exposed.

use std::fs::File;
use std::mem::{size_of, zeroed};
use std::os::windows::fs::MetadataExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetDriveTypeW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
};

const DRIVE_FIXED_VALUE: u32 = 3;
const FILE_LIST_DIRECTORY_VALUE: u32 = 0x0001;
const FILE_TRAVERSE_VALUE: u32 = 0x0020;
const FILE_READ_ATTRIBUTES_VALUE: u32 = 0x0080;
const SYNCHRONIZE_VALUE: u32 = 0x0010_0000;
const FILE_SHARE_READ_VALUE: u32 = 0x0001;
const FILE_SHARE_WRITE_VALUE: u32 = 0x0002;
const OPEN_EXISTING_VALUE: u32 = 3;
const FILE_FLAG_OPEN_REPARSE_POINT_VALUE: u32 = 0x0020_0000;
const FILE_FLAG_BACKUP_SEMANTICS_VALUE: u32 = 0x0200_0000;

const OBJ_CASE_INSENSITIVE: u32 = 0x0000_0040;
const OBJ_DONT_REPARSE: u32 = 0x0000_1000;
const FILE_OPEN: u32 = 1;
const FILE_DIRECTORY_FILE: u32 = 0x0000_0001;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
const STATUS_REPARSE_POINT_ENCOUNTERED: i32 = 0xC000_050B_u32 as i32;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: HANDLE,
    object_name: *mut UnicodeString,
    attributes: u32,
    security_descriptor: *mut std::ffi::c_void,
    security_quality_of_service: *mut std::ffi::c_void,
}

#[repr(C)]
union IoStatusValue {
    status: i32,
    pointer: *mut std::ffi::c_void,
}

#[repr(C)]
struct IoStatusBlock {
    value: IoStatusValue,
    information: usize,
}

#[link(name = "ntdll")]
extern "system" {
    fn NtCreateFile(
        file_handle: *mut HANDLE,
        desired_access: u32,
        object_attributes: *mut ObjectAttributes,
        io_status_block: *mut IoStatusBlock,
        allocation_size: *mut i64,
        file_attributes: u32,
        share_access: u32,
        create_disposition: u32,
        create_options: u32,
        ea_buffer: *mut std::ffi::c_void,
        ea_length: u32,
    ) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsRootError {
    InvalidVolumeGuidRoot,
    UnsupportedVolumeType,
    RootAcquisitionFailed,
    InvalidComponent,
    RelativeOpenFailed,
    ReparsePointRejected,
    NotDirectory,
    VolumeIdentityChanged,
    MetadataUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsFileIdentity {
    pub volume_serial: u32,
    pub file_index: u64,
}

pub struct WindowsWorkspaceRoot {
    root: File,
    identity: WindowsFileIdentity,
}

pub struct WindowsDirectoryHandle {
    handle: File,
    identity: WindowsFileIdentity,
}

impl WindowsWorkspaceRoot {
    /// Opens only a canonical local volume-GUID root, never a caller-supplied
    /// workspace path. The retained handle is the authority for later opens.
    pub fn open(volume_guid_root: &str) -> Result<Self, WindowsRootError> {
        if !is_volume_guid_root(volume_guid_root) {
            return Err(WindowsRootError::InvalidVolumeGuidRoot);
        }

        let wide = to_wide_nul(volume_guid_root);
        // SAFETY: wide is NUL-terminated and remains alive for the call.
        let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
        if drive_type != DRIVE_FIXED_VALUE {
            return Err(WindowsRootError::UnsupportedVolumeType);
        }

        // Share read/write but not delete. This pins the root against rename or
        // deletion while the capability is alive; sharing violations fail closed.
        // OPEN_REPARSE_POINT makes a reparse root inspectable rather than followed.
        // SAFETY: wide is NUL-terminated; null security/template handles are valid.
        let raw = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY_VALUE
                    | FILE_TRAVERSE_VALUE
                    | FILE_READ_ATTRIBUTES_VALUE
                    | SYNCHRONIZE_VALUE,
                FILE_SHARE_READ_VALUE | FILE_SHARE_WRITE_VALUE,
                null(),
                OPEN_EXISTING_VALUE,
                FILE_FLAG_BACKUP_SEMANTICS_VALUE | FILE_FLAG_OPEN_REPARSE_POINT_VALUE,
                null_mut(),
            )
        };
        if raw == invalid_handle_value() {
            return Err(WindowsRootError::RootAcquisitionFailed);
        }

        // SAFETY: raw is a valid, uniquely owned handle returned by CreateFileW.
        let root = unsafe { File::from(OwnedHandle::from_raw_handle(raw as RawHandle)) };
        let identity = checked_directory_identity(&root, None)?;
        Ok(Self { root, identity })
    }

    pub fn identity(&self) -> WindowsFileIdentity {
        self.identity
    }

    /// Opens validated directory components one at a time relative to the
    /// currently held directory handle. Parent handles remain open until their
    /// child has been opened; no concatenated path is ever passed to Windows.
    pub fn open_directory_components(
        &self,
        components: &[&str],
    ) -> Result<WindowsDirectoryHandle, WindowsRootError> {
        if components.is_empty() {
            return Err(WindowsRootError::InvalidComponent);
        }

        let mut parent = self.root.try_clone().map_err(|_| WindowsRootError::MetadataUnavailable)?;
        for component in components {
            validate_component(component)?;
            let child = open_relative_directory(&parent, component)?;
            let identity = checked_directory_identity(&child, Some(self.identity.volume_serial))?;
            parent = child;
            if identity.volume_serial != self.identity.volume_serial {
                return Err(WindowsRootError::VolumeIdentityChanged);
            }
        }

        let identity = checked_directory_identity(&parent, Some(self.identity.volume_serial))?;
        Ok(WindowsDirectoryHandle { handle: parent, identity })
    }
}

impl WindowsDirectoryHandle {
    pub fn identity(&self) -> WindowsFileIdentity {
        self.identity
    }

    /// Opens one child directory relative to this retained handle.
    pub fn open_child_directory(&self, component: &str) -> Result<Self, WindowsRootError> {
        validate_component(component)?;
        let child = open_relative_directory(&self.handle, component)?;
        let identity = checked_directory_identity(&child, Some(self.identity.volume_serial))?;
        Ok(Self { handle: child, identity })
    }

    /// Exposes metadata from the retained handle for diagnostics/tests only.
    pub fn is_directory(&self) -> bool {
        self.handle.metadata().map(|metadata| metadata.is_dir()).unwrap_or(false)
    }
}

fn open_relative_directory(parent: &File, component: &str) -> Result<File, WindowsRootError> {
    let mut name: Vec<u16> = component.encode_utf16().collect();
    let byte_len = name.len().checked_mul(2).ok_or(WindowsRootError::InvalidComponent)?;
    if byte_len == 0 || byte_len > u16::MAX as usize {
        return Err(WindowsRootError::InvalidComponent);
    }

    let mut unicode_name = UnicodeString {
        length: byte_len as u16,
        maximum_length: byte_len as u16,
        buffer: name.as_mut_ptr(),
    };
    // OBJ_DONT_REPARSE requests fail-closed name parsing. The returned handle is
    // still inspected for FILE_ATTRIBUTE_REPARSE_POINT; the flag alone is not
    // treated as proof. This combination requires runtime validation on Windows.
    let mut attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: parent.as_raw_handle() as HANDLE,
        object_name: &mut unicode_name,
        attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        security_descriptor: null_mut(),
        security_quality_of_service: null_mut(),
    };
    let mut io_status: IoStatusBlock = unsafe { zeroed() };
    let mut raw_handle: HANDLE = null_mut();

    // SAFETY: all pointers refer to live, correctly laid-out structures for the
    // duration of the synchronous call. The name is one validated component,
    // and RootDirectory is an owned directory handle.
    let status = unsafe {
        NtCreateFile(
            &mut raw_handle,
            FILE_LIST_DIRECTORY_VALUE
                | FILE_TRAVERSE_VALUE
                | FILE_READ_ATTRIBUTES_VALUE
                | SYNCHRONIZE_VALUE,
            &mut attributes,
            &mut io_status,
            null_mut(),
            0,
            FILE_SHARE_READ_VALUE | FILE_SHARE_WRITE_VALUE,
            FILE_OPEN,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
            null_mut(),
            0,
        )
    };

    if status == STATUS_REPARSE_POINT_ENCOUNTERED {
        return Err(WindowsRootError::ReparsePointRejected);
    }
    if status < 0 || raw_handle.is_null() {
        return Err(WindowsRootError::RelativeOpenFailed);
    }

    // SAFETY: successful NtCreateFile returns a uniquely owned kernel handle.
    let child = unsafe { File::from(OwnedHandle::from_raw_handle(raw_handle as RawHandle)) };
    checked_directory_identity(&child, None)?;
    Ok(child)
}

fn checked_directory_identity(
    file: &File,
    expected_volume: Option<u32>,
) -> Result<WindowsFileIdentity, WindowsRootError> {
    let metadata = file.metadata().map_err(|_| WindowsRootError::MetadataUnavailable)?;
    let attributes = metadata.file_attributes();
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(WindowsRootError::ReparsePointRejected);
    }
    if !metadata.is_dir() || attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
        return Err(WindowsRootError::NotDirectory);
    }
    let volume_serial = metadata.volume_serial_number().ok_or(WindowsRootError::MetadataUnavailable)?;
    let file_index = metadata.file_index().ok_or(WindowsRootError::MetadataUnavailable)?;
    if expected_volume.is_some_and(|expected| expected != volume_serial) {
        return Err(WindowsRootError::VolumeIdentityChanged);
    }
    Ok(WindowsFileIdentity { volume_serial, file_index })
}

fn validate_component(component: &str) -> Result<(), WindowsRootError> {
    if component.is_empty()
        || component == "."
        || component == ".."
        || component.contains('/')
        || component.contains('\\')
        || !crate::portable_path::is_safe_portable_relative_path(component)
    {
        return Err(WindowsRootError::InvalidComponent);
    }
    Ok(())
}

fn is_volume_guid_root(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 49
        || !value.starts_with(r"\\?\Volume{")
        || !value.ends_with(r"}\")
    {
        return false;
    }
    let guid = &bytes[11..47];
    guid.iter().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            *byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    })
}

fn to_wide_nul(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn invalid_handle_value() -> HANDLE {
    -1isize as HANDLE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_canonical_volume_guid_roots() {
        assert!(is_volume_guid_root(r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\"));
        assert!(!is_volume_guid_root(r"C:\"));
        assert!(!is_volume_guid_root(r"\\?\Volume{0123456789abcdef0123456789abcdef}\"));
        assert!(!is_volume_guid_root(r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}"));
    }

    #[test]
    fn rejects_unsafe_or_multi_component_names() {
        for component in ["", ".", "..", "a/b", r"a\b", "file:stream", "CON.txt", "name."] {
            assert_eq!(validate_component(component), Err(WindowsRootError::InvalidComponent));
        }
        assert_eq!(validate_component("src"), Ok(()));
    }

    #[test]
    fn retained_directory_handle_survives_path_rename() {
        use std::os::windows::ffi::OsStringExt;
        use std::path::Component;
        use windows_sys::Win32::Storage::FileSystem::{
            GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        };

        let parent = tempfile::tempdir().unwrap();
        let workspace = parent.path().join("workspace");
        let moved = parent.path().join("workspace-moved");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::create_dir(workspace.join("child")).unwrap();

        let path_wide = to_wide_nul(parent.path().to_string_lossy().as_ref());
        let mut mount = vec![0u16; 1024];
        // SAFETY: buffers are writable and lengths are UTF-16 code-unit counts.
        let ok = unsafe { GetVolumePathNameW(path_wide.as_ptr(), mount.as_mut_ptr(), mount.len() as u32) };
        assert_ne!(ok, 0);
        let mount_end = mount.iter().position(|value| *value == 0).unwrap();
        let mount_path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&mount[..mount_end]));
        let mut volume = vec![0u16; 128];
        // SAFETY: mount is NUL-terminated and volume is writable.
        let ok = unsafe {
            GetVolumeNameForVolumeMountPointW(mount.as_ptr(), volume.as_mut_ptr(), volume.len() as u32)
        };
        assert_ne!(ok, 0);
        let volume_end = volume.iter().position(|value| *value == 0).unwrap();
        let volume_root = String::from_utf16(&volume[..volume_end]).unwrap();

        let relative = workspace.strip_prefix(&mount_path).unwrap();
        let mut components: Vec<String> = relative
            .components()
            .filter_map(|part| match part {
                Component::Normal(value) => value.to_str().map(str::to_owned),
                _ => None,
            })
            .collect();
        let root = WindowsWorkspaceRoot::open(&volume_root).unwrap();
        let borrowed: Vec<&str> = components.iter().map(String::as_str).collect();
        let pinned = root.open_directory_components(&borrowed).unwrap();

        std::fs::rename(&workspace, &moved).unwrap();
        let child = pinned.open_child_directory("child").unwrap();
        let expected = std::fs::metadata(moved.join("child")).unwrap();
        assert_eq!(child.identity().volume_serial, expected.volume_serial_number().unwrap());
        assert_eq!(child.identity().file_index, expected.file_index().unwrap());
        components.clear();
    }

    #[test]
    fn opens_current_temp_directory_from_its_volume_root() {
        use std::os::windows::ffi::OsStringExt;
        use std::path::Component;
        use windows_sys::Win32::Storage::FileSystem::{
            GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        };

        let temp = tempfile::tempdir().unwrap();
        let path_wide = to_wide_nul(temp.path().to_string_lossy().as_ref());
        let mut mount = vec![0u16; 1024];
        // SAFETY: buffers are writable and lengths are expressed in UTF-16 code units.
        let ok = unsafe { GetVolumePathNameW(path_wide.as_ptr(), mount.as_mut_ptr(), mount.len() as u32) };
        assert_ne!(ok, 0, "GetVolumePathNameW failed");
        let mount_end = mount.iter().position(|value| *value == 0).unwrap();
        let mount_path = std::ffi::OsString::from_wide(&mount[..mount_end]);
        let mount_path = std::path::PathBuf::from(mount_path);

        let mut volume = vec![0u16; 128];
        // SAFETY: mount contains a NUL-terminated volume mount path and volume is writable.
        let ok = unsafe {
            GetVolumeNameForVolumeMountPointW(mount.as_ptr(), volume.as_mut_ptr(), volume.len() as u32)
        };
        assert_ne!(ok, 0, "GetVolumeNameForVolumeMountPointW failed");
        let volume_end = volume.iter().position(|value| *value == 0).unwrap();
        let volume_root = String::from_utf16(&volume[..volume_end]).unwrap();

        let relative = temp.path().strip_prefix(&mount_path).unwrap();
        let components: Vec<String> = relative
            .components()
            .filter_map(|part| match part {
                Component::Normal(value) => value.to_str().map(str::to_owned),
                _ => None,
            })
            .collect();
        assert!(!components.is_empty(), "temp directory unexpectedly equals volume root");
        let borrowed: Vec<&str> = components.iter().map(String::as_str).collect();

        let root = WindowsWorkspaceRoot::open(&volume_root).unwrap();
        let opened = root.open_directory_components(&borrowed).unwrap();
        assert!(opened.is_directory());
        assert_eq!(opened.identity().volume_serial, root.identity().volume_serial);
    }
}
