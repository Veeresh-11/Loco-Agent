//! Wire DTO for the published security-decision contract (v1.0).
 //!
 //! Policy evaluation remains pure and does not invent timestamps or request IDs.
 //! Callers must supply those metadata values from their own trusted context.
 
use serde::{Deserialize, Serialize};

use crate::{DecisionKind, SecurityDecision};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractDecision {
    Deny,
    ApprovalRequired,
    Allow,
}

impl From<DecisionKind> for ContractDecision {
    fn from(value: DecisionKind) -> Self {
        match value {
            DecisionKind::Deny => Self::Deny,
            DecisionKind::ApprovalRequired => Self::ApprovalRequired,
            DecisionKind::Allow => Self::Allow,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalChallengeContract {
    pub challenge_id: String,
    pub action_digest: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityDecisionContract {
    pub contract_version: String,
    pub request_id: String,
    pub proposal_id: String,
    pub decision: ContractDecision,
    pub reason_code: String,
    pub message: String,
    pub issued_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_challenge: Option<ApprovalChallengeContract>,
}

impl SecurityDecisionContract {
    /// Build the versioned wire response using metadata supplied by the caller.
    /// This constructor does not authenticate the caller, validate timestamps,
    /// or authorize any action; consumers must independently enforce those checks.
    pub fn from_evaluation(
        evaluated: &SecurityDecision,
        request_id: String,
        issued_at: String,
        expires_at: Option<String>,
        approval_challenge: Option<ApprovalChallengeContract>,
    ) -> Self {
        Self {
            contract_version: evaluated.contract_version.clone(),
            request_id,
            proposal_id: evaluated.proposal_id.clone(),
            decision: evaluated.decision.clone().into(),
            reason_code: evaluated.reason_code.clone(),
            message: evaluated.message.clone(),
            issued_at,
            expires_at,
            approval_challenge,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{evaluate, ActionProposal, PolicyConfig, ProposedAction};

    fn evaluated() -> SecurityDecision {
        let proposal = ActionProposal {
            contract_version: crate::CONTRACT_VERSION.into(),
            proposal_id: "proposal-42".into(),
            task_id: "task-7".into(),
            action: ProposedAction::ProposePatch {
                path: "src/lib.rs".into(),
                patch: "diff".into(),
                expected_target_sha256: None,
            },
            created_at: "2026-11-18T08:00:00Z".into(),
            reason: Some("Fix a bug".into()),
        };
        evaluate(&proposal, PolicyConfig::default())
    }

    #[test]
    fn wire_decision_serializes_contract_field_names_and_values() {
        let wire = SecurityDecisionContract::from_evaluation(
            &evaluated(),
            "request-42".into(),
            "2026-11-18T08:01:00Z".into(),
            Some("2026-11-18T08:02:00Z".into()),
            Some(ApprovalChallengeContract {
                challenge_id: "challenge-42".into(),
                action_digest: "a".repeat(64),
                expires_at: "2026-11-18T08:02:00Z".into(),
            }),
        );
        let json = serde_json::to_value(wire).unwrap();
        assert_eq!(json["contractVersion"], "1.0");
        assert_eq!(json["requestId"], "request-42");
        assert_eq!(json["proposalId"], "proposal-42");
        assert_eq!(json["decision"], "approval_required");
        assert_eq!(json["reasonCode"], "PATCH_REQUIRES_APPROVAL");
        assert_eq!(json["issuedAt"], "2026-11-18T08:01:00Z");
        assert_eq!(json["approvalChallenge"]["challengeId"], "challenge-42");
        assert!(json.get("request_id").is_none());
    }

    #[test]
    fn optional_wire_fields_are_omitted_when_absent() {
        let wire = SecurityDecisionContract::from_evaluation(
            &evaluated(),
            "request-42".into(),
            "2026-11-18T08:01:00Z".into(),
            None,
            None,
        );
        let json = serde_json::to_value(wire).unwrap();
        assert!(json.get("expiresAt").is_none());
        assert!(json.get("approvalChallenge").is_none());
    }
}
