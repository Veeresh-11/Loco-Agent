//! Versioned request contract for a future bounded, read-only workspace file capability.
//!
//! This module validates request shape only. It does not resolve a workspace,
//! open a file, establish containment, or authorize an operation. The current
//! sidecar does not advertise or implement this capability.
use serde::{Deserialize, Serialize};

use crate::portable_path::is_safe_portable_relative_path;

pub const WORKSPACE_READ_CONTRACT_VERSION: &str = "1.0";
pub const MAX_WORKSPACE_READ_REQUEST_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_WORKSPACE_ID_BYTES: usize = 128;
pub const MAX_WORKSPACE_RELATIVE_PATH_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceReadRequest {
    pub contract_version: String,
    /// Opaque identifier issued by the trusted extension host; never a path.
    pub workspace_id: String,
    /// Portable workspace-relative path, not an absolute OS path.
    pub relative_path: String,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceReadRequestError {
    UnsupportedVersion,
    InvalidWorkspaceId,
    InvalidRelativePath,
    InvalidByteLimit,
}

impl WorkspaceReadRequest {
    /// Validates only the versioned DTO and lexical path grammar.
    /// The workspace ID must be resolved against a host-owned registry by a
    /// future adapter; this method does not establish that binding.
    pub fn validate_contract(&self) -> Result<(), WorkspaceReadRequestError> {
        if self.contract_version != WORKSPACE_READ_CONTRACT_VERSION {
            return Err(WorkspaceReadRequestError::UnsupportedVersion);
        }
        if self.workspace_id.is_empty()
            || self.workspace_id.len() > MAX_WORKSPACE_ID_BYTES
            || !self.workspace_id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        {
            return Err(WorkspaceReadRequestError::InvalidWorkspaceId);
        }
        if self.relative_path.len() > MAX_WORKSPACE_RELATIVE_PATH_BYTES
            || !is_safe_portable_relative_path(&self.relative_path)
        {
            return Err(WorkspaceReadRequestError::InvalidRelativePath);
        }
        if self.max_bytes == 0 || self.max_bytes > MAX_WORKSPACE_READ_REQUEST_BYTES {
            return Err(WorkspaceReadRequestError::InvalidByteLimit);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> WorkspaceReadRequest {
        WorkspaceReadRequest {
            contract_version: WORKSPACE_READ_CONTRACT_VERSION.into(),
            workspace_id: "workspace-01".into(),
            relative_path: "src/main.rs".into(),
            max_bytes: 1024,
        }
    }

    #[test]
    fn accepts_valid_request_and_serializes_camel_case() {
        let request = valid();
        assert_eq!(request.validate_contract(), Ok(()));
        let json = serde_json::to_value(request).unwrap();
        assert_eq!(json["contractVersion"], "1.0");
        assert_eq!(json["workspaceId"], "workspace-01");
        assert_eq!(json["relativePath"], "src/main.rs");
        assert_eq!(json["maxBytes"], 1024);
        assert!(json.get("workspace_id").is_none());
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut request = valid();
        request.contract_version = "2.0".into();
        assert_eq!(request.validate_contract(), Err(WorkspaceReadRequestError::UnsupportedVersion));
    }

    #[test]
    fn rejects_invalid_workspace_ids() {
        for id in ["", "../workspace", "/tmp/workspace", "workspace id", "x".repeat(129).as_str()] {
            let mut request = valid();
            request.workspace_id = id.into();
            assert_eq!(request.validate_contract(), Err(WorkspaceReadRequestError::InvalidWorkspaceId));
        }
    }

    #[test]
    fn rejects_ambiguous_or_escaping_paths() {
        for path in ["", "../secret", "src/../secret", "/etc/passwd", "C:/secret", "src\\secret", "src//secret"] {
            let mut request = valid();
            request.relative_path = path.into();
            assert_eq!(request.validate_contract(), Err(WorkspaceReadRequestError::InvalidRelativePath));
        }
    }

    #[test]
    fn rejects_paths_over_the_byte_limit() {
        let mut request = valid();
        request.relative_path = format!("{}", "a".repeat(MAX_WORKSPACE_RELATIVE_PATH_BYTES + 1));
        assert_eq!(request.validate_contract(), Err(WorkspaceReadRequestError::InvalidRelativePath));
    }

    #[test]
    fn enforces_inclusive_byte_limit() {
        let mut request = valid();
        request.max_bytes = 1;
        assert_eq!(request.validate_contract(), Ok(()));
        request.max_bytes = MAX_WORKSPACE_READ_REQUEST_BYTES;
        assert_eq!(request.validate_contract(), Ok(()));
        request.max_bytes = 0;
        assert_eq!(request.validate_contract(), Err(WorkspaceReadRequestError::InvalidByteLimit));
        request.max_bytes = MAX_WORKSPACE_READ_REQUEST_BYTES + 1;
        assert_eq!(request.validate_contract(), Err(WorkspaceReadRequestError::InvalidByteLimit));
    }

    #[test]
    fn rejects_unknown_fields() {
        let value = serde_json::json!({
            "contractVersion": "1.0",
            "workspaceId": "workspace-01",
            "relativePath": "src/main.rs",
            "maxBytes": 1024,
            "execute": true
        });
        assert!(serde_json::from_value::<WorkspaceReadRequest>(value).is_err());
    }
}
