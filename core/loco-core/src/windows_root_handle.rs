//! Windows-only trusted-volume-root and handle-relative directory traversal prototype.
//!
//! Read-only Windows workspace acquisition prototype. It pins a workspace
//! directory beneath a trusted local volume root and captures bounded regular
//! files through handle-relative opens. No writes or authorization decisions.

use std::fs::File;
use std::mem::{size_of, zeroed};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr::{null, null_mut};

// The manual NT native declarations below are currently validated for Windows x64 only.
// Other architectures must not compile against unverified structure layouts/calling conventions.
#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
compile_error!("LOCO Windows native filesystem adapter currently supports x86_64 only");
use std::io::Read;

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


// Compile-time ABI gate for the manually declared Windows SDK/NT structures.
// Values are from the Windows x64 ABI; keep these assertions adjacent to the
// declarations so a layout change cannot silently alter NtCreateFile calls.
const _: () = {
    use std::mem::{align_of, offset_of, size_of};

    assert!(size_of::<UnicodeString>() == 16);
    assert!(align_of::<UnicodeString>() == 8);
    assert!(offset_of!(UnicodeString, length) == 0);
    assert!(offset_of!(UnicodeString, maximum_length) == 2);
    assert!(offset_of!(UnicodeString, buffer) == 8);

    assert!(size_of::<ObjectAttributes>() == 48);
    assert!(align_of::<ObjectAttributes>() == 8);
    assert!(offset_of!(ObjectAttributes, length) == 0);
    assert!(offset_of!(ObjectAttributes, root_directory) == 8);
    assert!(offset_of!(ObjectAttributes, object_name) == 16);
    assert!(offset_of!(ObjectAttributes, attributes) == 24);
    assert!(offset_of!(ObjectAttributes, security_descriptor) == 32);
    assert!(offset_of!(ObjectAttributes, security_quality_of_service) == 40);

    assert!(size_of::<FileTime>() == 8);
    assert!(align_of::<FileTime>() == 4);
    assert!(offset_of!(FileTime, low_date_time) == 0);
    assert!(offset_of!(FileTime, high_date_time) == 4);

    assert!(size_of::<ByHandleFileInformation>() == 52);
    assert!(align_of::<ByHandleFileInformation>() == 4);
    assert!(offset_of!(ByHandleFileInformation, file_attributes) == 0);
    assert!(offset_of!(ByHandleFileInformation, creation_time) == 4);
    assert!(offset_of!(ByHandleFileInformation, last_access_time) == 12);
    assert!(offset_of!(ByHandleFileInformation, last_write_time) == 20);
    assert!(offset_of!(ByHandleFileInformation, volume_serial_number) == 28);
    assert!(offset_of!(ByHandleFileInformation, file_size_high) == 32);
    assert!(offset_of!(ByHandleFileInformation, file_size_low) == 36);
    assert!(offset_of!(ByHandleFileInformation, number_of_links) == 40);
    assert!(offset_of!(ByHandleFileInformation, file_index_high) == 44);
    assert!(offset_of!(ByHandleFileInformation, file_index_low) == 48);

    assert!(size_of::<IoStatusValue>() == 8);
    assert!(align_of::<IoStatusValue>() == 8);
    assert!(offset_of!(IoStatusBlock, value) == 0);
    assert!(offset_of!(IoStatusBlock, information) == 8);
    assert!(size_of::<IoStatusBlock>() == 16);
    assert!(align_of::<IoStatusBlock>() == 8);
};

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
    NotRegularFile,
    InvalidWorkspaceId,
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


