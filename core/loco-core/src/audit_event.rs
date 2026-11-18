//! Typed audit-event contract DTO and conservative field bounds.
//!
//! This module structures audit metadata only. It is not a storage backend,
//! redaction system, or trusted timestamp provider.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuditScalar {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActionType {
    ReadFile,
    ProposePatch,
    RunCommand,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    ProposalReceived,
    PolicyDecision,
    ApprovalRequested,
    ApprovalGranted,
    ApprovalRejected,
    ExecutionStarted,
    ExecutionFinished,
    ExecutionFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Allowed,
    Denied,
    ApprovalRequired,
    Success,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub contract_version: String,
    pub event_id: String,
    pub occurred_at: String,
    pub task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_type: Option<AuditActionType>,
    pub event_type: AuditEventType,
    pub outcome: AuditOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<BTreeMap<String, AuditScalar>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditValidationError {
    UnsupportedContractVersion,
    EmptyOrOversizedIdentifier,
    TooManyDetails,
    InvalidDetailKey,
    OversizedDetailString,
    InvalidReasonCode,
    InvalidDigest,
}

impl AuditEvent {
    /// Validate bounded metadata fields before an event is serialized or stored.
    /// Does not redact sensitive values or validate timestamp authenticity.
    pub fn validate(&self) -> Result<(), AuditValidationError> {
        if self.contract_version != "1.0" {
            return Err(AuditValidationError::UnsupportedContractVersion);
        }
        for id in [&self.event_id, &self.task_id] {
            if id.trim().is_empty() || id.len() > 128 {
                return Err(AuditValidationError::EmptyOrOversizedIdentifier);
            }
        }
        if self.proposal_id.as_ref().is_some_and(|id| id.len() > 128 || id.trim().is_empty()) {
            return Err(AuditValidationError::EmptyOrOversizedIdentifier);
        }
        if let Some(details) = &self.details {
            if details.len() > 32 {
                return Err(AuditValidationError::TooManyDetails);
            }
            for (key, value) in details {
                if key.trim().is_empty() || key.len() > 128 {
                    return Err(AuditValidationError::InvalidDetailKey);
                }
                if let AuditScalar::String(s) = value {
                    if s.len() > 2000 {
                        return Err(AuditValidationError::OversizedDetailString);
                    }
                }
            }
        }
        if let Some(code) = &self.reason_code {
            if code.is_empty() || code.len() > 64
                || !code.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(AuditValidationError::InvalidReasonCode);
            }
        }
        if let Some(digest) = &self.action_digest {
            if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(AuditValidationError::InvalidDigest);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> AuditEvent {
        AuditEvent {
            contract_version: "1.0".into(),
            event_id: "event-1".into(),
            occurred_at: "2026-11-18T10:30:00Z".into(),
            task_id: "task-1".into(),
            proposal_id: Some("proposal-1".into()),
            action_type: Some(AuditActionType::ProposePatch),
            event_type: AuditEventType::PolicyDecision,
            outcome: AuditOutcome::ApprovalRequired,
            reason_code: Some("PATCH_REQUIRES_APPROVAL".into()),
            action_digest: Some("a".repeat(64)),
            details: Some(BTreeMap::from([
                ("patchBytes".into(), AuditScalar::Number(512.0)),
                ("policyVersion".into(), AuditScalar::String("proto-1".into())),
            ])),
        }
    }

    #[test]
    fn valid_event_serializes_camel_case_and_validates() {
        let e = event();
        assert_eq!(e.validate(), Ok(()));
        let json = serde_json::to_value(e).unwrap();
        assert_eq!(json["eventType"], "policy_decision");
        assert_eq!(json["actionType"], "propose_patch");
        assert_eq!(json["reasonCode"], "PATCH_REQUIRES_APPROVAL");
    }

    #[test]
    fn more_than_32_details_is_rejected() {
        let mut e = event();
        e.details = Some((0..33).map(|i| (format!("k{i}"), AuditScalar::Number(i as f64))).collect());
        assert_eq!(e.validate(), Err(AuditValidationError::TooManyDetails));
    }

    #[test]
    fn malformed_digest_is_rejected() {
        let mut e = event();
        e.action_digest = Some("not-a-digest".into());
        assert_eq!(e.validate(), Err(AuditValidationError::InvalidDigest));
    }

    #[test]
    fn oversized_string_detail_is_rejected() {
        let mut e = event();
        e.details = Some(BTreeMap::from([("value".into(), AuditScalar::String("x".repeat(2001)))]));
        assert_eq!(e.validate(), Err(AuditValidationError::OversizedDetailString));
    }

    #[test]
    fn sensitive_content_is_not_automatically_redacted() {
        // This documents the boundary: the DTO cannot determine whether scalar
        // values contain secrets. Redaction/classification is a separate layer.
        let mut e = event();
        e.details = Some(BTreeMap::from([("note".into(), AuditScalar::String("test-token=secret".into()))]));
        assert_eq!(e.validate(), Ok(()));
    }
}
