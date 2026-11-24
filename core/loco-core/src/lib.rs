//! LOCO Agent security-core policy prototype.
//!
//! This crate intentionally evaluates proposals only. It does not execute commands,
//! read or write workspace files, or establish authorization for OS operations.
//! Filesystem and approval verification remain future milestones.

use serde::{Deserialize, Serialize};

pub const CONTRACT_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum ProposedAction {
    ReadFile {
        path: String,
        #[serde(default)]
        max_bytes: Option<u64>,
    },
    ProposePatch {
        path: String,
        patch: String,
        #[serde(default)]
        expected_target_sha256: Option<String>,
    },
    RunCommand {
        executable: String,
        args: Vec<String>,
        #[serde(default)]
        working_directory: Option<String>,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionProposal {
    #[serde(rename = "contractVersion")]
    pub contract_version: String,
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    #[serde(rename = "taskId")]
    pub task_id: String,
    pub action: ProposedAction,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Deny,
    ApprovalRequired,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityDecision {
    #[serde(rename = "contractVersion")]
    pub contract_version: String,
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    pub decision: DecisionKind,
    #[serde(rename = "reasonCode")]
    pub reason_code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyConfig {
    /// When false, read requests are never allowed by this prototype policy.
    pub allow_basic_reads: bool,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self { allow_basic_reads: true }
    }
}

/// Basic syntactic path guard only. Not a substitute for canonical filesystem
/// resolution, symlink checks, or race-resistant file access.
pub fn is_safe_relative_path(path: &str) -> bool {
    crate::portable_path::is_safe_portable_relative_path(path)
}

pub fn evaluate(proposal: &ActionProposal, config: PolicyConfig) -> SecurityDecision {
    let (decision, reason_code, message) = if proposal.contract_version != CONTRACT_VERSION {
        (
            DecisionKind::Deny,
            "UNSUPPORTED_CONTRACT_VERSION",
            "Proposal contract version is not supported.",
        )
    } else if proposal.proposal_id.trim().is_empty() || proposal.task_id.trim().is_empty() {
        (
            DecisionKind::Deny,
            "MISSING_IDENTIFIERS",
            "Proposal and task identifiers must be non-empty.",
        )
    } else {
        match &proposal.action {
            ProposedAction::ReadFile { path, max_bytes } => {
                if !is_safe_relative_path(path) {
                    (
                        DecisionKind::Deny,
                        "INVALID_PATH",
                        "Read path failed the basic relative-path check.",
                    )
                } else if max_bytes.is_some_and(|n| n == 0 || n > 10 * 1024 * 1024) {
                    (
                        DecisionKind::Deny,
                        "INVALID_READ_LIMIT",
                        "Read size limit must be between 1 byte and 10 MiB.",
                    )
                } else if config.allow_basic_reads {
                    (
                        DecisionKind::Allow,
                        "READ_POLICY_ALLOW",
                        "Basic read proposal allowed by prototype policy; filesystem checks are not implemented.",
                    )
                } else {
                    (
                        DecisionKind::Deny,
                        "READ_POLICY_DISABLED",
                        "Read proposals are disabled by policy.",
                    )
                }
            }
            ProposedAction::ProposePatch { path, patch, .. } => {
                if !is_safe_relative_path(path) {
                    (
                        DecisionKind::Deny,
                        "INVALID_PATH",
                        "Patch target failed the basic relative-path check.",
                    )
                } else if patch.is_empty() || patch.len() > 1_000_000 {
                    (
                        DecisionKind::Deny,
                        "INVALID_PATCH_SIZE",
                        "Patch must be non-empty and no larger than 1 MiB.",
                    )
                } else {
                    (
                        DecisionKind::ApprovalRequired,
                        "PATCH_REQUIRES_APPROVAL",
                        "Patch proposals require approval and additional target-state validation.",
                    )
                }
            }
            ProposedAction::RunCommand { executable, args, timeout_ms, .. } => {
                if executable.trim().is_empty() {
                    (
                        DecisionKind::Deny,
                        "EMPTY_EXECUTABLE",
                        "Command executable must be non-empty.",
                    )
                } else if args.len() > 128 || args.iter().any(|arg| arg.len() > 8192) {
                    (
                        DecisionKind::Deny,
                        "COMMAND_ARGUMENT_LIMIT",
                        "Command arguments exceed prototype limits.",
                    )
                } else if timeout_ms.is_some_and(|n| n < 100 || n > 120_000) {
                    (
                        DecisionKind::Deny,
                        "INVALID_TIMEOUT",
                        "Command timeout must be between 100 ms and 120 seconds.",
                    )
                } else {
                    (
                        DecisionKind::ApprovalRequired,
                        "COMMAND_REQUIRES_APPROVAL",
                        "Command execution is not implemented and requires approval in a future executor.",
                    )
                }
            }
        }
    };

    SecurityDecision {
        contract_version: CONTRACT_VERSION.to_string(),
        proposal_id: proposal.proposal_id.clone(),
        decision,
        reason_code: reason_code.to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal(action: ProposedAction) -> ActionProposal {
        ActionProposal {
            contract_version: CONTRACT_VERSION.into(),
            proposal_id: "proposal-1".into(),
            task_id: "task-1".into(),
            action,
            created_at: "2026-11-18T00:00:00Z".into(),
            reason: None,
        }
    }

    #[test]
    fn basic_relative_read_is_allowed_by_configured_prototype_policy() {
        let p = proposal(ProposedAction::ReadFile {
            path: "src/main.rs".into(),
            max_bytes: Some(2048),
        });
        assert_eq!(evaluate(&p, PolicyConfig::default()).decision, DecisionKind::Allow);
    }

    #[test]
    fn parent_traversal_read_is_denied() {
        let p = proposal(ProposedAction::ReadFile {
            path: "src/../../secret.txt".into(),
            max_bytes: None,
        });
        let d = evaluate(&p, PolicyConfig::default());
        assert_eq!(d.decision, DecisionKind::Deny);
        assert_eq!(d.reason_code, "INVALID_PATH");
    }

    #[test]
    fn absolute_windows_path_is_denied() {
        assert!(!is_safe_relative_path("C:\\Windows\\system.ini"));
    }

    #[test]
    fn sibling_prefix_is_syntactically_relative_but_not_workspace_proven() {
        // The prototype only checks syntax. Workspace containment must be added
        // before this decision is used to authorize any filesystem operation.
        assert!(is_safe_relative_path("../workspace-evil/file.txt") == false);
        assert!(is_safe_relative_path("workspace-evil/file.txt"));
    }

    #[test]
    fn policy_uses_shared_portable_path_rules() {
        for path in ["src/CON.txt", "src/com².log", "src/bad?name", "src/name.", "src/name "] {
            assert!(!is_safe_relative_path(path), "should reject: {path:?}");
        }
        assert!(is_safe_relative_path(".env"));
        assert!(is_safe_relative_path("src/main.rs"));
    }

    #[test]
    fn patches_always_require_approval() {
        let p = proposal(ProposedAction::ProposePatch {
            path: "src/main.rs".into(),
            patch: "diff content".into(),
            expected_target_sha256: None,
        });
        assert_eq!(evaluate(&p, PolicyConfig::default()).decision, DecisionKind::ApprovalRequired);
    }

    #[test]
    fn commands_always_require_approval() {
        let p = proposal(ProposedAction::RunCommand {
            executable: "cargo".into(),
            args: vec!["test".into()],
            working_directory: None,
            timeout_ms: Some(30_000),
        });
        assert_eq!(evaluate(&p, PolicyConfig::default()).decision, DecisionKind::ApprovalRequired);
    }

    #[test]
    fn unsupported_contract_version_is_denied() {
        let mut p = proposal(ProposedAction::ReadFile {
            path: "README.md".into(),
            max_bytes: None,
        });
        p.contract_version = "99.0".into();
        let d = evaluate(&p, PolicyConfig::default());
        assert_eq!(d.decision, DecisionKind::Deny);
        assert_eq!(d.reason_code, "UNSUPPORTED_CONTRACT_VERSION");
    }

    #[test]
    fn empty_executable_is_denied() {
        let p = proposal(ProposedAction::RunCommand {
            executable: " ".into(),
            args: vec![],
            working_directory: None,
            timeout_ms: None,
        });
        assert_eq!(evaluate(&p, PolicyConfig::default()).decision, DecisionKind::Deny);
    }
    #[test]
    fn action_serialization_uses_contract_camel_case_fields() {
        let action = ProposedAction::RunCommand {
            executable: "cargo".into(),
            args: vec!["test".into()],
            working_directory: Some("core".into()),
            timeout_ms: Some(5000),
        };
        let json = serde_json::to_value(action).unwrap();
        assert_eq!(json["type"], "run_command");
        assert_eq!(json["workingDirectory"], "core");
        assert_eq!(json["timeoutMs"], 5000);
        assert!(json.get("working_directory").is_none());
    }

    #[test]
    fn proposal_serialization_contains_schema_metadata() {
        let p = proposal(ProposedAction::ReadFile { path: "README.md".into(), max_bytes: None });
        let json = serde_json::to_value(p).unwrap();
        assert_eq!(json["contractVersion"], "1.0");
        assert_eq!(json["proposalId"], "proposal-1");
        assert_eq!(json["taskId"], "task-1");
        assert_eq!(json["createdAt"], "2026-11-18T00:00:00Z");
        assert!(json.get("created_at").is_none());
    }

}


pub mod path_validation;
pub mod portable_path;

pub mod workspace_fs;

pub mod approval_binding;

pub mod approval_clock;

pub mod approval_store;

pub mod canonical_action_digest;
pub mod canonical_snapshot_digest;

pub mod security_decision_contract;

pub mod audit_event;


pub mod patch_validation;

pub mod workspace_snapshot;


pub mod snapshot_payload;

pub mod workspace_registry;


#[cfg(target_os = "linux")]
pub mod linux_snapshot_adapter;


#[cfg(target_os = "windows")]
pub mod windows_root_handle;

#[cfg(target_os = "macos")]
pub mod macos_snapshot_adapter;
