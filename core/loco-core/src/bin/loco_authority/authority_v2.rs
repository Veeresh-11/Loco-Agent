//! Protocol v2 stdio handlers. Read-only workspace registration and capture
//! are exposed only through Rust-owned native adapters and process-local handles.

use loco_core::{
    authority_protocol_v2::{
        CaptureWorkspaceFileRequestV2, HelloRequestV2, RegisterWorkspaceRequestV2,
        RevokeWorkspaceRequestV2, AUTHORITY_PROTOCOL_V2,
    },
    authority_protocol_v2_session::{AuthorityProtocolV2Session, ProtocolV2SessionError},
    authority_protocol_v21::{
        CaptureWorkspaceFileRequestV21, ConsumeBindingChallengeRequestV21, EvaluateRequestV21, HelloRequestV21,
        IssueBindingChallengeRequestV21, RegisterWorkspaceRequestV21,
        RevokeWorkspaceRequestV21, ShutdownRequestV21, AUTHORITY_PROTOCOL_V21,
    },
    approval_store::{ApprovalStore, StoreError},
    evaluate, ActionProposal, PolicyConfig, ProposedAction,
};
use loco_core::{
    snapshot_payload::{SnapshotCaptureRequest, MAX_SNAPSHOT_BYTES},
    workspace_registry::WorkspaceRegistry,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Read, Write},
    path::Path,
};

const MAX_FRAME_BYTES: usize = 12 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShutdownRequestV2 {
    protocol_version: String,
    message_type: String,
    request_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvaluateRequestV2 {
    protocol_version: String,
    message_type: String,
    request_id: String,
    proposal: ActionProposal,
}

fn read_frame<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut limited = reader.take((MAX_FRAME_BYTES + 1) as u64);
    let mut frame = Vec::with_capacity(4096);
    let count = limited.read_until(b'\n', &mut frame)?;
    if count == 0 {
        return Ok(None);
    }
    if count > MAX_FRAME_BYTES || frame.last() != Some(&b'\n') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid frame length"));
    }
    frame.pop();
    if frame.last() == Some(&b'\r') {
        frame.pop();
    }
    if frame.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "blank frame"));
    }
    Ok(Some(frame))
}

fn write_message<W: Write>(writer: &mut W, value: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn send_error<W: Write>(
    writer: &mut W,
    request_id: &str,
    code: &str,
    message: &str,
    retryable: bool,
) -> io::Result<()> {
    write_message(writer, &json!({
        "protocolVersion": AUTHORITY_PROTOCOL_V2,
        "messageType": "error",
        "requestId": request_id,
        "error": { "code": code, "message": message, "retryable": retryable }
    }))
}

fn random_workspace_identity() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| "workspace identity entropy unavailable".to_string())?;
    let mut output = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").map_err(|_| "workspace identity encoding failed".to_string())?;
    }
    Ok(output)
}

fn request_id(value: &Value) -> Option<&str> {
    value.get("requestId").and_then(Value::as_str)
}

fn session_error(error: ProtocolV2SessionError) -> (&'static str, &'static str) {
    match error {
        ProtocolV2SessionError::InvalidRequestId => ("MALFORMED_MESSAGE", "Request identifier is invalid."),
        ProtocolV2SessionError::HandshakeRequired => ("HANDSHAKE_REQUIRED", "A valid handshake is required."),
        ProtocolV2SessionError::UnexpectedHandshake => ("MALFORMED_MESSAGE", "Unexpected handshake."),
        ProtocolV2SessionError::DuplicateRequestId => ("DUPLICATE_REQUEST_ID", "Request identifier was already used."),
        ProtocolV2SessionError::RequestLimitExceeded => ("REQUEST_LIMIT_EXCEEDED", "Process request limit reached."),
        ProtocolV2SessionError::SessionTerminated => ("MALFORMED_MESSAGE", "Protocol session is closed."),
    }
}

