//! Snapshot-bound approval binding prototype.
//!
//! Binds the canonical action digest and canonical workspace-snapshot digest.
//! This is still not authenticated human approval or a production capability.

use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::ProposedAction;
use crate::canonical_action_digest::canonical_action_digest;
use crate::canonical_snapshot_digest::canonical_snapshot_binding_digest;
use crate::snapshot_payload::CapturedWorkspaceSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalChallenge {
    pub challenge_id: String,
    pub action_digest: String,
    pub snapshot_digest: String,
    pub expires_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    InvalidChallengeId,
    DigestMismatch,
    SnapshotDigestMismatch,
    SnapshotInvalid,
    SnapshotStale,
    ActionSnapshotMismatch,
    UnsupportedAction,
    Expired,
    InvalidDigestFormat,
}

/// SHA-256 of canonical Action Digest v1 bytes.
pub fn action_digest(action: &ProposedAction) -> Result<String, serde_json::Error> {
    Ok(canonical_action_digest(action))
}

/// Validate that a file-scoped action refers to the exact captured file.
/// Command actions require a separate workspace/command-context contract.
pub fn validate_action_snapshot_binding(
    action: &ProposedAction,
    snapshot: &CapturedWorkspaceSnapshot,
) -> Result<(), ApprovalError> {
    snapshot.snapshot().validate_contract().map_err(|_| ApprovalError::SnapshotInvalid)?;
    match action {
        ProposedAction::ReadFile { path, .. } => {
            if path == &snapshot.snapshot().relative_path { Ok(()) }
            else { Err(ApprovalError::ActionSnapshotMismatch) }
        }
        ProposedAction::ProposePatch { path, expected_target_sha256, .. } => {
            if path != &snapshot.snapshot().relative_path
                || expected_target_sha256.as_deref() != snapshot.snapshot().content_sha256.as_deref()
            {
                return Err(ApprovalError::ActionSnapshotMismatch);
            }
            Ok(())
        }
        ProposedAction::RunCommand { .. } => Err(ApprovalError::UnsupportedAction),
    }
}

fn decode_hex_digest(hex: &str) -> Result<[u8; 32], ApprovalError> {
    if hex.len() != 64 || !hex.is_ascii() { return Err(ApprovalError::InvalidDigestFormat); }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let pair = &hex[i*2..i*2+2];
        out[i] = u8::from_str_radix(pair, 16).map_err(|_| ApprovalError::InvalidDigestFormat)?;
    }
    Ok(out)
}

/// Create an action + snapshot binding. This is not proof of user approval.
pub fn create_challenge(
    challenge_id: String,
    action: &ProposedAction,
    snapshot: &CapturedWorkspaceSnapshot,
    expires_at_unix: u64,
) -> Result<ApprovalChallenge, ApprovalError> {
    if challenge_id.trim().is_empty() { return Err(ApprovalError::InvalidChallengeId); }
    validate_action_snapshot_binding(action, snapshot)?;
    let action_digest = action_digest(action).map_err(|_| ApprovalError::InvalidDigestFormat)?;
    let snapshot_digest = canonical_snapshot_binding_digest(snapshot.snapshot());
    Ok(ApprovalChallenge { challenge_id, action_digest, snapshot_digest, expires_at_unix })
}

