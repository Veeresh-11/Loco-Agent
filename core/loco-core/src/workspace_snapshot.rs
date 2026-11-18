//! Versioned workspace snapshot contract (metadata only).
//!
//! This module defines what a secure OS adapter must return after acquiring a
//! file through platform-appropriate handle-based operations. It does NOT open,
//! inspect, or read files and does NOT itself provide filesystem security.

use serde::{Deserialize, Serialize};

pub const SNAPSHOT_CONTRACT_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotFileKind {
    RegularFile,
    Directory,
    SymlinkOrReparsePoint,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub contract_version: String,
    pub workspace_id: String,
    pub relative_path: String,
    pub file_kind: SnapshotFileKind,
    pub byte_length: u64,
    pub content_sha256: Option<String>,
    /// Opaque adapter-specific identity (e.g., volume/file ID or inode tuple).
    /// Must not be treated as portable or meaningful outside the adapter/OS.
    pub file_identity: Option<String>,
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotContractError {
    UnsupportedVersion,
    EmptyWorkspaceId,
    InvalidRelativePath,
    NonRegularFile,
    MissingContentDigest,
    InvalidContentDigest,
    InvalidIdentity,
}


impl WorkspaceSnapshot {
    /// Validates contract fields only. This is not proof that the adapter
    /// acquired the file securely or that the snapshot is still current.
    pub fn validate_contract(&self) -> Result<(), SnapshotContractError> {
        if self.contract_version != SNAPSHOT_CONTRACT_VERSION {
            return Err(SnapshotContractError::UnsupportedVersion);
        }
        if self.workspace_id.trim().is_empty() {
            return Err(SnapshotContractError::EmptyWorkspaceId);
        }
        if self.relative_path.is_empty()
            || self.relative_path.starts_with('/')
            || self.relative_path.contains('\\')
            || self.relative_path.contains(':')
            || self.relative_path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(SnapshotContractError::InvalidRelativePath);
        }
        if self.file_kind != SnapshotFileKind::RegularFile {
            return Err(SnapshotContractError::NonRegularFile);
        }
        let digest = self.content_sha256.as_deref().ok_or(SnapshotContractError::MissingContentDigest)?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_digit() || (b >= b'a' && b <= b'f')) {
            return Err(SnapshotContractError::InvalidContentDigest);
        }
        if self.file_identity.as_deref().is_none_or(|identity| identity.trim().is_empty()) {
            return Err(SnapshotContractError::InvalidIdentity);
        }
        Ok(())
    }

    /// Compares the contract-relevant snapshot binding. Adapter-specific
    /// identity is compared only when both snapshots provide it; absence must
    /// never be interpreted as strong identity assurance.
    pub fn same_binding(&self, other: &Self) -> bool {
        self.contract_version == other.contract_version
            && self.workspace_id == other.workspace_id
            && self.relative_path == other.relative_path
            && self.file_kind == other.file_kind
            && self.byte_length == other.byte_length
            && self.content_sha256 == other.content_sha256
            && matches!((&self.file_identity, &other.file_identity), (Some(a), Some(b)) if a == b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            contract_version: SNAPSHOT_CONTRACT_VERSION.into(),
            workspace_id: "workspace-test".into(),
            relative_path: "src/lib.rs".into(),
            file_kind: SnapshotFileKind::RegularFile,
            byte_length: 12,
            content_sha256: Some("a".repeat(64)),
            file_identity: Some("adapter-opaque-id".into()),
        }
    }

    #[test]
    fn validates_regular_snapshot_contract() {
        assert_eq!(snapshot().validate_contract(), Ok(()));
    }

    #[test]
    fn rejects_unknown_version() {
        let mut s = snapshot();
        s.contract_version = "2".into();
        assert_eq!(s.validate_contract(), Err(SnapshotContractError::UnsupportedVersion));
    }

    #[test]
    fn rejects_traversal_and_ambiguous_path_forms() {
        for path in ["../outside", "src/../secret", "/tmp/file", "C:/temp/file", "src\\file", "src//file", "./file"] {
            let mut s = snapshot();
            s.relative_path = path.into();
            assert_eq!(s.validate_contract(), Err(SnapshotContractError::InvalidRelativePath), "path: {path}");
        }
    }

    #[test]
    fn rejects_non_regular_file_kinds() {
        for kind in [SnapshotFileKind::Directory, SnapshotFileKind::SymlinkOrReparsePoint, SnapshotFileKind::Other, SnapshotFileKind::Unknown] {
            let mut s = snapshot();
            s.file_kind = kind;
            assert_eq!(s.validate_contract(), Err(SnapshotContractError::NonRegularFile));
        }
    }

    #[test]
    fn requires_valid_content_digest() {
        let mut s = snapshot();
        s.content_sha256 = None;
        assert_eq!(s.validate_contract(), Err(SnapshotContractError::MissingContentDigest));
        s.content_sha256 = Some("z".repeat(64));
        assert_eq!(s.validate_contract(), Err(SnapshotContractError::InvalidContentDigest));
        s.content_sha256 = Some("A".repeat(64));
        assert_eq!(s.validate_contract(), Err(SnapshotContractError::InvalidContentDigest));
    }

    #[test]
    fn binding_changes_when_workspace_target_or_content_changes() {
        let original = snapshot();
        let mut changed = original.clone();
        changed.content_sha256 = Some("b".repeat(64));
        assert!(!original.same_binding(&changed));
        changed = original.clone();
        changed.relative_path = "src/other.rs".into();
        assert!(!original.same_binding(&changed));
        changed = original.clone();
        changed.workspace_id = "other-workspace".into();
        assert!(!original.same_binding(&changed));
    }

    #[test]
    fn binding_requires_identity_presence_to_match() {
        let original = snapshot();
        let mut changed = original.clone();
        changed.file_identity = None;
        assert!(!original.same_binding(&changed));
        changed = original.clone();
        let missing_identity = WorkspaceSnapshot { file_identity: None, ..original.clone() };
        assert!(!missing_identity.same_binding(&missing_identity);
        changed = original.clone();
        changed.file_identity = Some("different-id".into());
        assert!(!original.same_binding(&changed));
    }

    #[test]
    fn contract_serializes_camel_case_fields() {
        let value = serde_json::to_value(snapshot()).unwrap();
        assert_eq!(value["contractVersion"], "1");
        assert_eq!(value["workspaceId"], "workspace-test");
        assert_eq!(value["relativePath"], "src/lib.rs");
        assert_eq!(value["contentSha256"], "a".repeat(64));
        assert!(value.get("content_sha256").is_none());
    }
}