/// Handle the first v2 hello and all subsequent requests. The caller has already
/// consumed the first bounded frame; this function owns all remaining state.
pub fn run<R: BufRead, W: Write>(
    first_frame: Vec<u8>,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    let first_text = std::str::from_utf8(&first_frame).map_err(|_| "frame is not UTF-8".to_string())?;
    let first_value: Value = serde_json::from_str(first_text).map_err(|_| "frame is not valid JSON".to_string())?;
    if first_value.get("protocolVersion").and_then(Value::as_str) == Some(AUTHORITY_PROTOCOL_V21) {
        return run_v21(first_frame, reader, writer);
    }
    let hello: HelloRequestV2 = serde_json::from_value(first_value)
        .map_err(|_| "invalid protocol v2 hello".to_string())?;
    hello.validate().map_err(|_| "invalid protocol v2 hello".to_string())?;
    if hello.client.name != "loco-vscode-extension" {
        return Err("unsupported client metadata".into());
    }

    let mut session = AuthorityProtocolV2Session::new();
    session.accept_hello(&hello.request_id).map_err(|_| "invalid protocol v2 handshake state".to_string())?;
    let mut registry = WorkspaceRegistry::new();

    write_message(writer, &json!({
        "protocolVersion": AUTHORITY_PROTOCOL_V2,
        "messageType": "hello",
        "requestId": hello.request_id,
        "server": {
            "name": "loco-rust-authority",
            "version": env!("CARGO_PKG_VERSION"),
            "capabilities": [
                "proposal_evaluation",
                "workspace_registration",
                "workspace_capture",
                "workspace_revocation"
            ]
        }
    })).map_err(|e| e.to_string())?;

    loop {
        let frame = match read_frame(reader).map_err(|e| format!("invalid stdio frame: {e}"))? {
            Some(frame) => frame,
            None => return Ok(()), // EOF drops the registry and every retained root handle.
        };
        let text = std::str::from_utf8(&frame).map_err(|_| "frame is not UTF-8".to_string())?;
        let value: Value = serde_json::from_str(text).map_err(|_| "frame is not valid JSON".to_string())?;
        let message_type = value.get("messageType").and_then(Value::as_str)
            .ok_or_else(|| "messageType is missing or invalid".to_string())?;
        let id = request_id(&value).unwrap_or("");

        match message_type {
            "registerWorkspace" => {
                let request: RegisterWorkspaceRequestV2 = serde_json::from_value(value)
                    .map_err(|_| "invalid registerWorkspace message".to_string())?;
                request.validate().map_err(|_| "invalid registerWorkspace message".to_string())?;
                session.accept_operation(&request.request_id).map_err(|e| {
                    let (code, _) = session_error(e);
                    format!("{code}: invalid protocol session")
                })?;
                let workspace_id = random_workspace_identity()?;
                match registry.register_selected_workspace(Path::new(&request.selected_root), workspace_id) {
                    Ok(handle) => write_message(writer, &json!({
                        "protocolVersion": AUTHORITY_PROTOCOL_V2,
                        "messageType": "workspaceRegistered",
                        "requestId": request.request_id,
                        "workspaceHandle": handle.as_str()
                    })).map_err(|e| e.to_string())?,
                    Err(_) => {
                        send_error(writer, &request.request_id, "INVALID_WORKSPACE_SELECTION",
                            "Workspace selection could not be registered.", false).map_err(|e| e.to_string())?;
                    }
                }
            }
            "captureWorkspaceFile" => {
                let request: CaptureWorkspaceFileRequestV2 = serde_json::from_value(value)
                    .map_err(|_| "invalid captureWorkspaceFile message".to_string())?;
                request.validate().map_err(|_| "invalid captureWorkspaceFile message".to_string())?;
                session.accept_operation(&request.request_id).map_err(|e| {
                    let (code, _) = session_error(e);
                    format!("{code}: invalid protocol session")
                })?;
                let capture_request = SnapshotCaptureRequest::new(&request.relative_path, request.max_bytes)
                    .map_err(|_| "invalid capture request".to_string())?;
                match registry.capture_by_wire_handle(&request.workspace_handle, &capture_request) {
                    Ok(captured) => {
                        let byte_length = captured.bytes().len();
                        drop(captured); // trusted bytes and metadata remain Rust-internal and are discarded.
                        write_message(writer, &json!({
                            "protocolVersion": AUTHORITY_PROTOCOL_V2,
                            "messageType": "workspaceFileCaptured",
                            "requestId": request.request_id,
                            "captured": true,
                            "byteLength": byte_length
                        })).map_err(|e| e.to_string())?;
                    }
                    Err(_) => send_error(writer, &request.request_id, "CAPTURE_DENIED",
                        "Workspace file capture was denied.", false).map_err(|e| e.to_string())?,
                }
            }
            "revokeWorkspace" => {
                let request: RevokeWorkspaceRequestV2 = serde_json::from_value(value)
                    .map_err(|_| "invalid revokeWorkspace message".to_string())?;
                request.validate().map_err(|_| "invalid revokeWorkspace message".to_string())?;
                session.accept_operation(&request.request_id).map_err(|e| {
                    let (code, _) = session_error(e);
                    format!("{code}: invalid protocol session")
                })?;
                if registry.revoke_by_wire_handle(&request.workspace_handle) {
                    write_message(writer, &json!({
                        "protocolVersion": AUTHORITY_PROTOCOL_V2,
                        "messageType": "workspaceRevoked",
                        "requestId": request.request_id,
                        "revoked": true
                    })).map_err(|e| e.to_string())?;
                } else {
                    send_error(writer, &request.request_id, "WORKSPACE_NOT_FOUND",
                        "Workspace registration was not found.", false).map_err(|e| e.to_string())?;
                }
            }
            "evaluate" => {
                let request: EvaluateRequestV2 = serde_json::from_value(value)
                    .map_err(|_| "invalid evaluate message".to_string())?;
                if request.protocol_version != AUTHORITY_PROTOCOL_V2 || request.message_type != "evaluate" {
                    return Err("unsupported protocol version".into());
                }
                session.accept_operation(&request.request_id).map_err(|e| {
                    let (code, _) = session_error(e);
                    format!("{code}: invalid protocol session")
                })?;
                let decision = evaluate(&request.proposal, PolicyConfig::default());
                write_message(writer, &json!({
                    "protocolVersion": AUTHORITY_PROTOCOL_V2,
                    "messageType": "decision",
                    "requestId": request.request_id,
                    "decision": decision
                })).map_err(|e| e.to_string())?;
            }
            "shutdown" => {
                let request: ShutdownRequestV2 = serde_json::from_value(value)
                    .map_err(|_| "invalid shutdown message".to_string())?;
                if request.protocol_version != AUTHORITY_PROTOCOL_V2 || request.message_type != "shutdown" {
                    return Err("unsupported protocol version".into());
                }
                session.accept_shutdown(&request.request_id).map_err(|_| "invalid shutdown lifecycle".to_string())?;
                drop(registry);
                write_message(writer, &json!({
                    "protocolVersion": AUTHORITY_PROTOCOL_V2,
                    "messageType": "shutdown",
                    "requestId": request.request_id,
                    "accepted": true
                })).map_err(|e| e.to_string())?;
                return Ok(());
            }
            "hello" => return Err("duplicate or unexpected handshake".into()),
            _ => {
                let _ = id;
                return Err("unknown protocol v2 message type".into());
            }
        }
    }
}


