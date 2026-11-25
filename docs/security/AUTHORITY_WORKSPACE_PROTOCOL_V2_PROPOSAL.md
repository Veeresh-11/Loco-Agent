# Rust Authority Workspace Protocol v2.0 — Contract Proposal

**Status:** Proposed contract; not implemented and not approved for production use  
**Depends on:** native adapter factory and process-local workspace registry (PR #131 and predecessor PRs)  
**Scope:** explicit workspace registration and bounded read-only snapshot capture  
**Security posture:** fail closed; no execution capability, writes, patch application, or command execution

## 1. Purpose and boundary

The current stdio authority implements protocol 1.0: `hello`, `evaluate`, and `shutdown`. It does not register workspaces or capture files. The Rust registry and native adapters exist as library APIs, but are not reachable through the production sidecar request path.

This proposal adds two operations:

- `registerWorkspace`: accept an explicitly selected candidate root, validate it using the native platform adapter factory, retain the adapter in the Rust process, and return an opaque process-local handle.
- `captureWorkspaceFile`: accept that handle and a bounded portable relative path, capture through the retained adapter, and return only a bounded acknowledgement. The captured bytes and trusted snapshot metadata remain inside Rust and are discarded after this read-only prototype operation.

A successful registration or capture is not authentication, user approval, policy approval, or permission to execute an action. A policy `allow` remains a policy result only.

## 2. Versioning and compatibility

The new contract is protocol **2.0**. The protocol version is exact on every request and response. A client requesting 2.0 must not silently downgrade to 1.0, and a 1.0 client must not send 2.0 messages. The sidecar may continue to support the existing 1.0 surface during a deliberate migration, but the two versions must have separate strict DTO validation paths. Unknown fields remain rejected.

All messages are newline-delimited UTF-8 JSON objects. Stdout contains protocol frames only; diagnostics go to stderr. Existing frame, request-ID, per-process request-count, and one-outstanding-request limits remain in force. Every request ID is unique for the process lifetime.

## Implementation checkpoints

- PR #133 adds strict protocol v2 DTOs and lexical validation only; it does not change the production sidecar, which remains protocol 1.0.
- PR #135 adds a separately testable per-process session state machine: one initial hello, process-lifetime unique request IDs, a 4,096-request budget, handshake-required operation gating, and terminal shutdown.
- PR #136 wires a separate strict v2 stdio path alongside the existing v1 path. It implements Rust-generated workspace identity, native adapter registration, bounded capture with bytes/metadata discarded before response, opaque-handle revocation, and registry drop on shutdown/EOF. V2 advertises only implemented read-only capabilities.
- TypeScript client migration, cross-platform end-to-end validation, expanded adversarial lifecycle tests, and independent security review remain gates. The v1 client path remains available during migration.

## 3. Common envelope

Every request contains:

- `protocolVersion`: exact string `"2.0"`
- `messageType`: one of the request types below
- `requestId`: 1–128 ASCII characters from `[A-Za-z0-9._:-]`

Every response echoes the exact request ID and protocol version. Responses are either the matching success type or a typed `error` envelope. Duplicate IDs, malformed envelopes, invalid state transitions, unknown fields, and unsupported versions fail closed.

## 4. Handshake

Request:

```json
{
  "protocolVersion": "2.0",
  "messageType": "hello",
  "requestId": "boot-1",
  "client": { "name": "loco-vscode-extension", "version": "0.1.0" }
}
```

Success response:

```json
{
  "protocolVersion": "2.0",
  "messageType": "hello",
  "requestId": "boot-1",
  "server": {
    "name": "loco-rust-authority",
    "version": "0.1.0",
    "capabilities": ["proposal_evaluation", "workspace_registration", "workspace_capture"]
  }
}
```

The client name/version are compatibility metadata only. They do not authenticate the parent process. Capabilities advertise implemented protocol features, not permission grants. A capability is advertised only when its implementation and required native primitives are available on that process.

## 5. Workspace registration

Request:

```json
{
  "protocolVersion": "2.0",
  "messageType": "registerWorkspace",
  "requestId": "register-1",
  "selectedRoot": "/absolute/path/chosen/by/host",
  "displayName": "project"
}
```

Fields:
- `selectedRoot`: required absolute candidate path, bounded to 4,096 UTF-8 bytes. It is untrusted input, not proof of user selection, identity, or authority.
- `displayName`: optional display-only label, 1–128 UTF-8 bytes when present. It is never used for filesystem access, workspace identity, or authorization.

Rust must generate the internal workspace identity using a cryptographically secure random source. The parent must not choose or override that identity. Rust passes the generated identity and candidate root to the native factory, then transfers the adapter into the bounded process-local registry. Registration fails if capacity is exhausted, entropy is unavailable, the platform is unsupported, or native root acquisition rejects the selection.

Success response:

```json
{
  "protocolVersion": "2.0",
  "messageType": "workspaceRegistered",
  "requestId": "register-1",
  "workspaceHandle": "64-lowercase-hex-characters"
}
```

The handle is an unpredictable 256-bit lookup identifier scoped to one authority process. It is not an OS handle, not a durable ID, not an authenticator, and not a bearer token for any operation outside this process. It becomes invalid on revocation, shutdown, crash, or restart. Do not persist it.

The candidate path must never be inferred from an action proposal or file path. Do not canonicalize a path in the parent and treat that string as proof. No root pathname is returned in the response.

## 6. Bounded file capture

Request:

```json
{
  "protocolVersion": "2.0",
  "messageType": "captureWorkspaceFile",
  "requestId": "capture-1",
  "workspaceHandle": "64-lowercase-hex-characters",
  "relativePath": "src/main.rs",
  "maxBytes": 1048576
}
```

Fields:
- `workspaceHandle`: required handle returned by registration in this process.
- `relativePath`: required portable relative path; reject absolute paths, traversal, ambiguous separators, control characters, platform-reserved names, and unsupported forms.
- `maxBytes`: required integer from 1 through 10,485,760 (10 MiB).

Rust validates the DTO, resolves the handle only through its registry, constructs a `SnapshotCaptureRequest`, and invokes the native adapter. No caller-supplied identity, file kind, content digest, byte length, timestamp, or serialized snapshot metadata is accepted. No pathname fallback is permitted. Unsupported platforms and missing native primitives deny.

Success response:

```json
{
  "protocolVersion": "2.0",
  "messageType": "workspaceFileCaptured",
  "requestId": "capture-1",
  "captured": true,
  "byteLength": 1234
}
```

The acknowledgement reveals only that a bounded regular-file capture completed and its byte length. It does not return file contents, file identity, snapshot digest, capture time, or an approval challenge. The trusted `CapturedWorkspaceSnapshot` remains inside Rust and is dropped after the acknowledgement in this protocol-only milestone. A later approval integration must recapture immediately before use and bind the exact internal action to the fresh capture; it must never accept a digest echoed by the parent as proof.

Capture is not an atomic filesystem snapshot. The adapter's identity/content checks detect important changes but do not freeze the file against concurrent mutation.

## 7. Revocation and shutdown

A `revokeWorkspace` request accepts only a registered opaque handle. Success reports `revoked: true`; an unknown or already-revoked handle returns a generic not-found error. Revocation drops the adapter and retained root handle. No path or native handle is returned.

Shutdown is accepted only after handshake. It drops the registry before acknowledging shutdown and exiting. EOF, malformed protocol termination, process crash, or restart also drops all process-local registrations. Handles never survive process lifetime.

## 8. Error contract

Use stable, non-path-revealing error codes:

- `UNSUPPORTED_PROTOCOL`
- `MALFORMED_MESSAGE`
- `DUPLICATE_REQUEST_ID`
- `HANDSHAKE_REQUIRED`
- `INVALID_WORKSPACE_SELECTION`
- `WORKSPACE_CAPACITY_EXCEEDED`
- `WORKSPACE_NOT_FOUND`
- `INVALID_CAPTURE_REQUEST`
- `CAPTURE_DENIED`
- `REQUEST_LIMIT_EXCEEDED`
- `INTERNAL_ERROR`

Error messages must not disclose absolute paths, volume names, file identities, raw OS errors, or sensitive filesystem metadata. Do not distinguish fabricated, expired, and revoked handles in externally visible text. Do not echo untrusted path strings.

## 9. State machine and ordering

States:
1. `AwaitHello`: accept only a valid 2.0 hello.
2. `Ready`: accept registration, capture, revocation, policy evaluation, and shutdown.
3. `Closed`: no further messages.

No registration, capture, revocation, or evaluation is accepted before handshake. No request is accepted after shutdown. One request is processed at a time; concurrency is not introduced by this protocol. Registration capacity remains bounded by the registry's current hard cap of 16. All handles are process-local.

## 10. Required contract and adversarial tests

### Envelope / lifecycle
- exact version required; 1.0/2.0 downgrade and cross-version messages rejected
- unknown fields, duplicate IDs, malformed UTF-8/JSON, oversized and unterminated frames rejected
- requests before hello and after shutdown rejected
- request count and handle capacity remain bounded
- shutdown, EOF, and process restart invalidate every handle

### Registration
- relative, empty, symlinked, unsupported, traversal, and platform-invalid roots rejected
- Rust generates identity; parent display label cannot influence it
- root is retained by native adapter; rename/replacement does not retarget retained root
- fabricated handles rejected; revocation drops retained adapter
- entropy failure and capacity exhaustion deny without partial registration

### Capture
- capture before registration, fabricated/revoked handle, invalid path, and invalid byte limit rejected
- symlinks/reparse points, hard links where prohibited, directories, special files, oversized files, and mount/volume boundary escapes rejected
- caller-supplied identity, digest, timestamp, or file kind fields rejected as unknown
- response contains no bytes, path, identity, digest, timestamp, or execution token
- adapter failure yields a generic denial without pathname fallback

### Non-execution invariants
- policy `allow` never triggers a file read
- capture never writes, patches, launches a process, or executes a command
- registration/capture responses are not approval or execution capabilities
- no command action is accepted by the capture protocol

## 11. Implementation sequence and acceptance gates

1. Review this 2.0 wire contract and threat model.
2. Add strict Rust DTOs, state-machine tests, and protocol serialization tests without enabling any write/command API.
3. Add registration/capture/revocation handlers backed only by `WorkspaceRegistry` and the native factory.
4. Update the TypeScript client to negotiate the exact version and validate each response; never trust parent-supplied snapshot metadata.
5. Add cross-platform CI and adversarial lifecycle tests.
6. Keep approval issue/consume separate until this protocol is stable; integrate it only as a Rust-internal operation with fresh recapture.
7. Obtain independent security review. The separate platform-authenticator decision remains open.

**Acceptance criterion:** the sidecar can register a candidate workspace, capture a bounded file through its retained native adapter, and revoke the registration while keeping trusted snapshot metadata in Rust. No writes, patches, commands, authenticated-human-approval claims, or execution capabilities are introduced.
