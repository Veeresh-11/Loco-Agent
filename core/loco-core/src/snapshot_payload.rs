//! Validation for the bytes paired with a workspace snapshot contract.
//!
//! This validates payload consistency only. It does not acquire bytes securely,
//! bind an OS handle, or prevent concurrent filesystem mutation.

use sha2::{Digest, Sha256};
use crate::workspace_snapshot::{WorkspaceSnapshot, SnapshotContractError};

pub const MAX_SNAPSHOT_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotPayloadError {
    InvalidContract(SnapshotContractError),
    LimitExceeded,
    LengthMismatch,
    DigestMismatch,
    AcquisitionFailed,
    InvalidFileType,
    HardLinkNotAllowed,
}


/// Constructs snapshot metadata from a byte buffer already acquired by the caller.
///
/// This helper does not prove that the bytes came from a safely opened file.
/// The caller must supply a trusted workspace ID and adapter-derived identity.
pub fn build_snapshot_contract(
    workspace_id: &str,
    relative_path: &str,
    file_identity: &str,
    bytes: &[u8],
) -> Result<WorkspaceSnapshot, SnapshotPayloadError> {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(SnapshotPayloadError::LimitExceeded);
    }
    let digest = Sha256::digest(bytes);
    let hex = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let snapshot = WorkspaceSnapshot {
        contract_version: crate::workspace_snapshot::SNAPSHOT_CONTRACT_VERSION.into(),
        workspace_id: workspace_id.into(),
        relative_path: relative_path.into(),
        file_kind: crate::workspace_snapshot::SnapshotFileKind::RegularFile,
        byte_length: bytes.len() as u64,
        content_sha256: Some(hex),
        file_identity: Some(file_identity.into()),
    };
    snapshot.validate_contract().map_err(SnapshotPayloadError::InvalidContract)?;
    Ok(snapshot)
}



/// Validated input for a snapshot capture request.
/// Construction applies lexical checks and clamps limits before an adapter
/// receives the request. Workspace capability/root binding remains adapter-owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotCaptureRequest {
    relative_path: String,
    max_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotCaptureRequestError {
    InvalidPath,
    InvalidLimit,
}

fn is_portable_component(component: &str) -> bool {
    if component.is_empty() || component.ends_with('.') || component.ends_with(' ') {
        return false;
    }
    if component.chars().any(|c| c.is_control()) {
        return false;
    }
    // Windows device names are reserved even when followed by an extension.
    let stem = component.split('.').next().unwrap_or(component).to_ascii_uppercase();
    matches!(stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" |
        "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" |
        "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    ) == false
}

impl SnapshotCaptureRequest {
    pub fn new(relative_path: &str, max_bytes: usize) -> Result<Self, SnapshotCaptureRequestError> {
        if relative_path.is_empty()
            || relative_path.starts_with('/')
            || relative_path.contains('\\')
            || relative_path.contains(':')
            || relative_path.contains('\0')
            || relative_path.split('/').any(|p| !is_portable_component(p))
        {
            return Err(SnapshotCaptureRequestError::InvalidPath);
        }
        if max_bytes == 0 || max_bytes > MAX_SNAPSHOT_BYTES {
            return Err(SnapshotCaptureRequestError::InvalidLimit);
        }
        Ok(Self { relative_path: relative_path.to_owned(), max_bytes })
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }
}

/// A validated snapshot capture: metadata and bytes are checked to agree.
/// This object is not an authorization token and cannot be used to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedWorkspaceSnapshot {
    snapshot: WorkspaceSnapshot,
    bytes: Vec<u8>,
}

impl CapturedWorkspaceSnapshot {
    pub fn new(snapshot: WorkspaceSnapshot, bytes: Vec<u8>) -> Result<Self, SnapshotPayloadError> {
        validate_snapshot_payload(&snapshot, &bytes)?;
        Ok(Self { snapshot, bytes })
    }

