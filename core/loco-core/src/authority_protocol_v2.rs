//! Strict DTOs and lexical validation for the proposed authority stdio protocol v2.0.
//!
//! These DTOs define the strict wire contract. The v2 stdio handlers live in
//! the binary module and delegate filesystem acquisition only to the native
//! workspace factory and process-local registry.

use serde::{Deserialize, Serialize};

pub const AUTHORITY_PROTOCOL_V2: &str = "2.0";
pub const MAX_SELECTED_ROOT_BYTES: usize = 4096;
pub const MAX_DISPLAY_NAME_BYTES: usize = 128;
pub const MAX_WORKSPACE_HANDLE_BYTES: usize = 64;
pub const MAX_CAPTURE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolV2ValidationError {
    InvalidRequestId,
    InvalidProtocolVersion,
    InvalidSelectedRoot,
    InvalidDisplayName,
    InvalidWorkspaceHandle,
    InvalidRelativePath,
    InvalidByteLimit,
}

pub fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

fn validate_envelope(version: &str, request_id: &str) -> Result<(), ProtocolV2ValidationError> {
    if version != AUTHORITY_PROTOCOL_V2 {
        return Err(ProtocolV2ValidationError::InvalidProtocolVersion);
    }
    if !valid_request_id(request_id) {
        return Err(ProtocolV2ValidationError::InvalidRequestId);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HelloRequestV2 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub client: ClientInfoV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientInfoV2 {
    pub name: String,
    pub version: String,
}

impl HelloRequestV2 {
    pub fn validate(&self) -> Result<(), ProtocolV2ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "hello" {
            return Err(ProtocolV2ValidationError::InvalidProtocolVersion);
        }
        if self.client.name.is_empty() || self.client.name.len() > 128
            || self.client.version.trim().is_empty() || self.client.version.len() > 64 {
            return Err(ProtocolV2ValidationError::InvalidDisplayName);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterWorkspaceRequestV2 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub selected_root: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

impl RegisterWorkspaceRequestV2 {
    pub fn validate(&self) -> Result<(), ProtocolV2ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "registerWorkspace" {
            return Err(ProtocolV2ValidationError::InvalidProtocolVersion);
        }
        if self.selected_root.is_empty()
            || self.selected_root.len() > MAX_SELECTED_ROOT_BYTES
            || !std::path::Path::new(&self.selected_root).is_absolute()
            || self.selected_root.contains('\0') {
            return Err(ProtocolV2ValidationError::InvalidSelectedRoot);
        }
        if self.display_name.as_ref().is_some_and(|name|
            name.trim().is_empty() || name.len() > MAX_DISPLAY_NAME_BYTES || name.contains('\0')) {
            return Err(ProtocolV2ValidationError::InvalidDisplayName);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureWorkspaceFileRequestV2 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub workspace_handle: String,
    pub relative_path: String,
    pub max_bytes: usize,
}

impl CaptureWorkspaceFileRequestV2 {
    pub fn validate(&self) -> Result<(), ProtocolV2ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "captureWorkspaceFile" {
            return Err(ProtocolV2ValidationError::InvalidProtocolVersion);
        }
        if self.workspace_handle.len() != MAX_WORKSPACE_HANDLE_BYTES
            || !self.workspace_handle.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(ProtocolV2ValidationError::InvalidWorkspaceHandle);
        }
        if !crate::portable_path::is_safe_portable_relative_path(&self.relative_path) {
            return Err(ProtocolV2ValidationError::InvalidRelativePath);
        }
        if self.max_bytes == 0 || self.max_bytes > MAX_CAPTURE_BYTES {
            return Err(ProtocolV2ValidationError::InvalidByteLimit);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeWorkspaceRequestV2 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub workspace_handle: String,
}

impl RevokeWorkspaceRequestV2 {
    pub fn validate(&self) -> Result<(), ProtocolV2ValidationError> {
        validate_envelope(&self.protocol_version, &self.request_id)?;
        if self.message_type != "revokeWorkspace" {
            return Err(ProtocolV2ValidationError::InvalidProtocolVersion);
        }
        if self.workspace_handle.len() != MAX_WORKSPACE_HANDLE_BYTES
            || !self.workspace_handle.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(ProtocolV2ValidationError::InvalidWorkspaceHandle);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRegisteredResponseV2 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub workspace_handle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileCapturedResponseV2 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub captured: bool,
    pub byte_length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRevokedResponseV2 {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub revoked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_register() -> RegisterWorkspaceRequestV2 {
        RegisterWorkspaceRequestV2 {
            protocol_version: AUTHORITY_PROTOCOL_V2.into(),
            message_type: "registerWorkspace".into(),
            request_id: "register-1".into(),
            selected_root: if cfg!(windows) { "C:\\work\\repo".into() } else { "/work/repo".into() },
            display_name: Some("repo".into()),
        }
    }

    fn valid_capture() -> CaptureWorkspaceFileRequestV2 {
        CaptureWorkspaceFileRequestV2 {
            protocol_version: AUTHORITY_PROTOCOL_V2.into(),
            message_type: "captureWorkspaceFile".into(),
            request_id: "capture-1".into(),
            workspace_handle: "a".repeat(MAX_WORKSPACE_HANDLE_BYTES),
            relative_path: "src/main.rs".into(),
            max_bytes: 1024,
        }
    }

    #[test]
    fn registration_accepts_absolute_candidate_but_does_not_canonicalize_it() {
        let request = valid_register();
        assert_eq!(request.validate(), Ok(()));
        assert_eq!(request.selected_root, if cfg!(windows) { "C:\\work\\repo" } else { "/work/repo" });
    }

    #[test]
    fn registration_rejects_relative_empty_oversized_and_nul_paths() {
        let mut request = valid_register();
        request.selected_root = "relative/repo".into();
        assert_eq!(request.validate(), Err(ProtocolV2ValidationError::InvalidSelectedRoot));
        request.selected_root = String::new();
        assert_eq!(request.validate(), Err(ProtocolV2ValidationError::InvalidSelectedRoot));
        request.selected_root = "x".repeat(MAX_SELECTED_ROOT_BYTES + 1);
        assert_eq!(request.validate(), Err(ProtocolV2ValidationError::InvalidSelectedRoot));
        request.selected_root.push('\0');
        assert_eq!(request.validate(), Err(ProtocolV2ValidationError::InvalidSelectedRoot));
    }

    #[test]
    fn registration_display_name_is_display_only_and_bounded() {
        let mut request = valid_register();
        request.display_name = Some("x".repeat(MAX_DISPLAY_NAME_BYTES + 1));
        assert_eq!(request.validate(), Err(ProtocolV2ValidationError::InvalidDisplayName));
        request.display_name = None;
        assert_eq!(request.validate(), Ok(()));
    }

    #[test]
    fn capture_accepts_only_valid_handle_path_and_byte_limit() {
        assert_eq!(valid_capture().validate(), Ok(()));
        let mut request = valid_capture();
        request.workspace_handle = "A".repeat(64);
        assert_eq!(request.validate(), Err(ProtocolV2ValidationError::InvalidWorkspaceHandle));
        request.workspace_handle = "a".repeat(64);
        request.relative_path = "../escape".into();
        assert_eq!(request.validate(), Err(ProtocolV2ValidationError::InvalidRelativePath));
        request.relative_path = "src/main.rs".into();
        request.max_bytes = MAX_CAPTURE_BYTES + 1;
        assert_eq!(request.validate(), Err(ProtocolV2ValidationError::InvalidByteLimit));
        request.max_bytes = 0;
        assert_eq!(request.validate(), Err(ProtocolV2ValidationError::InvalidByteLimit));
    }

    #[test]
    fn revoke_requires_well_formed_opaque_handle() {
        let request = RevokeWorkspaceRequestV2 {
            protocol_version: AUTHORITY_PROTOCOL_V2.into(),
            message_type: "revokeWorkspace".into(),
            request_id: "revoke-1".into(),
            workspace_handle: "f".repeat(64),
        };
        assert_eq!(request.validate(), Ok(()));
        let mut invalid = request;
        invalid.workspace_handle = "not-a-handle".into();
        assert_eq!(invalid.validate(), Err(ProtocolV2ValidationError::InvalidWorkspaceHandle));
    }

    #[test]
    fn protocol_version_request_id_and_unknown_fields_are_strict() {
        let mut value = json!({
            "protocolVersion":"2.0",
            "messageType":"registerWorkspace",
            "requestId":"r-1",
            "selectedRoot": if cfg!(windows) { "C:\\repo" } else { "/repo" }
        });
        let mut request: RegisterWorkspaceRequestV2 = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(request.validate(), Ok(()));
        request.protocol_version = "1.0".into();
        assert_eq!(request.validate(), Err(ProtocolV2ValidationError::InvalidProtocolVersion));
        value["untrustedSnapshot"] = json!({"digest":"caller-controlled"});
        assert!(serde_json::from_value::<RegisterWorkspaceRequestV2>(value).is_err());
        assert!(!valid_request_id(""));
        assert!(!valid_request_id("bad id"));
        assert!(valid_request_id("req-1:part"));
    }

    #[test]
    fn capture_dto_rejects_caller_supplied_snapshot_metadata() {
        let mut value = serde_json::to_value(valid_capture()).unwrap();
        value["snapshotDigest"] = serde_json::Value::String("f".repeat(64));
        assert!(serde_json::from_value::<CaptureWorkspaceFileRequestV2>(value).is_err());
    }
}
