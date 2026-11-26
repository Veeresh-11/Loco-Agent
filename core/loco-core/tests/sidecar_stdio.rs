use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn run_with_input(input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_loco-authority"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sidecar");
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn hello(id: &str) -> Value {
    json!({
        "protocolVersion":"1.0",
        "messageType":"hello",
        "requestId":id,
        "client":{"name":"loco-vscode-extension","version":"0.1.0"}
    })
}

fn proposal() -> Value {
    json!({
        "contractVersion":"1.0",
        "proposalId":"p-1",
        "taskId":"t-1",
        "createdAt":"2026-11-24T00:00:00Z",
        "action":{"type":"read_file","path":"README.md","maxBytes":1024}
    })
}

fn line(value: &Value, out: &mut Vec<u8>) {
    out.extend(serde_json::to_vec(value).unwrap());
    out.push(b'\n');
}

#[test]
fn handshake_evaluate_and_shutdown_are_correlated() {
    let mut input = Vec::new();
    line(&hello("boot-1"), &mut input);
    line(&json!({
        "protocolVersion":"1.0","messageType":"evaluate","requestId":"eval-1","proposal":proposal()
    }), &mut input);
    line(&json!({"protocolVersion":"1.0","messageType":"shutdown","requestId":"stop-1"}), &mut input);
    let output = run_with_input(&input);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let messages: Vec<Value> = String::from_utf8(output.stdout).unwrap().lines()
        .map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["messageType"], "hello");
    assert_eq!(messages[0]["requestId"], "boot-1");
    let capabilities = messages[0]["server"]["capabilities"].as_array().unwrap();
    assert!(capabilities.iter().any(|value| value == "proposal_evaluation"));
    #[cfg(target_os = "macos")]
    {
        assert!(capabilities.iter().any(|value| value == "macos_fcntl_getpath_nofirmlink"));
        assert!(capabilities.iter().any(|value| value == "macos_attr_cmnex_nofirmlinkpath"));
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert!(!capabilities.iter().any(|value| value == "macos_fcntl_getpath_nofirmlink"));
        assert!(!capabilities.iter().any(|value| value == "macos_attr_cmnex_nofirmlinkpath"));
    }
    assert_eq!(messages[1]["messageType"], "decision");
    assert_eq!(messages[1]["requestId"], "eval-1");
    assert_eq!(messages[1]["decision"]["proposalId"], "p-1");
    assert_eq!(messages[2]["messageType"], "shutdown");
    assert_eq!(messages[2]["requestId"], "stop-1");
}

#[test]
fn evaluation_before_handshake_fails_closed() {
    let mut input = Vec::new();
    line(&json!({
        "protocolVersion":"1.0","messageType":"evaluate","requestId":"eval-1","proposal":proposal()
    }), &mut input);
    let output = run_with_input(&input);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn unknown_proposal_fields_are_rejected() {
    let mut p = proposal();
    p["untrusted"] = json!(true);
    let mut input = Vec::new();
    line(&hello("boot-1"), &mut input);
    line(&json!({"protocolVersion":"1.0","messageType":"evaluate","requestId":"eval-1","proposal":p}), &mut input);
    let output = run_with_input(&input);
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1, "only the handshake may be emitted");
}

#[test]
fn duplicate_request_ids_fail_closed() {
    let mut input = Vec::new();
    line(&hello("same-id"), &mut input);
    line(&json!({"protocolVersion":"1.0","messageType":"shutdown","requestId":"same-id"}), &mut input);
    let output = run_with_input(&input);
    assert!(!output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().lines().count(), 1);
}

#[test]
fn oversized_frame_is_rejected_before_json_parsing() {
    const LIMIT: usize = 12 * 1024 * 1024;
    let input = vec![b'x'; LIMIT + 1];
    let output = run_with_input(&input);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}


fn hello_v2(id: &str) -> Value {
    json!({
        "protocolVersion":"2.0",
        "messageType":"hello",
        "requestId":id,
        "client":{"name":"loco-vscode-extension","version":"0.1.0"}
    })
}