fn capture_request_for_action(action: &ProposedAction) -> Result<SnapshotCaptureRequest, &'static str> {
    let (path, max_bytes) = match action {
        ProposedAction::ReadFile { path, max_bytes } => {
            let limit = max_bytes.unwrap_or(MAX_SNAPSHOT_BYTES as u64);
            if limit == 0 || limit > MAX_SNAPSHOT_BYTES as u64 { return Err("CAPTURE_DENIED"); }
            (path.as_str(), limit as usize)
        }
        ProposedAction::ProposePatch { path, patch, expected_target_sha256 } => {
            if patch.is_empty() || patch.len() > 1_000_000 || expected_target_sha256.is_none() {
                return Err("UNSUPPORTED_ACTION");
            }
            (path.as_str(), MAX_SNAPSHOT_BYTES)
        }
        ProposedAction::RunCommand { .. } => return Err("UNSUPPORTED_ACTION"),
    };
    SnapshotCaptureRequest::new(path, max_bytes).map_err(|_| "CAPTURE_DENIED")
}

fn store_error_code(error: &StoreError) -> &'static str {
    match error {
        StoreError::NotFound | StoreError::AlreadyConsumedOrExpired => "BINDING_CHALLENGE_EXPIRED_OR_CONSUMED",
        StoreError::Binding(_) => "BINDING_MISMATCH",
        StoreError::InvalidChallenge => "BINDING_CHALLENGE_INVALID",
        StoreError::InvalidRegistration => "WORKSPACE_NOT_FOUND",
        StoreError::CapacityExceeded | StoreError::PersistentCapacityExceeded => "CAPACITY_EXCEEDED",
        _ => "BINDING_STORE_UNAVAILABLE",
    }
}

