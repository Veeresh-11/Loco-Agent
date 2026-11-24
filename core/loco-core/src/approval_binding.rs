//! Approval binding prototype for exact actions and captured workspace targets.
//!
//! The challenge binds a canonical action digest to a validated snapshot identity
//! and content digest. It is not an authenticator assertion and does not prove
//! that a human approved the action.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::ProposedAction;
use crate::canonical_action_digest::canonical_action_digest;
use crate::workspace_snapshot::{SnapshotFileKind, WorkspaceSnapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalChallenge {
    pub challenge_id: String,
    pub action_digest: String,
    pub snapshot_binding_digest: String,
    pub expires_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    InvalidChallengeId,
    DigestMismatch,
    SnapshotMismatch,
    InvalidSnapshot,
    TargetMismatch,
    UnsupportedAction,
    Expired,
    InvalidDigestFormat,
}

/// SHA-256 of canonical Action Digest v1 bytes. The encoding is language-neutral
/// and defined in contracts/action-digest-v1.md.
pub fn action_digest(action: &ProposedAction) -> Result<String, serde_json::Error> {
    Ok(canonical_action_digest(action))
}

fn encode_string(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn encode_optional_string(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            out.push(0x01);
            encode_string(out, value);
        }
        None => out.push(0x00),
    }
}

