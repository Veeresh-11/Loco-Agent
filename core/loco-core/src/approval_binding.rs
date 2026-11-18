//! Action approval binding prototype.
//!
//! Binds approval evidence to a serialized action digest and checks challenge
//! freshness. This prototype is not cryptographically signed and does not persist
//! challenge consumption. Production must use an authenticated approval channel,
//! protected replay store, and trusted clock.

use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::ProposedAction;
use crate::canonical_action_digest::canonical_action_digest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalChallenge {
    pub challenge_id: String,
    pub action_digest: String,
    pub expires_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    InvalidChallengeId,
    DigestMismatch,
    Expired,
    InvalidDigestFormat,
}

/// SHA-256 of canonical Action Digest v1 bytes. The encoding is language-neutral
/// and defined in contracts/action-digest-v1.md.
pub fn action_digest(action: &ProposedAction) -> Result<String, serde_json::Error> {
    Ok(canonical_action_digest(action))
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

/// Create binding data for one proposed action. This is not proof of user
/// approval or cryptographic authentication.
pub fn create_challenge(
    challenge_id: String,
    action: &ProposedAction,
    expires_at_unix: u64,
) -> Result<ApprovalChallenge, serde_json::Error> {
    Ok(ApprovalChallenge {
        challenge_id,
        action_digest: action_digest(action)?,
        expires_at_unix,
    })
}

/// Validate binding to the current action and expiry. The supplied timestamp
/// must come from a trusted caller clock. One-time consumption is not enforced.
pub fn verify_approval_binding(
    challenge: &ApprovalChallenge,
    action: &ProposedAction,
    now_unix: u64,
) -> Result<(), ApprovalError> {
    if challenge.challenge_id.trim().is_empty() {
        return Err(ApprovalError::InvalidChallengeId);
    }
    if now_unix >= challenge.expires_at_unix {
        return Err(ApprovalError::Expired);
    }
    let expected = decode_hex_digest(&challenge.action_digest)?;
    let actual_hex = action_digest(action).map_err(|_| ApprovalError::InvalidDigestFormat)?;
    let actual = decode_hex_digest(&actual_hex)?;
    if !bool::from(expected.ct_eq(&actual)) {
        return Err(ApprovalError::DigestMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ProposedAction {
        ProposedAction::ProposePatch {
            path: "src/lib.rs".into(),
            patch: "small diff".into(),
            expected_target_sha256: Some("a".repeat(64)),
        }
    }

    #[test]
    fn action_digest_is_stable() {
        assert_eq!(action_digest(&sample()).unwrap(), action_digest(&sample()).unwrap());
    }

    #[test]
    fn matching_action_and_fresh_challenge_pass() {
        let action = sample();
        let challenge = create_challenge("ch-1".into(), &action, 500).unwrap();
        assert_eq!(verify_approval_binding(&challenge, &action, 499), Ok(()));
    }

    #[test]
    fn modified_action_fails_binding() {
        let action = sample();
        let challenge = create_challenge("ch-1".into(), &action, 500).unwrap();
        let changed = ProposedAction::ProposePatch {
            path: "src/lib.rs".into(),
            patch: "different diff".into(),
            expected_target_sha256: Some("a".repeat(64)),
        };
        assert_eq!(verify_approval_binding(&challenge, &changed, 499), Err(ApprovalError::DigestMismatch));
    }

    #[test]
    fn expired_challenge_rejected() {
        let action = sample();
        let challenge = create_challenge("ch-1".into(), &action, 500).unwrap();
        assert_eq!(verify_approval_binding(&challenge, &action, 500), Err(ApprovalError::Expired));
    }

    #[test]
    fn blank_challenge_id_rejected() {
        let action = sample();
        let challenge = create_challenge(" ".into(), &action, 500).unwrap();
        assert_eq!(verify_approval_binding(&challenge, &action, 499), Err(ApprovalError::InvalidChallengeId));
    }

    #[test]
    fn malformed_digest_rejected() {
        let action = sample();
        let challenge = ApprovalChallenge { challenge_id: "ch".into(), action_digest: "nope".into(), expires_at_unix: 500 };
        assert_eq!(verify_approval_binding(&challenge, &action, 499), Err(ApprovalError::InvalidDigestFormat));
    }
    #[test]
    fn non_ascii_digest_is_rejected_without_panicking() {
        let action = sample();
        let challenge = ApprovalChallenge { challenge_id: "ch".into(), action_digest: "🦀".repeat(16), expires_at_unix: 500 };
        assert_eq!(verify_approval_binding(&challenge, &action, 499), Err(ApprovalError::InvalidDigestFormat));
    }

}