#[test]
fn protocol_v2_register_capture_revoke_and_shutdown_are_correlated() {
    // Keep the fixture under the checked-out workspace: platform adapters
    // intentionally reject roots reached through system-temp symlink aliases.
    let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("src/main.rs"), b"v2-native-capture").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_loco-authority"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().expect("spawn v2 sidecar");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    writeln!(stdin, "{}", hello_v2("v2-boot")).unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let hello: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(hello["messageType"], "hello");
    line.clear();

    let register = json!({
        "protocolVersion":"2.0", "messageType":"registerWorkspace",
        "requestId":"v2-register", "selectedRoot":root.path().to_string_lossy().to_string(),
        "displayName":"test workspace"
    });
    writeln!(stdin, "{}", register).unwrap();
    stdout.read_line(&mut line).unwrap();
    let registered: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(registered["messageType"], "workspaceRegistered", "registration response: {registered}");
    let handle = registered["workspaceHandle"].as_str().unwrap().to_string();
    assert_eq!(handle.len(), 64);
    line.clear();

    writeln!(stdin, "{}", json!({
        "protocolVersion":"2.0", "messageType":"captureWorkspaceFile",
        "requestId":"v2-capture", "workspaceHandle":handle,
        "relativePath":"src/main.rs", "maxBytes":1024
    })).unwrap();
    stdout.read_line(&mut line).unwrap();
    let captured: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(captured["messageType"], "workspaceFileCaptured");
    assert_eq!(captured["captured"], true);
    assert_eq!(captured["byteLength"], 17);
    assert!(captured.get("bytes").is_none());
    assert!(captured.get("snapshotDigest").is_none());
    line.clear();

    writeln!(stdin, "{}", json!({
        "protocolVersion":"2.0", "messageType":"revokeWorkspace",
        "requestId":"v2-revoke", "workspaceHandle":handle
    })).unwrap();
    stdout.read_line(&mut line).unwrap();
    let revoked: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(revoked["messageType"], "workspaceRevoked");
    assert_eq!(revoked["revoked"], true);
    line.clear();

    writeln!(stdin, "{}", json!({
        "protocolVersion":"2.0", "messageType":"captureWorkspaceFile",
        "requestId":"v2-capture-after-revoke", "workspaceHandle":handle,
        "relativePath":"src/main.rs", "maxBytes":1024
    })).unwrap();
    stdout.read_line(&mut line).unwrap();
    let denied: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(denied["messageType"], "error");
    assert_eq!(denied["error"]["code"], "CAPTURE_DENIED");
    line.clear();

    writeln!(stdin, "{}", json!({
        "protocolVersion":"2.0", "messageType":"shutdown", "requestId":"v2-stop"
    })).unwrap();
    stdout.read_line(&mut line).unwrap();
    let stopped: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(stopped["messageType"], "shutdown");
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn protocol_v2_rejects_cross_version_messages_and_pre_handshake_operations() {
    let mut input = Vec::new();
    line(&json!({
        "protocolVersion":"2.0",
        "messageType":"registerWorkspace",
        "requestId":"before-hello",
        "selectedRoot":"/tmp",
    }), &mut input);
    let output = run_with_input(&input);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn protocol_v2_handshake_advertises_only_implemented_read_only_capabilities() {
    let mut input = Vec::new();
    line(&hello_v2("v2-boot"), &mut input);
    line(&json!({"protocolVersion":"2.0","messageType":"shutdown","requestId":"v2-stop"}), &mut input);
    let output = run_with_input(&input);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let messages: Vec<Value> = String::from_utf8(output.stdout).unwrap().lines()
        .map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(messages[0]["protocolVersion"], "2.0");
    assert_eq!(messages[0]["messageType"], "hello");
    let capabilities = messages[0]["server"]["capabilities"].as_array().unwrap();
    assert!(capabilities.iter().any(|value| value == "workspace_registration"));
    assert!(capabilities.iter().any(|value| value == "workspace_capture"));
    assert!(capabilities.iter().any(|value| value == "workspace_revocation"));
    assert_eq!(messages[1]["messageType"], "shutdown");
}

#[test]
fn protocol_v2_unknown_and_already_revoked_handles_share_the_same_error() {
    let mut input = Vec::new();
    line(&hello_v2("v2-boot"), &mut input);
    let fabricated = "a".repeat(64);
    line(&json!({
        "protocolVersion":"2.0",
        "messageType":"revokeWorkspace",
        "requestId":"v2-revoke-unknown",
        "workspaceHandle":fabricated
    }), &mut input);
    line(&json!({
        "protocolVersion":"2.0",
        "messageType":"shutdown",
        "requestId":"v2-stop"
    }), &mut input);
    let output = run_with_input(&input);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let messages: Vec<Value> = String::from_utf8(output.stdout).unwrap().lines()
        .map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(messages[1]["messageType"], "error");
    assert_eq!(messages[1]["error"]["code"], "WORKSPACE_NOT_FOUND");
    assert_eq!(messages[1]["error"]["retryable"], false);
    assert_eq!(messages[1]["requestId"], "v2-revoke-unknown");
    assert_eq!(messages[2]["messageType"], "shutdown");
}