/// SHA-256 of the versioned, canonical snapshot binding. This includes the
/// workspace, relative target, file kind, byte length, content digest, and
/// adapter-derived file identity. It intentionally excludes raw file bytes.
pub fn snapshot_binding_digest(
    snapshot: &WorkspaceSnapshot,
) -> Result<String, ApprovalError> {
    snapshot
        .validate_contract()
        .map_err(|_| ApprovalError::InvalidSnapshot)?;

    let mut encoded = b"LOCO-SNAPSHOT-BINDING-V1\0".to_vec();
    encode_string(&mut encoded, &snapshot.contract_version);
    encode_string(&mut encoded, &snapshot.workspace_id);
    encode_string(&mut encoded, &snapshot.relative_path);
    encoded.push(match &snapshot.file_kind {
        SnapshotFileKind::RegularFile => 0x01,
        _ => return Err(ApprovalError::InvalidSnapshot),
    });
    encoded.extend_from_slice(&snapshot.byte_length.to_be_bytes());
    encode_optional_string(&mut encoded, snapshot.content_sha256.as_deref());
    encode_optional_string(&mut encoded, snapshot.file_identity.as_deref());

    let digest = Sha256::digest(&encoded);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Restrict this snapshot-bound approval path to file actions whose target can
/// be represented by WorkspaceSnapshot. Commands need a separate workspace and
/// executable binding and are deliberately rejected here.
pub fn validate_action_snapshot(
    action: &ProposedAction,
    snapshot: &WorkspaceSnapshot,
) -> Result<(), ApprovalError> {
    snapshot
        .validate_contract()
        .map_err(|_| ApprovalError::InvalidSnapshot)?;

    match action {
        ProposedAction::ReadFile { path, .. } => {
            if path == &snapshot.relative_path {
                Ok(())
            } else {
                Err(ApprovalError::TargetMismatch)
            }
        }
        ProposedAction::ProposePatch {
            path,
            expected_target_sha256: Some(expected),
            ..
        } => {
            if path != &snapshot.relative_path {
                return Err(ApprovalError::TargetMismatch);
            }
            let snapshot_digest = snapshot
                .content_sha256
                .as_deref()
                .ok_or(ApprovalError::InvalidSnapshot)?;
            let expected_bytes = decode_hex_digest(expected)?;
            let snapshot_bytes = decode_hex_digest(snapshot_digest)?;
            if expected != snapshot_digest || !bool::from(expected_bytes.ct_eq(&snapshot_bytes)) {
                return Err(ApprovalError::TargetMismatch);
            }
            Ok(())
        }
        ProposedAction::ProposePatch { .. } => Err(ApprovalError::TargetMismatch),
        ProposedAction::RunCommand { .. } => Err(ApprovalError::UnsupportedAction),
    }
}

fn decode_hex_digest(hex: &str) -> Result<[u8; 32], ApprovalError> {
    if hex.len() != 64 || !hex.is_ascii() {
        return Err(ApprovalError::InvalidDigestFormat);
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let pair = &hex[i * 2..i * 2 + 2];
        out[i] = u8::from_str_radix(pair, 16)
            .map_err(|_| ApprovalError::InvalidDigestFormat)?;
    }
    Ok(out)
}

/// Create binding data for one file-targeted action and validated snapshot.
/// This is not proof of user approval or cryptographic authentication.
pub fn create_challenge(
    challenge_id: String,
    action: &ProposedAction,
    snapshot: &WorkspaceSnapshot,
    expires_at_unix: u64,
) -> Result<ApprovalChallenge, ApprovalError> {
    validate_action_snapshot(action, snapshot)?;
    let action_digest = action_digest(action)
        .map_err(|_| ApprovalError::InvalidDigestFormat)?;
    Ok(ApprovalChallenge {
        challenge_id,
        action_digest,
        snapshot_binding_digest: snapshot_binding_digest(snapshot)?,
        expires_at_unix,
    })
}

/// Validate exact action, target snapshot identity/content, and expiry.
/// The supplied timestamp must come from the Rust-owned authority clock.
pub fn verify_approval_binding(
    challenge: &ApprovalChallenge,
    action: &ProposedAction,
    snapshot: &WorkspaceSnapshot,
    now_unix: u64,
) -> Result<(), ApprovalError> {
    if challenge.challenge_id.trim().is_empty() {
        return Err(ApprovalError::InvalidChallengeId);
    }
    if now_unix >= challenge.expires_at_unix {
        return Err(ApprovalError::Expired);
    }
    validate_action_snapshot(action, snapshot)?;

    let expected_action = decode_hex_digest(&challenge.action_digest)?;
    let actual_action = decode_hex_digest(
        &action_digest(action).map_err(|_| ApprovalError::InvalidDigestFormat)?,
    )?;
    if !bool::from(expected_action.ct_eq(&actual_action)) {
        return Err(ApprovalError::DigestMismatch);
    }

    let expected_snapshot = decode_hex_digest(&challenge.snapshot_binding_digest)?;
    let actual_snapshot = decode_hex_digest(&snapshot_binding_digest(snapshot)?)?;
    if !bool::from(expected_snapshot.ct_eq(&actual_snapshot)) {
        return Err(ApprovalError::SnapshotMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_snapshot::{SnapshotFileKind, SNAPSHOT_CONTRACT_VERSION};

    fn sample_bytes() -> Vec<u8> {
        b"original target bytes".to_vec()
    }

    fn sample_snapshot() -> WorkspaceSnapshot {
        let bytes = sample_bytes();
        let digest = Sha256::digest(&bytes);
        WorkspaceSnapshot {
            contract_version: SNAPSHOT_CONTRACT_VERSION.into(),
            workspace_id: "workspace-test".into(),
            relative_path: "src/lib.rs".into(),
            file_kind: SnapshotFileKind::RegularFile,
            byte_length: bytes.len() as u64,
            content_sha256: Some(digest.iter().map(|byte| format!("{byte:02x}")).collect()),
            file_identity: Some("adapter-file-identity".into()),
        }
    }

    fn sample_action(snapshot: &WorkspaceSnapshot) -> ProposedAction {
        ProposedAction::ProposePatch {
            path: snapshot.relative_path.clone(),
            patch: "small diff".into(),
            expected_target_sha256: snapshot.content_sha256.clone(),
        }
    }

    #[test]
    fn action_and_snapshot_digests_are_stable() {
        let snapshot = sample_snapshot();
        let action = sample_action(&snapshot);
        assert_eq!(action_digest(&action).unwrap(), action_digest(&action).unwrap());
        assert_eq!(
            snapshot_binding_digest(&snapshot).unwrap(),
            snapshot_binding_digest(&snapshot).unwrap()
        );
    }

    #[test]
    fn matching_action_snapshot_and_fresh_challenge_pass() {
        let snapshot = sample_snapshot();
        let action = sample_action(&snapshot);
        let challenge = create_challenge("ch-1".into(), &action, &snapshot, 500).unwrap();
        assert_eq!(verify_approval_binding(&challenge, &action, &snapshot, 499), Ok(()));
    }

    #[test]
    fn modified_action_fails_binding() {
        let snapshot = sample_snapshot();
        let action = sample_action(&snapshot);
        let challenge = create_challenge("ch-1".into(), &action, &snapshot, 500).unwrap();
        let changed = ProposedAction::ProposePatch {
            path: snapshot.relative_path.clone(),
            patch: "different diff".into(),
            expected_target_sha256: snapshot.content_sha256.clone(),
        };
        assert_eq!(
            verify_approval_binding(&challenge, &changed, &snapshot, 499),
            Err(ApprovalError::DigestMismatch)
        );
    }

    #[test]
    fn changed_snapshot_identity_or_content_fails_binding() {
        let snapshot = sample_snapshot();
        let action = sample_action(&snapshot);
        let challenge = create_challenge("ch-1".into(), &action, &snapshot, 500).unwrap();

        let mut changed = snapshot.clone();
        changed.file_identity = Some("replacement-file-identity".into());
        assert_eq!(
            verify_approval_binding(&challenge, &action, &changed, 499),
            Err(ApprovalError::SnapshotMismatch)
        );

        let mut changed = snapshot.clone();
        changed.content_sha256 = Some("b".repeat(64));
        changed.byte_length = 64;
        let changed_action = ProposedAction::ProposePatch {
            path: changed.relative_path.clone(),
            patch: "small diff".into(),
            expected_target_sha256: changed.content_sha256.clone(),
        };
        assert_eq!(
            verify_approval_binding(&challenge, &changed_action, &changed, 499),
            Err(ApprovalError::SnapshotMismatch)
        );
    }

    #[test]
    fn workspace_or_relative_target_change_fails_binding() {
        let snapshot = sample_snapshot();
        let action = sample_action(&snapshot);
        let challenge = create_challenge("ch-1".into(), &action, &snapshot, 500).unwrap();

        let mut changed = snapshot.clone();
        changed.workspace_id = "other-workspace".into();
        assert_eq!(
            verify_approval_binding(&challenge, &action, &changed, 499),
            Err(ApprovalError::TargetMismatch)
        );

        let mut changed = snapshot.clone();
        changed.relative_path = "src/other.rs".into();
        assert_eq!(
            verify_approval_binding(&challenge, &action, &changed, 499),
            Err(ApprovalError::TargetMismatch)
        );
    }

    #[test]
    fn patch_requires_expected_target_digest() {
        let snapshot = sample_snapshot();
        let action = ProposedAction::ProposePatch {
            path: snapshot.relative_path.clone(),
            patch: "small diff".into(),
            expected_target_sha256: None,
        };
        assert_eq!(
            create_challenge("ch-1".into(), &action, &snapshot, 500),
            Err(ApprovalError::TargetMismatch)
        );
    }

    #[test]
    fn command_action_is_not_supported_by_file_snapshot_binding() {
        let snapshot = sample_snapshot();
        let action = ProposedAction::RunCommand {
            executable: "echo".into(),
            args: vec!["hello".into()],
            working_directory: Some(".".into()),
            timeout_ms: Some(1000),
        };
        assert_eq!(
            create_challenge("ch-1".into(), &action, &snapshot, 500),
            Err(ApprovalError::UnsupportedAction)
        );
    }

    #[test]
    fn expired_challenge_rejected() {
        let snapshot = sample_snapshot();
        let action = sample_action(&snapshot);
        let challenge = create_challenge("ch-1".into(), &action, &snapshot, 500).unwrap();
        assert_eq!(
            verify_approval_binding(&challenge, &action, &snapshot, 500),
            Err(ApprovalError::Expired)
        );
    }

    #[test]
    fn malformed_digest_rejected() {
        let snapshot = sample_snapshot();
        let action = sample_action(&snapshot);
        let mut challenge = create_challenge("ch-1".into(), &action, &snapshot, 500).unwrap();
        challenge.snapshot_binding_digest = "nope".into();
        assert_eq!(
            verify_approval_binding(&challenge, &action, &snapshot, 499),
            Err(ApprovalError::InvalidDigestFormat)
        );
    }
}
