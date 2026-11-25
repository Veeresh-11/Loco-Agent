#[path = "loco_authority/authority_v2.rs"]
mod authority_v2;

use loco_core::{evaluate, ActionProposal, PolicyConfig};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{self, BufRead, Read, Write};

const PROTOCOL_VERSION: &str = "1.0";
const MAX_FRAME_BYTES: usize = 12 * 1024 * 1024;
const MAX_REQUESTS_PER_PROCESS: usize = 4096;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HelloRequest {
    protocol_version: String,
    message_type: String,
    request_id: String,
    client: ClientInfo,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClientInfo {
    name: String,
    version: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvaluateRequest {
    protocol_version: String,
    message_type: String,
    request_id: String,
    proposal: ActionProposal,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShutdownRequest {
    protocol_version: String,
    message_type: String,
    request_id: String,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

fn read_frame<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    // Take limits the read itself, so a peer cannot force an unbounded line allocation.
    let mut limited = reader.take((MAX_FRAME_BYTES + 1) as u64);
    let mut frame = Vec::with_capacity(4096);
    let count = limited.read_until(b'\n', &mut frame)?;
    if count == 0 {
        return Ok(None);
    }
    if count > MAX_FRAME_BYTES || frame.last() != Some(&b'\n') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame exceeds limit or is unterminated"));
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

fn send_error<W: Write>(writer: &mut W, request_id: &str, code: &str, message: &str, retryable: bool) -> io::Result<()> {
    write_message(writer, &json!({
        "protocolVersion": PROTOCOL_VERSION,
        "messageType": "error",
        "requestId": request_id,
        "error": { "code": code, "message": message, "retryable": retryable }
    }))
}

fn run() -> Result<(), String> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let first_frame = match read_frame(&mut input) {
        Ok(Some(frame)) => frame,
        Ok(None) => return Ok(()),
        Err(error) => return Err(format!("invalid stdio frame: {error}")),
    };
    let first_text = std::str::from_utf8(&first_frame).map_err(|_| "frame is not UTF-8".to_string())?;
    let first_value: Value = serde_json::from_str(first_text).map_err(|_| "frame is not valid JSON".to_string())?;
    match first_value.get("protocolVersion").and_then(Value::as_str) {
        Some("2.0") => return authority_v2::run(first_frame, &mut input, &mut output),
        Some("1.0") => {}
        _ => return Err("unsupported or missing protocol version".into()),
    }

    let mut first_frame = Some(first_frame);
    let mut seen_ids = HashSet::new();
    let mut handshaken = false;
    let mut request_count = 0usize;

    loop {
        let frame = match first_frame.take() {
            Some(frame) => frame,
            None => match read_frame(&mut input) {
                Ok(Some(frame)) => frame,
                Ok(None) => return Ok(()),
                Err(error) => return Err(format!("invalid stdio frame: {error}")),
            },
        };
        let text = std::str::from_utf8(&frame).map_err(|_| "frame is not UTF-8".to_string())?;
        let value: Value = serde_json::from_str(text).map_err(|_| "frame is not valid JSON".to_string())?;
        let message_type = value.get("messageType").and_then(Value::as_str)
            .ok_or_else(|| "messageType is missing or invalid".to_string())?;

        match message_type {
            "hello" if !handshaken => {
                let request: HelloRequest = serde_json::from_value(value)
                    .map_err(|_| "invalid hello message".to_string())?;
                if request.message_type != "hello" || request.protocol_version != PROTOCOL_VERSION {
                    send_error(&mut output, &request.request_id, "UNSUPPORTED_PROTOCOL", "Protocol version is not supported.", false)
                        .map_err(|e| e.to_string())?;
                    return Err("unsupported protocol version".into());
                }
                if !valid_id(&request.request_id) || !seen_ids.insert(request.request_id.clone()) {
                    return Err("invalid or duplicate request id".into());
                }
                if request.client.name != "loco-vscode-extension" || request.client.version.trim().is_empty() || request.client.version.len() > 64 {
                    send_error(&mut output, &request.request_id, "MALFORMED_MESSAGE", "Parent identity metadata is invalid.", false)
                        .map_err(|e| e.to_string())?;
                    return Err("invalid parent metadata".into());
                }
                let mut capabilities = vec!["proposal_evaluation"];
                #[cfg(target_os = "macos")]
                if loco_core::macos_snapshot_adapter::fcntl_getpath_nofirmlink_available() {
                    capabilities.push("macos_fcntl_getpath_nofirmlink");
                }
                #[cfg(target_os = "macos")]
                if loco_core::macos_snapshot_adapter::attr_cmnex_nofirmlinkpath_available() {
                    capabilities.push("macos_attr_cmnex_nofirmlinkpath");
                }
                write_message(&mut output, &json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "messageType": "hello",
                    "requestId": request.request_id,
                    "server": {
                        "name": "loco-rust-authority",
                        "version": env!("CARGO_PKG_VERSION"),
                        "capabilities": capabilities
                    }
                })).map_err(|e| e.to_string())?;
                handshaken = true;
            }
            "hello" => return Err("duplicate or unexpected handshake".into()),
            "evaluate" if handshaken => {
                let request: EvaluateRequest = serde_json::from_value(value)
                    .map_err(|_| "invalid evaluate message".to_string())?;
                if request.message_type != "evaluate" || request.protocol_version != PROTOCOL_VERSION {
                    send_error(&mut output, &request.request_id, "UNSUPPORTED_PROTOCOL", "Protocol version is not supported.", false)
                        .map_err(|e| e.to_string())?;
                    return Err("unsupported protocol version".into());
                }
                if !valid_id(&request.request_id) || !seen_ids.insert(request.request_id.clone()) {
                    return Err("invalid or duplicate request id".into());
                }
                request_count += 1;
                if request_count > MAX_REQUESTS_PER_PROCESS {
                    send_error(&mut output, &request.request_id, "REQUEST_LIMIT_EXCEEDED", "Process request limit reached; restart the sidecar.", false)
                        .map_err(|e| e.to_string())?;
                    return Err("process request limit reached".into());
                }
                let decision = evaluate(&request.proposal, PolicyConfig::default());
                write_message(&mut output, &json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "messageType": "decision",
                    "requestId": request.request_id,
                    "decision": decision
                })).map_err(|e| e.to_string())?;
            }
            "shutdown" if handshaken => {
                let request: ShutdownRequest = serde_json::from_value(value)
                    .map_err(|_| "invalid shutdown message".to_string())?;
                if request.message_type != "shutdown" || request.protocol_version != PROTOCOL_VERSION
                    || !valid_id(&request.request_id) || !seen_ids.insert(request.request_id.clone()) {
                    return Err("invalid shutdown request".into());
                }
                write_message(&mut output, &json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "messageType": "shutdown",
                    "requestId": request.request_id,
                    "accepted": true
                })).map_err(|e| e.to_string())?;
                return Ok(());
            }
            "evaluate" | "shutdown" => return Err("handshake required".into()),
            _ => return Err("unknown message type".into()),
        }
        if seen_ids.len() >= MAX_REQUESTS_PER_PROCESS {
            return Err("request identifier limit reached".into());
        }
    }
}

fn main() {
    if let Err(error) = run() {
        // stdout is reserved exclusively for protocol frames.
        eprintln!("loco-authority: {error}");
        std::process::exit(2);
    }
}