fn send_error_v21<W: Write>(
    writer: &mut W,
    request_id: &str,
    code: &str,
    message: &str,
    retryable: bool,
) -> io::Result<()> {
    write_message(writer, &json!({
        "protocolVersion": AUTHORITY_PROTOCOL_V21,
        "messageType": "error",
        "requestId": request_id,
        "error": { "code": code, "message": message, "retryable": retryable }
    }))
}

fn run_v21<R: BufRead, W: Write>(first_frame: Vec<u8>, reader: &mut R, writer: &mut W) -> Result<(), String> {
    let text = std::str::from_utf8(&first_frame).map_err(|_| "frame is not UTF-8".to_string())?;
    let value: Value = serde_json::from_str(text).map_err(|_| "frame is not valid JSON".to_string())?;
    let hello: HelloRequestV21 = serde_json::from_value(value).map_err(|_| "invalid protocol v2.1 hello".to_string())?;
    hello.validate().map_err(|_| "unsupported protocol v2.1 handshake".to_string())?;
    let mut session = AuthorityProtocolV2Session::new();
    session.accept_hello(&hello.request_id).map_err(|_| "invalid protocol handshake state".to_string())?;
    let mut registry = WorkspaceRegistry::new();
    let mut approvals = ApprovalStore::in_memory().map_err(|_| "approval store unavailable".to_string())?;

    write_message(writer, &json!({
        "protocolVersion": AUTHORITY_PROTOCOL_V21, "messageType": "hello", "requestId": hello.request_id,
        "negotiatedProtocolVersion": AUTHORITY_PROTOCOL_V21, "supportedProtocolVersions": ["2.0", "2.1"],
        "server": { "name": "loco-rust-authority", "version": env!("CARGO_PKG_VERSION"),
            "capabilities": ["proposal_evaluation", "workspace_registration", "workspace_capture", "workspace_revocation", "approval_binding_challenge"] }
    })).map_err(|e| e.to_string())?;

    loop {
        let frame = match read_frame(reader).map_err(|e| format!("invalid stdio frame: {e}"))? {
            Some(frame) => frame, None => return Ok(()),
        };
        let text = std::str::from_utf8(&frame).map_err(|_| "frame is not UTF-8".to_string())?;
        let value: Value = serde_json::from_str(text).map_err(|_| "frame is not valid JSON".to_string())?;
        let message_type = value.get("messageType").and_then(Value::as_str)
            .ok_or_else(|| "messageType is missing or invalid".to_string())?;
        let id = request_id(&value).unwrap_or("");

        match message_type {
            "registerWorkspace" => {
                let request: RegisterWorkspaceRequestV21 = serde_json::from_value(value).map_err(|_| "invalid registerWorkspace message".to_string())?;
                request.validate().map_err(|_| "invalid registerWorkspace message".to_string())?;
                session.accept_operation(&request.request_id).map_err(|e| { let (code, _) = session_error(e); format!("{code}: invalid protocol session") })?;
                let workspace_id = random_workspace_identity()?;
                match registry.register_selected_workspace(Path::new(&request.selected_root), workspace_id) {
                    Ok(handle) => write_message(writer, &json!({"protocolVersion":AUTHORITY_PROTOCOL_V21,"messageType":"workspaceRegistered","requestId":request.request_id,"workspaceHandle":handle.as_str()})).map_err(|e|e.to_string())?,
                    Err(_) => send_error_v21(writer,&request.request_id,"INVALID_WORKSPACE_SELECTION","Workspace selection could not be registered.",false).map_err(|e|e.to_string())?,
                }
            }
            "captureWorkspaceFile" => {
                let request: CaptureWorkspaceFileRequestV21 = serde_json::from_value(value).map_err(|_| "invalid captureWorkspaceFile message".to_string())?;
                request.validate().map_err(|_| "invalid captureWorkspaceFile message".to_string())?;
                session.accept_operation(&request.request_id).map_err(|e| { let (code, _) = session_error(e); format!("{code}: invalid protocol session") })?;
                let capture_request = SnapshotCaptureRequest::new(&request.relative_path, request.max_bytes)
                    .map_err(|_| "invalid capture request".to_string())?;
                match registry.capture_by_wire_handle(&request.workspace_handle, &capture_request) {
                    Ok(captured) => {
                        let byte_length = captured.bytes().len();
                        drop(captured);
                        write_message(writer, &json!({"protocolVersion":AUTHORITY_PROTOCOL_V21,"messageType":"workspaceFileCaptured",
                            "requestId":request.request_id,"captured":true,"byteLength":byte_length})).map_err(|e|e.to_string())?;
                    }
                    Err(_) => send_error_v21(writer,&request.request_id,"CAPTURE_DENIED","Workspace file capture was denied.",false).map_err(|e|e.to_string())?,
                }
            }
            "issueBindingChallenge" => {
                let request: IssueBindingChallengeRequestV21 = serde_json::from_value(value).map_err(|_| "invalid issueBindingChallenge message".to_string())?;
                request.validate().map_err(|_| "invalid issueBindingChallenge message".to_string())?;
                session.accept_operation(&request.request_id).map_err(|e| { let (code, _) = session_error(e); format!("{code}: invalid protocol session") })?;
                let capture_request = match capture_request_for_action(&request.action) {
                    Ok(request) => request,
                    Err(code) => { send_error_v21(writer,&request.request_id,code,"The requested action cannot be bound to a workspace snapshot.",false).map_err(|e|e.to_string())?; continue; }
                };
                let captured = match registry.capture_by_wire_handle(&request.workspace_handle,&capture_request) {
                    Ok(captured) => captured,
                    Err(_) => { send_error_v21(writer,&request.request_id,"CAPTURE_DENIED","Workspace capture was denied.",false).map_err(|e|e.to_string())?; continue; }
                };
                match approvals.issue_for_registration(&request.action,&captured,&request.workspace_handle,request.ttl_seconds) {
                    Ok(challenge) => write_message(writer,&json!({
                        "protocolVersion":AUTHORITY_PROTOCOL_V21,"messageType":"bindingChallengeIssued","requestId":request.request_id,
                        "challengeId":challenge.challenge_id,"actionDigest":challenge.action_digest,
                        "snapshotDigest":challenge.snapshot_digest,"expiresAtUnix":challenge.expires_at_unix
                    })).map_err(|e|e.to_string())?,
                    Err(error) => { let code=store_error_code(&error); send_error_v21(writer,&request.request_id,code,"Binding challenge could not be issued.",false).map_err(|e|e.to_string())?; }
                }
            }
            "consumeBindingChallenge" => {
                let request: ConsumeBindingChallengeRequestV21 = serde_json::from_value(value).map_err(|_| "invalid consumeBindingChallenge message".to_string())?;
                request.validate().map_err(|_| "invalid consumeBindingChallenge message".to_string())?;
                session.accept_operation(&request.request_id).map_err(|e| { let (code, _) = session_error(e); format!("{code}: invalid protocol session") })?;
                let capture_request = match capture_request_for_action(&request.action) {
                    Ok(request) => request,
                    Err(code) => { send_error_v21(writer,&request.request_id,code,"The requested action cannot be verified.",false).map_err(|e|e.to_string())?; continue; }
                };
                let captured = match registry.capture_by_wire_handle(&request.workspace_handle,&capture_request) {
                    Ok(captured) => captured,
                    Err(_) => { send_error_v21(writer,&request.request_id,"BINDING_CHALLENGE_EXPIRED_OR_CONSUMED","Binding challenge is unavailable.",false).map_err(|e|e.to_string())?; continue; }
                };
                match approvals.consume_for_registration(&request.challenge_id,&request.action,&captured,&request.workspace_handle) {
                    Ok(()) => write_message(writer,&json!({"protocolVersion":AUTHORITY_PROTOCOL_V21,"messageType":"bindingVerified","requestId":request.request_id,"bindingVerified":true})).map_err(|e|e.to_string())?,
                    Err(error) => { let code=store_error_code(&error); send_error_v21(writer,&request.request_id,code,"Binding challenge could not be verified.",false).map_err(|e|e.to_string())?; }
                }
            }
            "revokeWorkspace" => {
                let request: RevokeWorkspaceRequestV21 = serde_json::from_value(value).map_err(|_| "invalid revokeWorkspace message".to_string())?;
                request.validate().map_err(|_| "invalid revokeWorkspace message".to_string())?;
                session.accept_operation(&request.request_id).map_err(|e| { let (code, _) = session_error(e); format!("{code}: invalid protocol session") })?;
                if registry.revoke_by_wire_handle(&request.workspace_handle) {
                    if approvals.revoke_registration(&request.workspace_handle).is_err() {
                        send_error_v21(writer,&request.request_id,"BINDING_STORE_UNAVAILABLE","Workspace was revoked but pending bindings could not be cleared.",false).map_err(|e|e.to_string())?;
                    } else {
                        write_message(writer,&json!({"protocolVersion":AUTHORITY_PROTOCOL_V21,"messageType":"workspaceRevoked","requestId":request.request_id,"revoked":true})).map_err(|e|e.to_string())?;
                    }
                } else { send_error_v21(writer,&request.request_id,"WORKSPACE_NOT_FOUND","Workspace registration was not found.",false).map_err(|e|e.to_string())?; }
            }
            "evaluate" => {
                let request: EvaluateRequestV21 = serde_json::from_value(value).map_err(|_| "invalid evaluate message".to_string())?;
                request.validate().map_err(|_| "invalid evaluate message".to_string())?;
                session.accept_operation(&request.request_id).map_err(|e| { let (code, _) = session_error(e); format!("{code}: invalid protocol session") })?;
                let decision = evaluate(&request.proposal,PolicyConfig::default());
                write_message(writer,&json!({"protocolVersion":AUTHORITY_PROTOCOL_V21,"messageType":"decision","requestId":request.request_id,"decision":decision})).map_err(|e|e.to_string())?;
            }
            "shutdown" => {
                let request: ShutdownRequestV21 = serde_json::from_value(value).map_err(|_| "invalid shutdown message".to_string())?;
                request.validate().map_err(|_| "invalid shutdown message".to_string())?;
                session.accept_shutdown(&request.request_id).map_err(|_| "invalid shutdown lifecycle".to_string())?;
                drop(registry); drop(approvals);
                write_message(writer,&json!({"protocolVersion":AUTHORITY_PROTOCOL_V21,"messageType":"shutdown","requestId":request.request_id,"accepted":true})).map_err(|e|e.to_string())?;
                return Ok(());
            }
            "hello" => return Err("duplicate or unexpected handshake".into()),
            _ => { let _ = id; return Err("unknown protocol v2.1 message type".into()); }
        }
    }
}