/// Verify the exact action and fresh capture binding. Freshness age is checked
/// by ApprovalStore using its authority-owned monotonic clock.
pub fn verify_approval_binding(
    challenge: &ApprovalChallenge,
    action: &ProposedAction,
    snapshot: &CapturedWorkspaceSnapshot,
    now_unix: u64,
) -> Result<(), ApprovalError> {
    if challenge.challenge_id.trim().is_empty() { return Err(ApprovalError::InvalidChallengeId); }
    if now_unix >= challenge.expires_at_unix { return Err(ApprovalError::Expired); }
    validate_action_snapshot_binding(action, snapshot)?;

    let expected_action = decode_hex_digest(&challenge.action_digest)?;
    let actual_action = decode_hex_digest(&action_digest(action).map_err(|_| ApprovalError::InvalidDigestFormat)?)?;
    if !bool::from(expected_action.ct_eq(&actual_action)) { return Err(ApprovalError::DigestMismatch); }

    let expected_snapshot = decode_hex_digest(&challenge.snapshot_digest)?;
    let actual_snapshot = decode_hex_digest(&canonical_snapshot_binding_digest(snapshot.snapshot()))?;
    if !bool::from(expected_snapshot.ct_eq(&actual_snapshot)) { return Err(ApprovalError::SnapshotDigestMismatch); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot_payload::build_snapshot_contract;
    use std::time::Duration;

    fn sample() -> ProposedAction {
        ProposedAction::ReadFile { path: "src/lib.rs".into(), max_bytes: Some(4096) }
    }
    fn sample_snapshot() -> CapturedWorkspaceSnapshot {
        let bytes = b"captured";
        let digest = crate::snapshot_payload::build_snapshot_contract("workspace", "src/lib.rs", "opaque-id", bytes).unwrap();
        CapturedWorkspaceSnapshot::new_at(digest, bytes.to_vec(), Duration::from_secs(10)).unwrap()
    }

    #[test]
    fn action_digest_is_stable() {
        assert_eq!(action_digest(&sample()).unwrap(), action_digest(&sample()).unwrap());
    }

    #[test]
    fn matching_action_and_snapshot_pass() {
        let action = sample();
        let snapshot = sample_snapshot();
        let challenge = create_challenge("ch-1".into(), &action, &snapshot, 500).unwrap();
        assert_eq!(verify_approval_binding(&challenge, &action, &snapshot, 499), Ok(()));
    }

    #[test]
    fn modified_action_fails_binding() {
        let action = sample();
        let snapshot = sample_snapshot();
        let challenge = create_challenge("ch-1".into(), &action, &snapshot, 500).unwrap();
        let changed = ProposedAction::ReadFile { path: "src/other.rs".into(), max_bytes: Some(4096) };
        assert_eq!(verify_approval_binding(&challenge, &changed, &snapshot, 499), Err(ApprovalError::ActionSnapshotMismatch));
    }

    #[test]
    fn changed_snapshot_identity_fails_binding() {
        let action = sample();
        let snapshot = sample_snapshot();
        let challenge = create_challenge("ch-1".into(), &action, &snapshot, 500).unwrap();
        let bytes = b"captured";
        let changed = build_snapshot_contract("workspace", "src/lib.rs", "different-id", bytes).unwrap();
        let changed = CapturedWorkspaceSnapshot::new_at(changed, bytes.to_vec(), Duration::from_secs(11)).unwrap();
        assert_eq!(verify_approval_binding(&challenge, &action, &changed, 499), Err(ApprovalError::SnapshotDigestMismatch));
    }

    #[test]
    fn command_action_is_rejected_without_workspace_snapshot_contract() {
        let action = ProposedAction::RunCommand { executable: "cargo".into(), args: vec!["test".into()], working_directory: None, timeout_ms: None };
        assert_eq!(validate_action_snapshot_binding(&action, &sample_snapshot()), Err(ApprovalError::UnsupportedAction));
    }

    #[test]
    fn patch_requires_matching_target_digest() {
        let snapshot = sample_snapshot();
        let action = ProposedAction::ProposePatch { path: "src/lib.rs".into(), patch: "diff".into(), expected_target_sha256: Some("a".repeat(64)) };
        assert_eq!(validate_action_snapshot_binding(&action, &snapshot), Err(ApprovalError::ActionSnapshotMismatch));
    }

    #[test]
    fn expired_challenge_rejected() {
        let action = sample(); let snapshot = sample_snapshot();
        let challenge = create_challenge("ch-1".into(), &action, &snapshot, 500).unwrap();
        assert_eq!(verify_approval_binding(&challenge, &action, &snapshot, 500), Err(ApprovalError::Expired));
    }

    #[test]
    fn malformed_digest_rejected() {
        let action = sample(); let snapshot = sample_snapshot();
        let challenge = ApprovalChallenge { challenge_id: "ch".into(), action_digest: "nope".into(), snapshot_digest: "a".repeat(64), expires_at_unix: 500 };
        assert_eq!(verify_approval_binding(&challenge, &action, &snapshot, 499), Err(ApprovalError::InvalidDigestFormat));
    }
}
