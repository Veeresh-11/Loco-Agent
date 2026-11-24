use serde_json::{json, Value};
use std::io::Write;
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
    assert!(capabilities.iter().any(|value| value == "macos_fcntl_getpath_nofirmlink"));
    #[cfg(not(target_os = "macos"))]
    assert!(!capabilities.iter().any(|value| value == "macos_fcntl_getpath_nofirmlink"));
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
