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
        relative_path: &str,
        max_bytes: usize,
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