/// Read-only Windows adapter that captures regular files through retained
/// volume/workspace directory handles and validates the shared snapshot payload.
pub struct WindowsWorkspaceSnapshotAdapter {
    workspace: WindowsDirectoryHandle,
    volume_serial: u32,
    workspace_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowsFileMetadata {
    identity: WindowsFileIdentity,
    attributes: u32,
    size: u64,
    links: u32,
    creation_time: (u32, u32),
    last_write_time: (u32, u32),
}

pub struct WindowsFileHandle {
    handle: File,
    metadata: WindowsFileMetadata,
}

impl WindowsWorkspaceSnapshotAdapter {
    /// Binds the adapter to an explicit workspace path relative to the trusted
    /// volume-GUID root. Every component is opened and retained by handle.
    pub fn new(
        volume_guid_root: &str,
        workspace_relative_path: &str,
        workspace_id: impl Into<String>,
    ) -> Result<Self, WindowsRootError> {
        let workspace_id = workspace_id.into();
        if workspace_id.trim().is_empty() {
            return Err(WindowsRootError::InvalidWorkspaceId);
        }
        let components: Vec<&str> = workspace_relative_path.split('/').collect();
        if components.is_empty() || components.iter().any(|part| validate_component(part).is_err()) {
            return Err(WindowsRootError::InvalidComponent);
        }
        let root = WindowsWorkspaceRoot::open(volume_guid_root)?;
        let volume_serial = root.identity.volume_serial;
        let workspace = root.open_directory_components(&components)?;
        Ok(Self { workspace, volume_serial, workspace_id })
    }
}

impl crate::snapshot_payload::WorkspaceSnapshotAdapter for WindowsWorkspaceSnapshotAdapter {
    fn capture(
        &self,
        request: &crate::snapshot_payload::SnapshotCaptureRequest,
    ) -> Result<crate::snapshot_payload::CapturedWorkspaceSnapshot, crate::snapshot_payload::SnapshotPayloadError> {
        use crate::snapshot_payload::{build_snapshot_contract, CapturedWorkspaceSnapshot, SnapshotPayloadError, MAX_SNAPSHOT_BYTES};

        let components: Vec<&str> = request.relative_path().split('/').collect();
        if components.is_empty() || components.iter().any(|part| validate_component(part).is_err()) {
            return Err(SnapshotPayloadError::AcquisitionFailed);
        }

        let mut parent = self.workspace.handle.try_clone().map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
        for component in &components[..components.len() - 1] {
            let child = open_relative_directory(&parent, component)
                .map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
            let identity = checked_directory_identity(&child, Some(self.volume_serial))
                .map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
            if identity.volume_serial != self.volume_serial {
                return Err(SnapshotPayloadError::AcquisitionFailed);
            }
            parent = child;
        }

        let mut file = open_relative_file(&parent, components[components.len() - 1])
            .map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
        if file.metadata.identity.volume_serial != self.volume_serial {
            return Err(SnapshotPayloadError::AcquisitionFailed);
        }
        let before = file.metadata;
        if before.links != 1 {
            return Err(SnapshotPayloadError::HardLinkNotAllowed);
        }
        let limit = MAX_SNAPSHOT_BYTES.min(request.max_bytes());
        if before.size > limit as u64 {
            return Err(SnapshotPayloadError::LimitExceeded);
        }

        let mut bytes = Vec::with_capacity(before.size as usize);
        (&mut file.handle).take(limit as u64 + 1).read_to_end(&mut bytes)
            .map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
        if bytes.len() > limit {
            return Err(SnapshotPayloadError::LimitExceeded);
        }
        if bytes.len() as u64 != before.size {
            return Err(SnapshotPayloadError::AcquisitionFailed);
        }

        let after = query_regular_file_metadata(&file.handle)
            .map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
        if before != after {
            return Err(SnapshotPayloadError::AcquisitionFailed);
        }

        let identity = format!(
            "windows:volume:{}:file:{}:captured_len:{}",
            before.identity.volume_serial, before.identity.file_index, bytes.len()
        );
        let snapshot = build_snapshot_contract(
            &self.workspace_id,
            request.relative_path(),
            &identity,
            &bytes,
        )?;
        CapturedWorkspaceSnapshot::new(snapshot, bytes)
    }
}

impl WindowsFileHandle {
    pub fn identity(&self) -> WindowsFileIdentity { self.metadata.identity }
    pub fn byte_length(&self) -> u64 { self.metadata.size }
}

fn open_relative_file(parent: &File, component: &str) -> Result<WindowsFileHandle, WindowsRootError> {
    validate_component(component)?;
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
    const FILE_READ_DATA_VALUE: u32 = 0x0001;
    const FILE_NON_DIRECTORY_FILE_VALUE: u32 = 0x0040;
    let status = unsafe {
        NtCreateFile(
            &mut raw_handle,
            FILE_READ_DATA_VALUE | FILE_READ_ATTRIBUTES_VALUE | SYNCHRONIZE_VALUE,
            &mut attributes,
            &mut io_status,
            null_mut(),
            0,
            FILE_SHARE_READ_VALUE,
            FILE_OPEN,
            FILE_NON_DIRECTORY_FILE_VALUE | FILE_SYNCHRONOUS_IO_NONALERT,
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
    let handle = unsafe { File::from(OwnedHandle::from_raw_handle(raw_handle as RawHandle)) };
    let metadata = query_regular_file_metadata(&handle)?;
    Ok(WindowsFileHandle { handle, metadata })
}

fn query_regular_file_metadata(file: &File) -> Result<WindowsFileMetadata, WindowsRootError> {
    let mut information: ByHandleFileInformation = unsafe { zeroed() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut information) };
    if ok == 0 {
        return Err(WindowsRootError::MetadataUnavailable);
    }
    if information.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(WindowsRootError::ReparsePointRejected);
    }
    if information.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        return Err(WindowsRootError::NotRegularFile);
    }
    let metadata = file.metadata().map_err(|_| WindowsRootError::MetadataUnavailable)?;
    if !metadata.is_file() {
        return Err(WindowsRootError::NotRegularFile);
    }
    let identity = WindowsFileIdentity {
        volume_serial: information.volume_serial_number,
        file_index: ((information.file_index_high as u64) << 32) | information.file_index_low as u64,
    };
    Ok(WindowsFileMetadata {
        identity,
        attributes: information.file_attributes,
        size: ((information.file_size_high as u64) << 32) | information.file_size_low as u64,
        links: information.number_of_links,
        creation_time: (information.creation_time.high_date_time, information.creation_time.low_date_time),
        last_write_time: (information.last_write_time.high_date_time, information.last_write_time.low_date_time),
    })
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
    fn rejects_volume_mount_paths_as_trusted_volume_roots() {
        // A mounted-folder path (and ordinary drive-letter root) is not the
        // trusted volume identity. Callers must resolve the containing volume
        // to its canonical \\?\\Volume{GUID}\\ root before acquisition.
        let workspace = tempfile::tempdir().unwrap();
        let mounted_folder_path = workspace.path().to_string_lossy();
        assert!(
            matches!(
                WindowsWorkspaceRoot::open(&mounted_folder_path),
                Err(WindowsRootError::InvalidVolumeGuidRoot)
            ),
            "a DOS/mounted-folder path must never be accepted as the trusted volume root"
        );

        assert!(!is_volume_guid_root(r"\\?\C:\mounted-volume\"));
        assert!(!is_volume_guid_root(r"\\server\share\"));
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

    fn test_workspace_relative_path(path: &std::path::Path) -> String {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::Storage::FileSystem::GetVolumePathNameW;
        let path_wide = to_wide_nul(path.to_string_lossy().as_ref());
        let mut mount = vec![0u16; 1024];
        let ok = unsafe { GetVolumePathNameW(path_wide.as_ptr(), mount.as_mut_ptr(), mount.len() as u32) };
        assert_ne!(ok, 0);
        let end = mount.iter().position(|value| *value == 0).unwrap();
        let mount_path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&mount[..end]));
        let relative = path.strip_prefix(&mount_path).unwrap();
        relative.components().filter_map(|part| match part {
            std::path::Component::Normal(value) => value.to_str().map(str::to_owned),
            _ => None,
        }).collect::<Vec<_>>().join("/")
    }

    fn test_volume_root_for(path: &std::path::Path) -> String {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::Storage::FileSystem::{
            GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        };
        let path_wide = to_wide_nul(path.to_string_lossy().as_ref());
        let mut mount = vec![0u16; 1024];
        let ok = unsafe { GetVolumePathNameW(path_wide.as_ptr(), mount.as_mut_ptr(), mount.len() as u32) };
        assert_ne!(ok, 0);
        let mut volume = vec![0u16; 128];
        let ok = unsafe { GetVolumeNameForVolumeMountPointW(mount.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) };
        assert_ne!(ok, 0);
        let end = volume.iter().position(|value| *value == 0).unwrap();
        String::from_utf16(&volume[..end]).unwrap()
    }

    #[test]
    fn captures_regular_and_empty_files_with_shared_snapshot_contract() {
        use crate::snapshot_payload::{SnapshotCaptureRequest, WorkspaceSnapshotAdapter};
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("sample.txt"), b"bounded capture").unwrap();
        std::fs::write(workspace.path().join("empty.txt"), b"").unwrap();
        let adapter = WindowsWorkspaceSnapshotAdapter::new(
            &test_volume_root_for(workspace.path()),
            &test_workspace_relative_path(workspace.path()),
            "windows-test-workspace"
        ).unwrap();

        let captured = adapter.capture(&SnapshotCaptureRequest::new("sample.txt", 128).unwrap()).unwrap();
        assert_eq!(captured.bytes(), b"bounded capture");
        assert_eq!(captured.snapshot().byte_length, 15);
        assert_eq!(crate::snapshot_payload::validate_snapshot_payload(captured.snapshot(), captured.bytes()), Ok(()));

        let empty = adapter.capture(&SnapshotCaptureRequest::new("empty.txt", 1).unwrap()).unwrap();
        assert!(empty.bytes().is_empty());
        assert_eq!(empty.snapshot().byte_length, 0);
    }

    #[test]
    fn rejects_oversized_and_hard_linked_files() {
        use crate::snapshot_payload::{SnapshotCaptureRequest, SnapshotPayloadError, WorkspaceSnapshotAdapter};
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("large.bin"), b"12345").unwrap();
        std::fs::write(workspace.path().join("origin.bin"), b"linked").unwrap();
        std::fs::hard_link(workspace.path().join("origin.bin"), workspace.path().join("alias.bin")).unwrap();
        let adapter = WindowsWorkspaceSnapshotAdapter::new(
            &test_volume_root_for(workspace.path()),
            &test_workspace_relative_path(workspace.path()),
            "windows-test-workspace"
        ).unwrap();

        assert!(matches!(
            adapter.capture(&SnapshotCaptureRequest::new("large.bin", 4).unwrap()),
            Err(SnapshotPayloadError::LimitExceeded)
        ));
        assert!(matches!(
            adapter.capture(&SnapshotCaptureRequest::new("alias.bin", 64).unwrap()),
            Err(SnapshotPayloadError::HardLinkNotAllowed)
        ));
    }

    #[test]
    fn rejects_directories_and_pins_open_file_against_rename() {
        use crate::snapshot_payload::{SnapshotCaptureRequest, WorkspaceSnapshotAdapter};
        let workspace = tempfile::tempdir().unwrap();
        std::fs::create_dir(workspace.path().join("folder")).unwrap();
        std::fs::write(workspace.path().join("stable.txt"), b"stable").unwrap();
        let adapter = WindowsWorkspaceSnapshotAdapter::new(
            &test_volume_root_for(workspace.path()),
            &test_workspace_relative_path(workspace.path()),
            "windows-test-workspace"
        ).unwrap();
        assert!(adapter.capture(&SnapshotCaptureRequest::new("folder", 64).unwrap()).is_err());

        let root_components = {
            use std::path::Component;
            let mount_path = {
                use std::os::windows::ffi::OsStringExt;
                use windows_sys::Win32::Storage::FileSystem::GetVolumePathNameW;
                let wide = to_wide_nul(workspace.path().to_string_lossy().as_ref());
                let mut mount = vec![0u16; 1024];
                let ok = unsafe { GetVolumePathNameW(wide.as_ptr(), mount.as_mut_ptr(), mount.len() as u32) };
                assert_ne!(ok, 0);
                let end = mount.iter().position(|value| *value == 0).unwrap();
                std::path::PathBuf::from(std::ffi::OsString::from_wide(&mount[..end]))
            };
            workspace.path().strip_prefix(&mount_path).unwrap().components()
                .filter_map(|part| match part {
                    Component::Normal(value) => value.to_str().map(str::to_owned),
                    _ => None,
                }).collect::<Vec<_>>()
        };
        let root = WindowsWorkspaceRoot::open(&test_volume_root_for(workspace.path())).unwrap();
        let refs: Vec<&str> = root_components.iter().map(String::as_str).collect();
        let parent = root.open_directory_components(&refs).unwrap();
        let pinned = open_relative_file(&parent.handle, "stable.txt").unwrap();
        assert!(std::fs::rename(
            workspace.path().join("stable.txt"),
            workspace.path().join("moved.txt")
        ).is_err(), "open file handle must deny delete sharing");
        assert!(std::fs::write(workspace.path().join("stable.txt"), b"changed").is_err(),
            "open file handle must deny concurrent write sharing");
        assert_eq!(pinned.byte_length(), 6);
    }


    #[test]
    fn snapshot_adapter_rejects_intermediate_ntfs_junction() {
        use crate::snapshot_payload::{SnapshotCaptureRequest, SnapshotPayloadError, WorkspaceSnapshotAdapter};

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("target.txt"), b"OUTSIDE-SENTINEL").unwrap();
        create_junction(&workspace.path().join("junction"), outside.path());

        let adapter = WindowsWorkspaceSnapshotAdapter::new(
            &test_volume_root_for(workspace.path()),
            &test_workspace_relative_path(workspace.path()),
            "windows-junction-workspace",
        ).unwrap();

        assert!(matches!(
            adapter.capture(&SnapshotCaptureRequest::new("junction/target.txt", 128).unwrap()),
            Err(SnapshotPayloadError::AcquisitionFailed)
        ), "snapshot capture must not traverse an intermediate mount-point junction");
    }


