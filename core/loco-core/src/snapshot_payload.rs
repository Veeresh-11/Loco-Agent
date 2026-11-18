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
