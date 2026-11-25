//! Strict protocol 2.1 DTOs for Rust-owned approval binding challenges.
//!
//! Protocol 2.0 remains unchanged. A 2.1 client must explicitly advertise
//! support for 2.1; challenge success is binding verification, not user approval.

use serde::{Deserialize, Serialize};

use crate::{authority_protocol_v2::valid_request_id, ProposedAction};

pub const AUTHORITY_PROTOCOL_V21: &str = "2.1";
pub const MAX_APPROVAL_TTL_SECONDS: u64 = 300;

fn validate_envelope(version: &str, request_id: &str) -> Result<(), V21ValidationError> {
    if version != AUTHORITY_PROTOCOL_V21 { return Err(V21ValidationError::UnsupportedVersion); }
    if !valid_request_id(request_id) { return Err(V21ValidationError::InvalidRequestId); }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HelloRequestV21 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub supported_protocol_versions: Vec<String>,
    pub client: ClientInfoV21,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientInfoV21 {
    pub name: String,
    pub version: String,
}

impl HelloRequestV21 {
    pub fn validate(&self) -> Result<(), V21ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "hello"
            || self.supported_protocol_versions.is_empty()
            || self.supported_protocol_versions.len() > 2
            || !self.supported_protocol_versions.iter().any(|v| v == AUTHORITY_PROTOCOL_V21)
            || self.supported_protocol_versions.iter().any(|v| v != "2.0" && v != AUTHORITY_PROTOCOL_V21)
        {
            return Err(V21ValidationError::UnsupportedVersion);
        }
        if self.client.name != "loco-vscode-extension"
            || self.client.version.trim().is_empty()
            || self.client.version.len() > 64
        {
            return Err(V21ValidationError::InvalidClient);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterWorkspaceRequestV21 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub selected_root: String,
    #[serde(default)]
    pub display_name: Option<String>,
}
impl RegisterWorkspaceRequestV21 {
    pub fn validate(&self) -> Result<(), V21ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "registerWorkspace" || self.selected_root.is_empty()
            || self.selected_root.len() > 4096 || !std::path::Path::new(&self.selected_root).is_absolute()
            || self.selected_root.contains('\0')
        { return Err(V21ValidationError::InvalidWorkspace); }
        if self.display_name.as_ref().is_some_and(|n| n.trim().is_empty() || n.len() > 128 || n.contains('\0')) {
            return Err(V21ValidationError::InvalidWorkspace);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureWorkspaceFileRequestV21 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub workspace_handle: String,
    pub relative_path: String,
    pub max_bytes: usize,
}
impl CaptureWorkspaceFileRequestV21 {
    pub fn validate(&self) -> Result<(), V21ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "captureWorkspaceFile" || !valid_workspace_handle(&self.workspace_handle)
            || !crate::portable_path::is_safe_portable_relative_path(&self.relative_path)
            || self.max_bytes == 0 || self.max_bytes > crate::snapshot_payload::MAX_SNAPSHOT_BYTES
        {
            return Err(V21ValidationError::InvalidWorkspace);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeWorkspaceRequestV21 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub workspace_handle: String,
}
impl RevokeWorkspaceRequestV21 {
    pub fn validate(&self) -> Result<(), V21ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "revokeWorkspace" || !valid_workspace_handle(&self.workspace_handle) {
            return Err(V21ValidationError::InvalidWorkspace);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IssueBindingChallengeRequestV21 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub workspace_handle: String,
    pub action: ProposedAction,
    pub ttl_seconds: u64,
}
impl IssueBindingChallengeRequestV21 {
    pub fn validate(&self) -> Result<(), V21ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "issueBindingChallenge" || !valid_workspace_handle(&self.workspace_handle) {
            return Err(V21ValidationError::InvalidWorkspace);
        }
        if !(1..=MAX_APPROVAL_TTL_SECONDS).contains(&self.ttl_seconds) {
            return Err(V21ValidationError::InvalidTtl);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsumeBindingChallengeRequestV21 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub workspace_handle: String,
    pub challenge_id: String,
    pub action: ProposedAction,
}
impl ConsumeBindingChallengeRequestV21 {
    pub fn validate(&self) -> Result<(), V21ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "consumeBindingChallenge" || !valid_workspace_handle(&self.workspace_handle)
            || self.challenge_id.len() != 64 || !self.challenge_id.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(V21ValidationError::InvalidChallenge);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluateRequestV21 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub proposal: crate::ActionProposal,
}
impl EvaluateRequestV21 {
    pub fn validate(&self) -> Result<(), V21ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "evaluate" { return Err(V21ValidationError::InvalidMessageType); }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShutdownRequestV21 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
}
impl ShutdownRequestV21 {
    pub fn validate(&self) -> Result<(), V21ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "shutdown" { return Err(V21ValidationError::InvalidMessageType); }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingChallengeIssuedResponseV21 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub challenge_id: String,
    pub action_digest: String,
    pub snapshot_digest: String,
    pub expires_at_unix: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingVerifiedResponseV21 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub binding_verified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V21ValidationError {
    UnsupportedVersion,
    InvalidRequestId,
    InvalidClient,
    InvalidWorkspace,
    InvalidTtl,
    InvalidChallenge,
    InvalidMessageType,
}

pub fn valid_workspace_handle(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn action() -> ProposedAction {
        ProposedAction::ReadFile { path: "src/main.rs".into(), max_bytes: Some(1024) }
    }

    #[test]
    fn hello_requires_explicit_v21_support_and_rejects_downgrade_only_clients() {
        let hello = HelloRequestV21 {
            protocol_version: "2.1".into(), message_type: "hello".into(), request_id: "hello-1".into(),
            supported_protocol_versions: vec!["2.0".into(), "2.1".into()],
            client: ClientInfoV21 { name: "loco-vscode-extension".into(), version: "1.0".into() },
        };
        assert!(hello.validate().is_ok());
        let mut unsupported = hello;
        unsupported.supported_protocol_versions = vec!["2.0".into()];
        assert_eq!(unsupported.validate(), Err(V21ValidationError::UnsupportedVersion));
    }

    #[test]
    fn issue_requires_bounded_ttl_and_strict_action_shape() {
        let mut request = IssueBindingChallengeRequestV21 {
            protocol_version: "2.1".into(), message_type: "issueBindingChallenge".into(), request_id: "issue-1".into(),
            workspace_handle: "a".repeat(64), action: action(), ttl_seconds: 60,
        };
        assert!(request.validate().is_ok());
        request.ttl_seconds = 301;
        assert_eq!(request.validate(), Err(V21ValidationError::InvalidTtl));
        let mut value = json!({
            "protocolVersion":"2.1","messageType":"issueBindingChallenge","requestId":"issue-2",
            "workspaceHandle":"a".repeat(64),"action":{"type":"read_file","path":"src/main.rs","maxBytes":1024},
            "ttlSeconds":60,"snapshotDigest":"caller-controlled"
        });
        assert!(serde_json::from_value::<IssueBindingChallengeRequestV21>(value.clone()).is_err());
        value.as_object_mut().unwrap().remove("snapshotDigest");
        assert!(serde_json::from_value::<IssueBindingChallengeRequestV21>(value).is_ok());
    }

    #[test]
    fn consume_requires_valid_opaque_challenge_and_workspace_handles() {
        let request = ConsumeBindingChallengeRequestV21 {
            protocol_version: "2.1".into(), message_type: "consumeBindingChallenge".into(), request_id: "consume-1".into(),
            workspace_handle: "b".repeat(64), challenge_id: "c".repeat(64), action: action(),
        };
        assert!(request.validate().is_ok());
        assert!(!valid_workspace_handle(&"A".repeat(64)));
    }
}