    #[test]
    fn snapshot_adapter_rejects_intermediate_directory_symlink_reparse_tag() {
        use crate::snapshot_payload::{SnapshotCaptureRequest, SnapshotPayloadError, WorkspaceSnapshotAdapter};
        use std::os::windows::fs::symlink_dir;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("target.txt"), b"OUTSIDE-SENTINEL").unwrap();
        symlink_dir(outside.path(), workspace.path().join("linked"))
            .expect("Windows CI must permit directory symlink creation for this adversarial test");

        let adapter = WindowsWorkspaceSnapshotAdapter::new(
            &test_volume_root_for(workspace.path()),
            &test_workspace_relative_path(workspace.path()),
            "windows-symlink-workspace",
        ).unwrap();

        assert!(matches!(
            adapter.capture(&SnapshotCaptureRequest::new("linked/target.txt", 128).unwrap()),
            Err(SnapshotPayloadError::AcquisitionFailed)
        ), "snapshot capture must reject an intermediate directory symlink");
    }

    #[test]
    fn snapshot_adapter_rejects_final_file_symlink_reparse_tag() {
        use crate::snapshot_payload::{SnapshotCaptureRequest, SnapshotPayloadError, WorkspaceSnapshotAdapter};
        use std::os::windows::fs::symlink_file;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("target.txt");
        std::fs::write(&outside_file, b"OUTSIDE-SENTINEL").unwrap();
        symlink_file(&outside_file, workspace.path().join("linked.txt"))
            .expect("Windows CI must permit file symlink creation for this adversarial test");

        let adapter = WindowsWorkspaceSnapshotAdapter::new(
            &test_volume_root_for(workspace.path()),
            &test_workspace_relative_path(workspace.path()),
            "windows-final-symlink-workspace",
        ).unwrap();

        assert!(matches!(
            adapter.capture(&SnapshotCaptureRequest::new("linked.txt", 128).unwrap()),
            Err(SnapshotPayloadError::AcquisitionFailed)
        ), "snapshot capture must reject a final-file symlink rather than read its target");
    }


    #[test]
    fn final_file_mutation_race_never_captures_partial_content() {
        use crate::snapshot_payload::{SnapshotCaptureRequest, SnapshotPayloadError, WorkspaceSnapshotAdapter};
        use std::sync::{Arc, Barrier};
        use std::thread;

        const CONTENT_A: &[u8] = b"complete-original-content-aaaaaaaaaaaaaaaa";
        const CONTENT_B: &[u8] = b"complete-replacement-content-bbbbbbbbbbbb";

        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("mutable.txt"), CONTENT_A).unwrap();
        let adapter = Arc::new(WindowsWorkspaceSnapshotAdapter::new(
            &test_volume_root_for(workspace.path()),
            &test_workspace_relative_path(workspace.path()),
            "windows-final-mutation-workspace",
        ).unwrap());

        let barrier = Arc::new(Barrier::new(2));
        let writer_path = workspace.path().join("mutable.txt");
        let writer_barrier = barrier.clone();
        let writer = thread::spawn(move || {
            writer_barrier.wait();
            let mut successful_writes = 0usize;
            for index in 0..300 {
                let content = if index % 2 == 0 { CONTENT_B } else { CONTENT_A };
                // A successful capture holds a read-only handle that denies
                // write sharing. Contending writes may therefore fail; any
                // successful write must publish one complete known payload.
                if std::fs::write(&writer_path, content).is_ok() {
                    successful_writes += 1;
                }
                thread::yield_now();
            }
            successful_writes
        });

        barrier.wait();
        let request = SnapshotCaptureRequest::new("mutable.txt", 128).unwrap();
        for _ in 0..600 {
            match adapter.capture(&request) {
                Ok(captured) => {
                    assert!(
                        captured.bytes() == CONTENT_A || captured.bytes() == CONTENT_B,
                        "capture returned partial or unexpected bytes during concurrent mutation"
                    );
                    assert_eq!(
                        crate::snapshot_payload::validate_snapshot_payload(
                            captured.snapshot(),
                            captured.bytes()
                        ),
                        Ok(())
                    );
                }
                Err(SnapshotPayloadError::AcquisitionFailed) => {}
                Err(other) => panic!("unexpected snapshot error during final-file mutation: {other:?}"),
            }
        }

        let successful_writes = writer.join().unwrap();
        assert!(successful_writes > 0, "mutation writer did not complete any writes");
        let final_capture = adapter.capture(&request).unwrap();
        assert!(
            final_capture.bytes() == CONTENT_A || final_capture.bytes() == CONTENT_B,
            "final capture must contain one complete published payload"
        );
    }

    #[test]
    fn intermediate_junction_replacement_race_never_captures_outside_bytes() {
        use crate::snapshot_payload::{SnapshotCaptureRequest, SnapshotPayloadError, WorkspaceSnapshotAdapter};
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

        let adapter = Arc::new(WindowsWorkspaceSnapshotAdapter::new(
            &test_volume_root_for(&workspace),
            &test_workspace_relative_path(&workspace),
            "windows-race-workspace",
        ).unwrap());
        let barrier = Arc::new(Barrier::new(2));
        let writer_workspace = workspace.clone();
        let writer_outside = outside.clone();
        let writer_barrier = barrier.clone();

        let writer = thread::spawn(move || {
            writer_barrier.wait();
            let mut replacements = 0usize;
            for index in 0..200 {
                let active = writer_workspace.join("nested");
                let held = writer_workspace.join(format!("nested-held-{index}"));
                // A retained child handle denies delete sharing. If the rename
                // wins before acquisition, the junction is installed and the
                // reader must reject it rather than follow it.
                if std::fs::rename(&active, &held).is_ok() {
                    create_junction(&active, &writer_outside);
                    thread::yield_now();
                    std::fs::remove_dir(&active).unwrap();
                    std::fs::rename(&held, &active).unwrap();
                    replacements += 1;
                } else {
                    thread::yield_now();
                }
            }
            replacements
        });

        barrier.wait();
        let request = SnapshotCaptureRequest::new("nested/target.txt", 128).unwrap();
        for _ in 0..600 {
            match adapter.capture(&request) {
                Ok(captured) => {
                    assert_eq!(
                        captured.bytes(),
                        b"inside-workspace",
                        "snapshot escaped through a replaced intermediate directory"
                    );
                    assert_eq!(
                        crate::snapshot_payload::validate_snapshot_payload(
                            captured.snapshot(),
                            captured.bytes()
                        ),
                        Ok(())
                    );
                }
                Err(SnapshotPayloadError::AcquisitionFailed) => {}
                Err(other) => panic!("unexpected snapshot error during junction race: {other:?}"),
            }
        }

        let replacements = writer.join().unwrap();
        assert!(replacements > 0, "race writer did not complete any directory replacements");
        let final_capture = adapter.capture(&request).unwrap();
        assert_eq!(final_capture.bytes(), b"inside-workspace");
    }

    #[test]
    fn snapshot_capture_fails_closed_when_existing_handle_denies_read_sharing() {
        use crate::snapshot_payload::{SnapshotCaptureRequest, SnapshotPayloadError, WorkspaceSnapshotAdapter};

        const GENERIC_WRITE_VALUE: u32 = 0x4000_0000;
        const FILE_ATTRIBUTE_NORMAL_VALUE: u32 = 0x0000_0080;

        let workspace = tempfile::tempdir().unwrap();
        let target = workspace.path().join("busy.txt");
        std::fs::write(&target, b"contended").unwrap();

        let adapter = WindowsWorkspaceSnapshotAdapter::new(
            &test_volume_root_for(workspace.path()),
            &test_workspace_relative_path(workspace.path()),
            "windows-share-conflict-workspace",
        ).unwrap();

        let wide = to_wide_nul(target.to_string_lossy().as_ref());
        // Open an incompatible pre-existing writer handle: it requests write
        // access while sharing no access with subsequent opens. The snapshot
        // adapter must not retry by pathname or weaken the requested sharing.
        // SAFETY: wide is NUL-terminated; null security/template handles are valid.
        let raw = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_WRITE_VALUE,
                0,
                null(),
                OPEN_EXISTING_VALUE,
                FILE_ATTRIBUTE_NORMAL_VALUE,
                null_mut(),
            )
        };
        assert_ne!(raw, invalid_handle_value(), "failed to create share-conflict fixture");
        // SAFETY: raw is a uniquely owned handle returned by CreateFileW.
        let _blocking_writer = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };

        assert!(matches!(
            adapter.capture(&SnapshotCaptureRequest::new("busy.txt", 128).unwrap()),
            Err(SnapshotPayloadError::AcquisitionFailed)
        ), "an incompatible pre-existing handle must cause fail-closed capture");
    }

}
