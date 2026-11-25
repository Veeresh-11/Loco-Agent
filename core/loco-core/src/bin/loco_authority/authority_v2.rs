//! Protocol v2 stdio handlers. Read-only workspace registration and capture
//! are exposed only through Rust-owned native adapters and process-local handles.

use loco_core::{
    authority_protocol_v2::{
        CaptureWorkspaceFileRequestV2, HelloRequestV2, RegisterWorkspaceRequestV2,
        RevokeWorkspaceRequestV2, AUTHORITY_PROTOCOL_V2,
    },
    authority_protocol_v2_session::{AuthorityProtocolV2Session, ProtocolV2SessionError},
    evaluate, ActionProposal, PolicyConfig,
};
use loco_core::{snapshot_payload::SnapshotCaptureRequest, workspace_registry::WorkspaceRegistry};
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


#[cfg(test)]
mod adversarial_handler_tests {
    use super::*;
    use std::io::Cursor;

    fn hello() -> Vec<u8> {
        br#"{"protocolVersion":"2.0","messageType":"hello","requestId":"hello-1","client":{"name":"loco-vscode-extension","version":"0.1.0"}}"#.to_vec()
    }

    fn run_with(first: Vec<u8>, subsequent: Vec<u8>) -> (Result<(), String>, Vec<u8>) {
        let mut reader = io::BufReader::new(Cursor::new(subsequent));
        let mut writer = Vec::new();
        let result = run(first, &mut reader, &mut writer);
        (result, writer)
    }

    #[test]
    fn duplicate_request_id_is_rejected_by_the_stdio_handler() {
        let handle = "a".repeat(64);
        let request = format!(
            r#"{{"protocolVersion":"2.0","messageType":"revokeWorkspace","requestId":"hello-1","workspaceHandle":"{handle}"}}"#
        );
        let (result, output) = run_with(hello(), format!("{request}\n").into_bytes());
        assert!(result.unwrap_err().contains("DUPLICATE_REQUEST_ID"));
        let response = String::from_utf8(output).unwrap();
        assert_eq!(response.lines().count(), 1, "duplicate request must not produce a success response");
    }

    #[test]
    fn strict_dto_rejects_unknown_fields_before_registry_access() {
        let handle = "b".repeat(64);
        let request = format!(
            r#"{{"protocolVersion":"2.0","messageType":"revokeWorkspace","requestId":"revoke-1","workspaceHandle":"{handle}","unexpected":true}}"#
        );
        let (result, _) = run_with(hello(), format!("{request}\n").into_bytes());
        assert!(result.unwrap_err().contains("invalid revokeWorkspace message"));
    }

    #[test]
    fn cross_version_operation_is_rejected_without_downgrade() {
        let handle = "c".repeat(64);
        let request = format!(
            r#"{{"protocolVersion":"1.0","messageType":"revokeWorkspace","requestId":"revoke-1","workspaceHandle":"{handle}"}}"#
        );
        let (result, _) = run_with(hello(), format!("{request}\n").into_bytes());
        assert!(result.unwrap_err().contains("invalid revokeWorkspace message"));
    }

    #[test]
    fn a_second_hello_is_rejected_as_a_terminal_protocol_violation() {
        let second = br#"{"protocolVersion":"2.0","messageType":"hello","requestId":"hello-2","client":{"name":"loco-vscode-extension","version":"0.1.0"}}"#.to_vec();
        let (result, output) = run_with(hello(), [second, b"\n".to_vec()].concat());
        assert!(result.unwrap_err().contains("duplicate or unexpected handshake"));
        assert_eq!(String::from_utf8(output).unwrap().lines().count(), 1);
    }

    #[test]
    fn eof_after_handshake_exits_cleanly_and_releases_session_state() {
        let (result, output) = run_with(hello(), Vec::new());
        assert!(result.is_ok());
        let frames: Vec<Value> = String::from_utf8(output).unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["messageType"], "hello");
    }

    #[test]
    fn oversized_followup_frame_is_rejected_before_json_parsing() {
        let oversized = vec![b'x'; MAX_FRAME_BYTES + 1];
        let (result, _) = run_with(hello(), oversized);
        assert!(result.unwrap_err().contains("invalid stdio frame"));
    }
    #[test]
    fn eof_after_workspace_registration_releases_retained_root_handle() {
        let parent = std::env::current_dir().expect("current directory");
        let workspace = tempfile::tempdir_in(parent).expect("temporary workspace");
        let request = json!({
            "protocolVersion": "2.0",
            "messageType": "registerWorkspace",
            "requestId": "register-1",
            "selectedRoot": workspace.path().to_string_lossy(),
            "displayName": "eof-cleanup-test"
        })
        .to_string();

        let mut request_frame = request;
        request_frame.push('\n');
        let (result, output) = run_with(hello(), request_frame.into_bytes());
        assert!(result.is_ok(), "EOF should terminate the session cleanly: {result:?}");

        let frames: Vec<Value> = String::from_utf8(output)
            .expect("handler output is UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid response frame"))
            .collect();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1]["messageType"], "workspaceRegistered");
        assert_eq!(frames[1]["requestId"], "register-1");
        assert_eq!(frames[1]["workspaceHandle"].as_str().map(str::len), Some(64));

        // Closing/removing the selected root after EOF proves the handler dropped
        // its registry and the adapter's retained native root handle.
        workspace
            .close()
            .expect("EOF must release the registered workspace root");
    }

}