    pub fn snapshot(&self) -> &WorkspaceSnapshot {
        &self.snapshot
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Read-only adapter boundary. Implementations must acquire bytes through
/// platform-safe, workspace-root-relative operations and derive identity from
/// the opened handle. This trait alone does not make an implementation secure.
pub trait WorkspaceSnapshotAdapter {
    fn capture(
        &self,
        request: &SnapshotCaptureRequest,
    ) -> Result<CapturedWorkspaceSnapshot, SnapshotPayloadError>;
}

pub fn validate_snapshot_payload(
    snapshot: &WorkspaceSnapshot,
    bytes: &[u8],
) -> Result<(), SnapshotPayloadError> {
    snapshot.validate_contract().map_err(SnapshotPayloadError::InvalidContract)?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(SnapshotPayloadError::LimitExceeded);
    }
    if snapshot.byte_length != bytes.len() as u64 {
        return Err(SnapshotPayloadError::LengthMismatch);
    }
    let digest = Sha256::digest(bytes);
    let actual = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
    if snapshot.content_sha256.as_deref() != Some(actual.as_str()) {
        return Err(SnapshotPayloadError::DigestMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_snapshot::{SnapshotFileKind, SNAPSHOT_CONTRACT_VERSION};

    fn snapshot_for(bytes: &[u8]) -> WorkspaceSnapshot {
        let digest = Sha256::digest(bytes);
        let hex = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
        WorkspaceSnapshot {
            contract_version: SNAPSHOT_CONTRACT_VERSION.into(),
            workspace_id: "test-workspace".into(),
            relative_path: "src/lib.rs".into(),
            file_kind: SnapshotFileKind::RegularFile,
            byte_length: bytes.len() as u64,
            content_sha256: Some(hex),
            file_identity: Some("test-identity".into()),
        }
    }




    #[test]
    fn capture_request_rejects_windows_reserved_names_case_insensitively() {
        for component in [
            "CON", "con.txt", "PRN.log", "AUX", "nul.bin",
            "COM1", "com9.txt", "LPT1", "lpt9.csv",
        ] {
            let path = format!("src/{component}");
            assert_eq!(
                SnapshotCaptureRequest::new(&path, 100),
                Err(SnapshotCaptureRequestError::InvalidPath),
                "reserved component should be rejected: {path}"
            );
        }
    }

    #[test]
    fn capture_request_rejects_trailing_dot_space_and_control_chars() {
        for path in [
            "src/name.", "src/name ", "src/tab\\tname", "src/new\\nline",
            "src/\u{7f}name",
        ] {
            assert_eq!(
                SnapshotCaptureRequest::new(path, 100),
                Err(SnapshotCaptureRequestError::InvalidPath),
                "non-portable component should be rejected: {path:?}"
            );
        }
    }

    #[test]
    fn capture_request_rejects_windows_ads_and_namespace_forms() {
        for path in [
            "src/file:stream", "\\\\server\\share\\file",
            "\\\\?\\C:\\workspace\\file", "C:\\workspace\\file",
        ] {
            assert_eq!(
                SnapshotCaptureRequest::new(path, 100),
                Err(SnapshotCaptureRequestError::InvalidPath),
                "Windows namespace or ADS path should be rejected: {path:?}"
            );
        }
    }

    #[test]
    fn capture_request_accepts_valid_relative_path_and_bound() {
        let req = SnapshotCaptureRequest::new("src/main.rs", 4096).unwrap();
        assert_eq!(req.relative_path(), "src/main.rs");
        assert_eq!(req.max_bytes(), 4096);
    }

    #[test]
    fn capture_request_rejects_traversal_and_ambiguous_paths() {
        for path in ["../secret", "src/../secret", "/absolute", "C:/secret", "src\\file", "src//file", "./file", ""] {
            assert_eq!(SnapshotCaptureRequest::new(path, 100), Err(SnapshotCaptureRequestError::InvalidPath), "path: {path}");
        }
    }

    #[test]
    fn capture_request_enforces_hard_byte_cap() {
        assert_eq!(SnapshotCaptureRequest::new("file.txt", 0), Err(SnapshotCaptureRequestError::InvalidLimit));
        assert_eq!(SnapshotCaptureRequest::new("file.txt", MAX_SNAPSHOT_BYTES + 1), Err(SnapshotCaptureRequestError::InvalidLimit));
        assert!(SnapshotCaptureRequest::new("file.txt", MAX_SNAPSHOT_BYTES).is_ok());
    }

    #[test]
    fn captured_snapshot_exposes_only_validated_metadata_and_bytes() {
        let bytes = b"immutable captured bytes".to_vec();
        let contract = snapshot_for(&bytes);
        let captured = CapturedWorkspaceSnapshot::new(contract, bytes.clone()).unwrap();
        assert_eq!(captured.bytes(), bytes.as_slice());
        assert_eq!(captured.snapshot().byte_length, bytes.len() as u64);
    }

    #[test]
    fn captured_snapshot_rejects_inconsistent_payload() {
        let bytes = b"correct bytes".to_vec();
        let mut contract = snapshot_for(&bytes);
        contract.content_sha256 = Some("f".repeat(64));
        assert!(matches!(
            CapturedWorkspaceSnapshot::new(contract, bytes),
            Err(SnapshotPayloadError::DigestMismatch)
        ));
    }

    #[test]
    fn builder_creates_consistent_contract_from_captured_bytes() {
        let bytes = b"captured";
        let snapshot = build_snapshot_contract("workspace", "src/file.rs", "adapter-id", bytes).unwrap();
        assert_eq!(snapshot.byte_length, bytes.len() as u64);
        assert_eq!(validate_snapshot_payload(&snapshot, bytes), Ok(()));
    }

    #[test]
    fn builder_rejects_empty_identity_and_invalid_paths() {
        assert!(matches!(
            build_snapshot_contract("workspace", "src/file.rs", "", b"x"),
            Err(SnapshotPayloadError::InvalidContract(SnapshotContractError::InvalidIdentity))
        ));
        assert!(matches!(
            build_snapshot_contract("workspace", "../escape", "id", b"x"),
            Err(SnapshotPayloadError::InvalidContract(SnapshotContractError::InvalidRelativePath))
        ));
    }

    #[test]
    fn accepts_payload_matching_contract() {
        let bytes = b"sample file content";
        assert_eq!(validate_snapshot_payload(&snapshot_for(bytes), bytes), Ok(()));
    }

    #[test]
    fn rejects_length_mismatch() {
        let bytes = b"sample file content";
        let mut snapshot = snapshot_for(bytes);
        snapshot.byte_length += 1;
        assert_eq!(validate_snapshot_payload(&snapshot, bytes), Err(SnapshotPayloadError::LengthMismatch));
    }

    #[test]
    fn rejects_digest_mismatch() {
        let bytes = b"sample file content";
        let mut snapshot = snapshot_for(bytes);
        snapshot.content_sha256 = Some("0".repeat(64));
        assert_eq!(validate_snapshot_payload(&snapshot, bytes), Err(SnapshotPayloadError::DigestMismatch));
    }

    #[test]
    fn rejects_oversized_payload_before_digest() {
        let bytes = vec![0u8; MAX_SNAPSHOT_BYTES + 1];
        let snapshot = snapshot_for(&bytes);
        assert_eq!(validate_snapshot_payload(&snapshot, &bytes), Err(SnapshotPayloadError::LimitExceeded));
    }

    #[test]
    fn rejects_missing_identity_via_contract() {
        let bytes = b"sample file content";
        let mut snapshot = snapshot_for(bytes);
        snapshot.file_identity = None;
        assert!(matches!(
            validate_snapshot_payload(&snapshot, bytes),
            Err(SnapshotPayloadError::InvalidContract(SnapshotContractError::InvalidIdentity))
        ));
    }
}
