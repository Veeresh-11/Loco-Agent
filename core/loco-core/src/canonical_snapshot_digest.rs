//! Canonical snapshot binding digest v1.
//! Domain-separated from Action Digest v1 and bound to the validated snapshot contract.

use sha2::{Digest, Sha256};
use crate::workspace_snapshot::{WorkspaceSnapshot, SnapshotFileKind};

const PREFIX: &[u8] = b"LOCO-SNAPSHOT-BINDING-V1\0";

fn put_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn put_optional_string(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        None => out.push(0),
        Some(value) => { out.push(1); put_string(out, value); }
    }
}

/// Returns the canonical v1 bytes. Callers must validate the snapshot first.
pub fn canonical_snapshot_binding_bytes(snapshot: &WorkspaceSnapshot) -> Vec<u8> {
    let mut out = PREFIX.to_vec();
    put_string(&mut out, &snapshot.contract_version);
    put_string(&mut out, &snapshot.workspace_id);
    put_string(&mut out, &snapshot.relative_path);
    out.push(match snapshot.file_kind {
        SnapshotFileKind::RegularFile => 1,
        SnapshotFileKind::Directory => 2,
        SnapshotFileKind::SymlinkOrReparsePoint => 3,
        SnapshotFileKind::Other => 4,
        SnapshotFileKind::Unknown => 255,
    });
    out.extend_from_slice(&snapshot.byte_length.to_be_bytes());
    put_optional_string(&mut out, snapshot.content_sha256.as_deref());
    put_optional_string(&mut out, snapshot.file_identity.as_deref());
    out
}

pub fn canonical_snapshot_binding_digest(snapshot: &WorkspaceSnapshot) -> String {
    let digest = Sha256::digest(canonical_snapshot_binding_bytes(snapshot));
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_snapshot::SNAPSHOT_CONTRACT_VERSION;

    fn sample() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            contract_version: SNAPSHOT_CONTRACT_VERSION.into(),
            workspace_id: "workspace".into(),
            relative_path: "src/lib.rs".into(),
            file_kind: SnapshotFileKind::RegularFile,
            byte_length: 6,
            content_sha256: Some("a".repeat(64)),
            file_identity: Some("opaque-id".into()),
        }
    }

    #[test]
    fn snapshot_binding_digest_is_stable_and_domain_separated() {
        let snapshot = sample();
        assert_eq!(canonical_snapshot_binding_digest(&snapshot), canonical_snapshot_binding_digest(&snapshot));
        assert!(canonical_snapshot_binding_bytes(&snapshot).starts_with(PREFIX));
    }

    #[test]
    fn every_security_relevant_snapshot_field_changes_the_binding() {
        let original = sample();
        let original_digest = canonical_snapshot_binding_digest(&original);
        let mut changed = original.clone();
        changed.workspace_id.push_str("-other");
        assert_ne!(original_digest, canonical_snapshot_binding_digest(&changed));
        changed = original.clone(); changed.relative_path = "src/other.rs".into();
        assert_ne!(original_digest, canonical_snapshot_binding_digest(&changed));
        changed = original.clone(); changed.byte_length += 1;
        assert_ne!(original_digest, canonical_snapshot_binding_digest(&changed));
        changed = original.clone(); changed.content_sha256 = Some("b".repeat(64));
        assert_ne!(original_digest, canonical_snapshot_binding_digest(&changed));
        changed = original.clone(); changed.file_identity = Some("other-id".into());
        assert_ne!(original_digest, canonical_snapshot_binding_digest(&changed));
        changed = original.clone(); changed.contract_version = "2".into();
        assert_ne!(original_digest, canonical_snapshot_binding_digest(&changed));
    }
}
