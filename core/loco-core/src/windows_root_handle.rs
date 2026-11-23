//! Windows-only trusted-volume-root and handle-relative directory traversal prototype.
//!
//! This is deliberately not a WorkspaceSnapshotAdapter yet. It establishes a
//! narrow root capability and proves one-component directory opens. No writes,
//! file reads, path-based fallback, or authorization decisions are exposed.

use std::fs::File;
use std::mem::{size_of, zeroed};
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
const FILE_SHARE_DELETE_VALUE: u32 = 0x0004;
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
struct FileTime {
    low_date_time: u32,
    high_date_time: u32,
}

#[repr(C)]
struct ByHandleFileInformation {
    file_attributes: u32,
    creation_time: FileTime,
    last_access_time: FileTime,
    last_write_time: FileTime,
    volume_serial_number: u32,
    file_size_high: u32,
    file_size_low: u32,
    number_of_links: u32,
    file_index_high: u32,
    file_index_low: u32,
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

#[link(name = "kernel32")]
extern "system" {
    fn GetFileInformationByHandle(
        file_handle: HANDLE,
        information: *mut ByHandleFileInformation,
    ) -> i32;
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

        // A volume root itself cannot be renamed or deleted; allow normal sharing
        // here. Descendant directory handles deliberately omit FILE_SHARE_DELETE
        // so each traversed workspace component is pinned against replacement.
        // OPEN_REPARSE_POINT makes a reparse root inspectable rather than followed.
        // SAFETY: wide is NUL-terminated; null security/template handles are valid.
        let raw = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY_VALUE
                    | FILE_TRAVERSE_VALUE
                    | FILE_READ_ATTRIBUTES_VALUE
                    | SYNCHRONIZE_VALUE,
                FILE_SHARE_READ_VALUE | FILE_SHARE_WRITE_VALUE | FILE_SHARE_DELETE_VALUE,
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
    if !metadata.is_dir() {
        return Err(WindowsRootError::NotDirectory);
    }

    let mut information: ByHandleFileInformation = unsafe { zeroed() };
    // SAFETY: file owns a live Windows handle and information is writable for
    // the exact BY_HANDLE_FILE_INFORMATION-compatible layout declared above.
    let ok = unsafe {
        GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut information)
    };
    if ok == 0 {
        return Err(WindowsRootError::MetadataUnavailable);
    }
    if information.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(WindowsRootError::ReparsePointRejected);
    }
    if information.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
        return Err(WindowsRootError::NotDirectory);
    }
    let volume_serial = information.volume_serial_number;
    let file_index = ((information.file_index_high as u64) << 32)
        | information.file_index_low as u64;
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
    fn retained_directory_handle_blocks_workspace_rename() {
        use std::os::windows::ffi::OsStringExt;
        use std::path::Component;
        use windows_sys::Win32::Storage::FileSystem::{
            GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        };

        let parent = tempfile::tempdir().unwrap();
        let workspace = parent.path().join("workspace");
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
        let components: Vec<String> = relative
            .components()
            .filter_map(|part| match part {
                Component::Normal(value) => value.to_str().map(str::to_owned),
                _ => None,
            })
            .collect();
        let root = WindowsWorkspaceRoot::open(&volume_root).unwrap();
        let borrowed: Vec<&str> = components.iter().map(String::as_str).collect();
        let pinned = root.open_directory_components(&borrowed).unwrap();

        assert!(std::fs::rename(&workspace, parent.path().join("workspace-moved")).is_err(),
            "the retained directory handle must deny delete-sharing needed for rename");
        // Pin the final component itself, then attempt the classic
        // rename-and-replace attack against that exact directory entry.
        let child_path = workspace.join("child");
        let moved_child_path = workspace.join("child-moved");
        let pinned_child = pinned.open_child_directory("child").unwrap();
        assert!(std::fs::rename(&child_path, &moved_child_path).is_err(),
            "a retained child handle must deny delete-sharing needed for replacement");

        // A second handle-relative lookup must still resolve to the same object;
        // the attacker must not be able to substitute a new directory at "child".
        let child_again = pinned.open_child_directory("child").unwrap();
        assert!(pinned_child.is_directory());
        assert_eq!(pinned_child.identity(), child_again.identity());
    }

    #[test]
    fn rejects_directory_symlink_when_creation_is_permitted() {
        use std::os::windows::ffi::OsStringExt;
        use std::os::windows::fs::symlink_dir;
        use std::path::Component;
        use windows_sys::Win32::Storage::FileSystem::{
            GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        };

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let link = workspace.path().join("linked");
        if symlink_dir(outside.path(), &link).is_err() {
            // Some Windows runners do not grant symbolic-link creation rights.
            return;
        }

        let path_wide = to_wide_nul(workspace.path().to_string_lossy().as_ref());
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

        let relative = workspace.path().strip_prefix(&mount_path).unwrap();
        let mut components: Vec<String> = relative
            .components()
            .filter_map(|part| match part {
                Component::Normal(value) => value.to_str().map(str::to_owned),
                _ => None,
            })
            .collect();
        components.push("linked".into());
        let borrowed: Vec<&str> = components.iter().map(String::as_str).collect();
        let root = WindowsWorkspaceRoot::open(&volume_root).unwrap();
        assert!(root.open_directory_components(&borrowed).is_err(),
            "directory reparse points must fail closed");
    }


    // Creates a real NTFS mount-point reparse point (junction), not a
    // symbolic-link reparse point. The test exercises IO_REPARSE_TAG_MOUNT_POINT.
    fn create_junction(link: &std::path::Path, target: &std::path::Path) {
        use std::os::windows::ffi::OsStrExt;

        #[link(name = "kernel32")]
        extern "system" {
            fn DeviceIoControl(
                device: HANDLE,
                control_code: u32,
                in_buffer: *const std::ffi::c_void,
                in_buffer_size: u32,
                out_buffer: *mut std::ffi::c_void,
                out_buffer_size: u32,
                bytes_returned: *mut u32,
                overlapped: *mut std::ffi::c_void,
            ) -> i32;
        }

        const GENERIC_WRITE: u32 = 0x4000_0000;
        const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
        const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;

        std::fs::create_dir(link).unwrap();
        let link_wide: Vec<u16> = link.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: link_wide is NUL-terminated and remains alive for the call.
        let raw = unsafe {
            CreateFileW(
                link_wide.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_READ_VALUE | FILE_SHARE_WRITE_VALUE | FILE_SHARE_DELETE_VALUE,
                null(),
                OPEN_EXISTING_VALUE,
                FILE_FLAG_BACKUP_SEMANTICS_VALUE | FILE_FLAG_OPEN_REPARSE_POINT_VALUE,
                null_mut(),
            )
        };
        assert_ne!(raw, invalid_handle_value(), "open junction directory failed");
        // SAFETY: raw is a valid, uniquely owned handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };

        let canonical_target = std::fs::canonicalize(target).unwrap();
        let display_target = canonical_target
            .strip_prefix(r"\\?\")
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| canonical_target.to_string_lossy().into_owned());
        let substitute = format!(r"\??\{}", display_target);
        let substitute_units: Vec<u16> = substitute.encode_utf16().collect();
        let print_units: Vec<u16> = display_target.encode_utf16().collect();

        let substitute_len = (substitute_units.len() * 2) as u16;
        let print_offset = ((substitute_units.len() + 1) * 2) as u16;
        let print_len = (print_units.len() * 2) as u16;

        let mut path_units = substitute_units;
        path_units.push(0);
        path_units.extend(print_units);
        path_units.push(0);

        // REPARSE_DATA_BUFFER: tag + data length + reserved, then the
        // mount-point offsets/lengths and UTF-16 path buffer.
        let data_length = 8 + path_units.len() * 2;
        let mut buffer = Vec::with_capacity(16 + path_units.len() * 2);
        buffer.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        buffer.extend_from_slice(&(data_length as u16).to_le_bytes());
        buffer.extend_from_slice(&0u16.to_le_bytes());
        buffer.extend_from_slice(&0u16.to_le_bytes()); // SubstituteNameOffset
        buffer.extend_from_slice(&substitute_len.to_le_bytes());
        buffer.extend_from_slice(&print_offset.to_le_bytes());
        buffer.extend_from_slice(&print_len.to_le_bytes());
        for unit in path_units {
            buffer.extend_from_slice(&unit.to_le_bytes());
        }

        let mut bytes_returned = 0u32;
        // SAFETY: buffer is a correctly laid-out mount-point reparse buffer;
        // handle remains open and all output arguments are valid.
        let ok = unsafe {
            DeviceIoControl(
                handle.as_raw_handle() as HANDLE,
                FSCTL_SET_REPARSE_POINT,
                buffer.as_ptr() as *const std::ffi::c_void,
                buffer.len() as u32,
                null_mut(),
                0,
                &mut bytes_returned,
                null_mut(),
            )
        };
        assert_ne!(ok, 0, "setting junction mount-point reparse data failed");
    }

    #[test]
    fn rejects_ntfs_junction_mount_point_reparse_tag() {
        use std::os::windows::ffi::OsStringExt;
        use std::path::Component;
        use windows_sys::Win32::Storage::FileSystem::{
            GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        };

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        create_junction(&workspace.path().join("junction"), outside.path());

        let path_wide = to_wide_nul(workspace.path().to_string_lossy().as_ref());
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

        let relative = workspace.path().strip_prefix(&mount_path).unwrap();
        let components: Vec<String> = relative
            .components()
            .filter_map(|part| match part {
                Component::Normal(value) => value.to_str().map(str::to_owned),
                _ => None,
            })
            .collect();
        let root = WindowsWorkspaceRoot::open(&volume_root).unwrap();
        let borrowed: Vec<&str> = components.iter().map(String::as_str).collect();
        let pinned_workspace = root.open_directory_components(&borrowed).unwrap();
        assert!(pinned_workspace.open_child_directory("junction").is_err(),
            "an NTFS mount-point junction must not be traversed");
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
